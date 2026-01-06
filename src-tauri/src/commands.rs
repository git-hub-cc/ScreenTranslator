use serde::{Serialize};
use tauri::{Manager, State};
use std::process::Command as StdCommand;
use encoding_rs::GBK;
use std::fs;
use base64::{Engine as _, engine::general_purpose};
use std::sync::atomic::Ordering;
use tauri::api::notification::Notification;

use crate::ImageViewerPayload;
use crate::settings::{AppSettings, AppState, LastOcrResult, copy_image_to_clipboard, save_image_to_desktop};
use crate::translator;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

// --- 事件 Payload 定义 ---

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

// --- Tauri 命令定义 ---

/// 处理截图区域的主入口
#[tauri::command]
pub async fn process_screenshot_area(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    x: f64, y: f64, width: f64, height: f64,
) -> Result<(), String> {
    println!("[COMMANDS] 处理截图区域: x={}, y={}, w={}, h={}", x, y, width, height);

    let fullscreen_image = {
        let mut capture_cache = state.fullscreen_capture.lock().unwrap();
        capture_cache.take().ok_or("错误：在 AppState 中未找到缓存的全屏截图。")?
    };

    let cropped_image_buffer = image::imageops::crop_imm(
        &fullscreen_image,
        x as u32, y as u32, width as u32, height as u32,
    ).to_image();

    let settings = state.settings.lock().unwrap().clone();
    let app_for_task = app.clone();

    tokio::spawn(async move {
        let temp_dir = app_for_task.path_resolver().app_cache_dir().unwrap().join("tmp");
        let _ = tokio::fs::create_dir_all(&temp_dir).await;
        let image_path = temp_dir.join("screenshot.png");

        if let Err(e) = cropped_image_buffer.save(&image_path) {
            eprintln!("[COMMANDS] 保存截图失败: {}", e);
            release_lock(&app_for_task);
            return;
        }

        let image_path_str = image_path.to_str().unwrap().to_string();

        {
            let state: State<AppState> = app_for_task.state();
            *state.last_screenshot_path.lock().unwrap() = Some(image_path.clone());
        }

        // 核心分发逻辑
        match settings.primary_action.as_str() {
            "ocr" => handle_ocr_mode(&app_for_task, &image_path_str, &settings, false).await,
            "ocr_translate" => handle_ocr_mode(&app_for_task, &image_path_str, &settings, true).await,
            "copy" => handle_copy_mode(&app_for_task, image_path_str).await,
            "save" => handle_save_mode(&app_for_task, image_path_str).await,
            "preview" | _ => handle_preview_mode(&app_for_task, &image_path, image_path_str).await,
        }

        release_lock(&app_for_task);
    });

    Ok(())
}

/// 手动处理图片指令
#[tauri::command]
pub async fn process_image_from_path(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
    action: String
) -> Result<(), String> {
    println!("[COMMANDS] 手动处理图片: {}, 动作: {}", path, action);
    let settings = state.settings.lock().unwrap().clone();

    if action == "ocr" {
        handle_ocr_mode(&app, &path, &settings, false).await;
        let app_handle = app.clone();
        app.run_on_main_thread(move || {
            crate::show_results_window_with_cache(&app_handle);
        }).unwrap();
    }
    Ok(())
}

// --- 内部处理逻辑 ---

async fn handle_copy_mode(app: &tauri::AppHandle, path: String) {
    match copy_image_to_clipboard(path).await {
        Ok(_) => send_notification(app, "✅ 复制成功", "截图已复制到剪贴板。"),
        Err(e) => send_notification(app, "❌ 复制失败", &e),
    }
}

async fn handle_save_mode(app: &tauri::AppHandle, path: String) {
    match save_image_to_desktop(path).await {
        Ok(_) => send_notification(app, "✅ 保存成功", "截图已保存到桌面。"),
        Err(e) => send_notification(app, "❌ 保存失败", &e),
    }
}

async fn handle_preview_mode(app: &tauri::AppHandle, path: &std::path::Path, path_str: String) {
    if let Ok(bytes) = fs::read(path) {
        let b64 = general_purpose::STANDARD.encode(&bytes);
        let payload = ImageViewerPayload {
            image_data_url: format!("data:image/png;base64,{}", b64),
            image_path: path_str,
        };
        create_and_show_image_viewer_window(app, payload);
    } else {
        send_notification(app, "❌ 错误", "无法读取截图文件进行预览。");
    }
}

