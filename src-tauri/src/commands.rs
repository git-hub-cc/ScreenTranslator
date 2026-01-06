use serde::{Serialize};
use tauri::{Manager, State};
use std::process::Command as StdCommand;
use encoding_rs::GBK;
use std::fs;
use base64::{Engine as _, engine::general_purpose};
// --- 新增：引入 Ordering 以配合 AtomicBool 使用 ---
use std::sync::atomic::Ordering;

use crate::ImageViewerPayload;
use crate::settings::{AppSettings, AppState};
use crate::translator;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

// 事件 Payload 结构体定义 (无修改)
#[derive(Debug, Serialize, Clone)]
struct OcrPayload {
    original_text: Option<String>,
    error_message: Option<String>,
    image_path: String,
}

#[derive(Debug, Serialize, Clone)]
struct TranslationUpdatePayload {
    translated_text: Option<String>,
    error_message: Option<String>,
}


/// Tauri 命令：处理前端发送的截图区域坐标。
#[tauri::command]
pub async fn process_screenshot_area(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    println!("[COMMANDS] 接收到截图区域: x={}, y={}, width={}, height={}", x, y, width, height);

    let fullscreen_image = {
        let mut capture_cache = state.fullscreen_capture.lock().unwrap();
        // --- 日志：检查缓存的截图 ---
        match capture_cache.is_some() {
            true => println!("[COMMANDS] 成功从 AppState 获取缓存的全屏截图。"),
            false => eprintln!("[COMMANDS] 错误：在 AppState 中未找到缓存的全屏截图。"),
        }
        capture_cache.take().ok_or("错误：在 AppState 中未找到缓存的全屏截图。")?
    };

    let cropped_image_buffer = image::imageops::crop_imm(
        &fullscreen_image,
        x as u32,
        y as u32,
        width as u32,
        height as u32,
    ).to_image();
    println!("[COMMANDS] 已根据坐标裁剪图像，尺寸: {}x{}", cropped_image_buffer.width(), cropped_image_buffer.height());


    let settings = state.settings.lock().unwrap().clone();
    let app_for_task = app.clone();

    // --- 核心修改：在异步任务的末尾释放锁 ---
    // 将后续的耗时操作（保存、OCR、翻译）放入一个独立的异步任务中。
    println!("[COMMANDS] 准备启动独立的异步任务处理 OCR/翻译...");
    tokio::spawn(async move {
        // 执行核心的保存、OCR和翻译流程
        if let Err(e) = save_ocr_translate(&app_for_task, settings, cropped_image_buffer).await {
            eprintln!("[COMMANDS] [异步任务] 处理流程出现严重错误: {}", e);
        }

        // --- 关键修复：无论成功还是失败，在所有操作完成后，获取状态并释放锁 ---
        let state: State<AppState> = app_for_task.state();
        println!("[COMMANDS] [异步任务] OCR/翻译流程完成，释放截图锁。");
        // 将 is_capturing 标志安全地设回 false，以允许下一次截图
        state.is_capturing.store(false, Ordering::SeqCst);
    });

    Ok(())
}


