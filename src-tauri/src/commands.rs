use serde::{Serialize};
use tauri::{Manager, State, AppHandle}; // 引入 AppHandle
use image::{ImageBuffer, Rgba};
use std::process::Command as StdCommand;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::settings::{AppSettings, AppState};
use crate::translator;

#[derive(Debug, Serialize, Clone)]
struct TranslationPayload {
    original_text: String,
    translated_text: String,
    error_message: Option<String>,
    image_path: Option<String>,
}

#[tauri::command]
pub async fn process_screenshot_area(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    println!("接收到截图区域: x={}, y={}, width={}, height={}", x, y, width, height);

    let settings_clone = state.settings.lock().unwrap().clone();
    let app_for_task = app.clone();

    tokio::spawn(async move {
        create_and_show_results_window(&app_for_task);

        if let Err(e) = capture_ocr_translate(&app_for_task, settings_clone, x, y, width, height).await {
            eprintln!("处理流程出错: {}", e);
            let payload = TranslationPayload {
                original_text: "处理失败".to_string(),
                translated_text: String::new(),
                error_message: Some(e),
                image_path: None,
            };
            app_for_task.emit_all("translation_result", payload).unwrap();
        }
    });

    Ok(())
}

async fn capture_ocr_translate(
    app: &AppHandle,
    settings: AppSettings,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    // 1. 截图
    let screen = screenshots::Screen::from_point(x as i32, y as i32)
        .map_err(|e| format!("无法找到屏幕: {}", e))?;
    let capture = screen.capture_area(x as i32, y as i32, width as u32, height as u32)
        .map_err(|e| format!("截图失败: {}", e))?;

    // 2. 保存截图到临时文件
    let temp_dir = app.path_resolver().app_cache_dir().unwrap().join("tmp");
    tokio::fs::create_dir_all(&temp_dir).await
        .map_err(|e| format!("创建临时目录失败: {}", e))?;

    let image_path = temp_dir.join("screenshot.png");
    let img_buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(capture.width(), capture.height(), capture.rgba().to_vec())
        .ok_or_else(|| "无法从原始数据创建图像缓冲区".to_string())?;

    img_buffer.save(&image_path)
        .map_err(|e| format!("保存截图文件失败: {}", e))?;

    println!("截图已保存至: {:?}", image_path);

    // 更新全局状态中的最后截图路径
    let state: State<AppState> = app.state();
    {
        let mut last_path = state.last_screenshot_path.lock().unwrap();
        *last_path = Some(image_path.clone());
        println!("最后截图路径已更新。");
    }

    // --- 核心修复：使用 Tauri 的 Path Resolver 来定位打包后的资源 ---
    // 3. 调用本地 OCR
    let ocr_exe_path = app
        .path_resolver()
        .resolve_resource("external/PaddleOCR-json/PaddleOCR-json.exe")
        .ok_or_else(|| "在应用资源中找不到 OCR 可执行文件路径".to_string())?
        .canonicalize() // canonicalize 仍然有用，可以解析为绝对路径
        .map_err(|e| format!("无法找到 or 规范化 OCR 可执行文件路径: {}. 请确认 external/PaddleOCR-json/PaddleOCR-json.exe 文件存在。", e))?;


    if !ocr_exe_path.exists() {
        return Err(format!("错误: OCR 可执行文件在路径 {:?} 下不存在!", ocr_exe_path));
    }

    let ocr_dir = ocr_exe_path.parent().ok_or("无法获取OCR程序的父目录")?;
    let image_path_str = image_path.to_str().unwrap();

    let args = vec![format!("--image_path={}", image_path_str)];

    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut command = StdCommand::new(&ocr_exe_path);
    command.args(&args).current_dir(&ocr_dir);

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let ocr_output = command.output().map_err(|e| format!("执行 OCR 进程失败: {}", e))?;

    if !ocr_output.status.success() {
        let stderr = String::from_utf8_lossy(&ocr_output.stderr);
        return Err(format!("OCR 进程执行出错: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&ocr_output.stdout);
    let json_str = stdout.lines().find(|line| line.starts_with('{')).unwrap_or("{}");

    let ocr_value: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| format!("解析 OCR JSON 失败: {}. 原始输出: {}", e, stdout))?;

    let code = ocr_value["code"].as_i64().unwrap_or(0);

    let original_text = match code {
        100 => { // 成功
            ocr_value["data"].as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|item| item["text"].as_str())
                .map(|s| s.to_string())
                .collect::<Vec<String>>()
                .join(" ")
        },
        101 => { // 未找到文字
            return Err("未识别到任何文字".to_string());
        },
        _ => { // 其他错误
            let error_message = ocr_value["data"].as_str().unwrap_or("OCR 返回未知错误").to_string();
            return Err(error_message);
        }
    };

    if original_text.trim().is_empty() {
        return Err("未识别到任何文字".to_string());
    }

    println!("OCR 识别原文: {}", original_text);

    // 4. 调用翻译 API
    let api_key = settings.api_key;
    let target_lang = settings.target_lang;
    let translated_text: String;

    if api_key.trim().is_empty() {
        translated_text = "未配置翻译API Key，仅显示识别原文。".to_string();
    } else {
        let translator = translator::get_translator(api_key);
        translated_text = translator.translate(&original_text, &target_lang).await?;
        println!("翻译结果: {}", translated_text);
    }

    // 5. 发送结果给前端
    let payload = TranslationPayload {
        original_text,
        translated_text,
        error_message: None,
        image_path: Some(image_path.to_str().unwrap().to_string())
    };

    app.emit_all("translation_result", payload).unwrap();

    Ok(())
}

fn create_and_show_results_window(app: &AppHandle) {
    let handle = app.clone();
    if let Some(window) = handle.get_window("results") {
        window.show().unwrap();
        window.set_focus().unwrap();
    } else {
        tauri::WindowBuilder::new(&handle, "results", tauri::WindowUrl::App("results.html".into()))
            .build()
            .unwrap();
    }
}