async fn handle_ocr_mode(
    app: &tauri::AppHandle,
    image_path: &str,
    settings: &AppSettings,
    do_translate: bool
) {
    let ocr_res = perform_ocr(app, image_path, settings);

    match ocr_res {
        Ok(text) => {
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(text.clone());
            }

            if !do_translate {
                send_notification(app, "✅ 文字识别成功", "内容已复制到剪贴板。");
                cache_result(app, Some(text), None, image_path.to_string());
            } else {
                let translator = translator::get_translator(app);
                let trans_res = translator.translate(&text, &settings.target_lang).await;

                match trans_res {
                    Ok(trans_text) => {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(trans_text.clone());
                        }
                        send_notification(app, "✅ 翻译完成", "译文已复制。按 Win+V 查看原文。");
                        cache_result(app, Some(text), Some(trans_text), image_path.to_string());
                    },
                    Err(e) => {
                        send_notification(app, "⚠️ 翻译失败", &format!("OCR成功但翻译出错: {}", e));
                        cache_result(app, Some(text), None, image_path.to_string());
                    }
                }
            }
        },
        Err(e) => {
            send_notification(app, "❌ 识别失败", &format!("出错: {}", e));
            cache_result(app, None, None, image_path.to_string());
        }
    }
}

// --- 辅助函数 ---

fn release_lock(app: &tauri::AppHandle) {
    let state: State<AppState> = app.state();
    state.is_capturing.store(false, Ordering::SeqCst);
}

fn cache_result(app: &tauri::AppHandle, original: Option<String>, translated: Option<String>, path: String) {
    let state: State<AppState> = app.state();
    let mut cache = state.last_ocr_result.lock().unwrap();
    *cache = Some(LastOcrResult {
        original_text: original,
        translated_text: translated,
        image_path: path,
    });
}

fn send_notification(app: &tauri::AppHandle, title: &str, body: &str) {
    let _ = Notification::new(&app.config().tauri.bundle.identifier)
        .title(title)
        .body(body)
        .show();
}

fn perform_ocr(app: &tauri::AppHandle, image_path_str: &str, settings: &AppSettings) -> Result<String, String> {
    let ocr_exe_path = app.path_resolver().resolve_resource("external/PaddleOCR-json/PaddleOCR-json.exe")
        .ok_or_else(|| "无法找到 OCR 程序".to_string())?.canonicalize().map_err(|e| format!("{}", e))?;
    let ocr_dir = ocr_exe_path.parent().ok_or("无法获取OCR目录")?;
    #[cfg(windows)] const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut command = StdCommand::new(&ocr_exe_path);
    command.args(&[format!("--image_path={}", image_path_str)]).current_dir(&ocr_dir);
    #[cfg(windows)] command.creation_flags(CREATE_NO_WINDOW);

    let ocr_output = command.output().map_err(|e| format!("{}", e))?;
    if !ocr_output.status.success() {
        let stderr = GBK.decode(&ocr_output.stderr).0.into_owned();
        return Err(format!("OCR错误: {}", stderr));
    }

    let stdout = GBK.decode(&ocr_output.stdout).0.into_owned();
    let json_start = stdout.lines().find(|line| line.starts_with('{')).unwrap_or("{}");
    let ocr_value: serde_json::Value = serde_json::from_str(json_start).map_err(|e| format!("{}", e))?;

    if ocr_value["code"].as_i64().unwrap_or(0) == 100 {
        let separator = if settings.preserve_line_breaks { "\n" } else { " " };
        let text = ocr_value["data"].as_array().unwrap_or(&vec![]).iter()
            .filter_map(|item| item["text"].as_str()).collect::<Vec<_>>().join(separator);
        if text.trim().is_empty() { Err("未识别到文字".to_string()) } else { Ok(text) }
    } else {
        Err(ocr_value["data"].as_str().unwrap_or("未知错误").to_string())
    }
}

fn create_and_show_image_viewer_window(app: &tauri::AppHandle, payload: ImageViewerPayload) {
    let handle = app.clone();
    let handle_for_closure = handle.clone(); // 解决 E0505
    handle.run_on_main_thread(move || {
        if let Some(window) = handle_for_closure.get_window("image_viewer") {
            window.emit("display-image", payload).unwrap();
            window.show().unwrap(); window.set_focus().unwrap();
        } else {
            let builder = tauri::WindowBuilder::new(&handle_for_closure, "image_viewer", tauri::WindowUrl::App("image_viewer.html".into()))
                .title("截图预览").decorations(false).transparent(true).resizable(true).skip_taskbar(true).visible(false);
            if let Ok(window) = builder.build() {
                let window_clone = window.clone();
                window.once("tauri://created", move |_| {
                    window_clone.emit("display-image", payload).unwrap();
                });
            }
        }
    }).unwrap();
}