/// 完整的“保存 -> OCR -> 翻译”流程
async fn save_ocr_translate(
    app: &tauri::AppHandle,
    settings: AppSettings,
    cropped_image: image::RgbaImage,
) -> Result<(), String> {

    // 步骤 1: 将裁剪后的图像保存到临时文件
    let temp_dir = app.path_resolver().app_cache_dir().unwrap().join("tmp");
    tokio::fs::create_dir_all(&temp_dir).await
        .map_err(|e| format!("创建临时目录失败: {}", e))?;
    let image_path = temp_dir.join("screenshot.png");

    cropped_image.save(&image_path)
        .map_err(|e| format!("保存裁剪后的截图文件失败: {}", e))?;
    println!("[COMMANDS] [异步任务] 裁剪后的截图已保存至: {:?}", image_path);

    let state: State<AppState> = app.state();
    {
        let mut last_path = state.last_screenshot_path.lock().unwrap();
        *last_path = Some(image_path.clone());
    }
    let image_path_str = image_path.to_str().unwrap().to_string();

    // 步骤 2: 根据 OCR 开关状态执行不同逻辑
    if settings.enable_ocr {
        println!("[COMMANDS] [异步任务] OCR 功能已开启，执行识别流程...");
        create_and_show_results_window(app);

        let ocr_result = perform_ocr(app, &image_path_str, &settings);

        match ocr_result {
            Ok(original_text) => {
                println!("[COMMANDS] [异步任务] OCR 成功。准备向前端发送 'ocr_result' 事件。");
                app.emit_all("ocr_result", OcrPayload {
                    original_text: Some(original_text.clone()),
                    error_message: None,
                    image_path: image_path_str,
                }).unwrap();

                if settings.enable_translation {
                    println!("[COMMANDS] [异步任务] 翻译功能已开启，开始翻译...");
                    let translator = translator::get_translator(app);
                    let translation_result = translator.translate(&original_text, &settings.target_lang).await;

                    println!("[COMMANDS] [异步任务] 翻译完成。准备向前端发送 'translation_update' 事件。");
                    app.emit_all("translation_update", match translation_result {
                        Ok(translated_text) => {
                            println!("[COMMANDS] [异步任务] 翻译成功: {}", translated_text);
                            TranslationUpdatePayload {
                                translated_text: Some(translated_text),
                                error_message: None,
                            }
                        },
                        Err(e) => {
                            eprintln!("[COMMANDS] [异步任务] 翻译失败: {}", e);
                            TranslationUpdatePayload {
                                translated_text: None,
                                error_message: Some(e),
                            }
                        }
                    }).unwrap();
                } else {
                    println!("[COMMANDS] [异步任务] 翻译功能已关闭，跳过翻译步骤。");
                    app.emit_all("translation_update", TranslationUpdatePayload {
                        translated_text: None,
                        error_message: Some("翻译功能已关闭".to_string()),
                    }).unwrap();
                }
            },
            Err(e) => { // OCR 失败
                eprintln!("[COMMANDS] [异步任务] OCR 失败: {}", e);
                println!("[COMMANDS] [异步任务] OCR 失败。准备向前端发送带错误信息的 'ocr_result' 事件。");
                app.emit_all("ocr_result", OcrPayload {
                    original_text: Some("识别失败".to_string()),
                    error_message: Some(e),
                    image_path: image_path_str,
                }).unwrap();
            }
        };

    } else { // 如果OCR关闭，则只显示图片预览
        println!("[COMMANDS] [异步任务] OCR 功能已关闭，仅显示截图预览。");
        let bytes = fs::read(&image_path).map_err(|e| format!("读取截图文件失败: {}", e))?;
        let b64 = general_purpose::STANDARD.encode(&bytes);
        let payload = ImageViewerPayload {
            image_data_url: format!("data:image/png;base64,{}", b64),
            image_path: image_path_str,
        };
        create_and_show_image_viewer_window(app, payload);
    }

    Ok(())
}

// --- 辅助函数 (添加日志) ---

fn perform_ocr(app: &tauri::AppHandle, image_path_str: &str, settings: &AppSettings) -> Result<String, String> {
    let ocr_exe_path = app
        .path_resolver()
        .resolve_resource("external/PaddleOCR-json/PaddleOCR-json.exe")
        .ok_or_else(|| "在应用资源中找不到 OCR 可执行文件路径".to_string())?
        .canonicalize()
        .map_err(|e| format!("无法找到 or 规范化 OCR 可执行文件路径: {}. 请确认 external/PaddleOCR-json/PaddleOCR-json.exe 文件存在。", e))?;

    if !ocr_exe_path.exists() { return Err(format!("错误: OCR 可执行文件在路径 {:?} 下不存在!", ocr_exe_path)); }

    let ocr_dir = ocr_exe_path.parent().ok_or("无法获取OCR程序的父目录")?;
    let args = vec![format!("--image_path={}", image_path_str)];
    println!("[OCR] 准备执行OCR命令: {:?} with args: {:?}", ocr_exe_path, args);

    #[cfg(windows)] const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut command = StdCommand::new(&ocr_exe_path);
    command.args(&args).current_dir(&ocr_dir);
    #[cfg(windows)] command.creation_flags(CREATE_NO_WINDOW);

    let ocr_output = command.output().map_err(|e| format!("执行 OCR 进程失败: {}", e))?;

    if !ocr_output.status.success() {
        let stderr = GBK.decode(&ocr_output.stderr).0.into_owned();
        eprintln!("[OCR] 进程执行出错, Status: {:?}, Stderr: {}", ocr_output.status, stderr);
        return Err(format!("OCR 进程执行出错: {}", stderr));
    }

    let stdout = GBK.decode(&ocr_output.stdout).0.into_owned();
    println!("[OCR] 原始输出 (stdout): {}", stdout);

    let json_str = stdout.lines().find(|line| line.starts_with('{')).unwrap_or("{}");
    let ocr_value: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| format!("解析 OCR JSON 失败: {}. 原始输出: {}", e, stdout))?;
    println!("[OCR] 解析到的JSON数据: {:?}", ocr_value);

    let code = ocr_value["code"].as_i64().unwrap_or(0);
    let separator = if settings.preserve_line_breaks { "\n" } else { " " };

    let original_text = match code {
        100 => ocr_value["data"].as_array().unwrap_or(&vec![]).iter()
            .filter_map(|item| item["text"].as_str()).map(|s| s.to_string())
            .collect::<Vec<String>>().join(separator),
        101 => return Err("未识别到任何文字".to_string()),
        _ => return Err(ocr_value["data"].as_str().unwrap_or("OCR 返回未知错误").to_string()),
    };

    if original_text.trim().is_empty() { return Err("未识别到任何文字".to_string()); }

    println!("[OCR] 识别原文: {}", original_text);
    Ok(original_text)
}

fn create_and_show_results_window(app: &tauri::AppHandle) {
    let handle = app.clone();
    if let Some(window) = handle.get_window("results") {
        println!("[WINDOW] 'results' 窗口已存在，直接显示。");
        window.show().unwrap();
        window.set_focus().unwrap();
    } else {
        println!("[WINDOW] 'results' 窗口不存在，创建新窗口。");
        tauri::WindowBuilder::new(&handle, "results", tauri::WindowUrl::App("results.html".into()))
            .build()
            .expect("无法创建结果窗口");
    }
}

fn create_and_show_image_viewer_window(app: &tauri::AppHandle, payload: ImageViewerPayload) {
    let handle = app.clone();
    let handle_for_closure = handle.clone();
    handle.run_on_main_thread(move || {
        if let Some(window) = handle_for_closure.get_window("image_viewer") {
            println!("[WINDOW] 'image_viewer' 窗口已存在，发送 display-image 事件。");
            window.emit("display-image", payload).unwrap();
            window.show().unwrap();
            window.set_focus().unwrap();
        } else {
            println!("[WINDOW] 'image_viewer' 窗口不存在，创建新窗口。");
            let builder = tauri::WindowBuilder::new(&handle_for_closure, "image_viewer", tauri::WindowUrl::App("image_viewer.html".into()))
                .title("截图预览").decorations(false).transparent(true)
                .resizable(true).skip_taskbar(true).visible(false);

            if let Ok(window) = builder.build() {
                let window_for_closure = window.clone();
                window.once("tauri://created", move |_| {
                    println!("[WINDOW] 'image_viewer' 窗口创建完成，发送 display-image 事件。");
                    window_for_closure.emit("display-image", payload).unwrap();
                });
            }
        }
    }).unwrap();
}