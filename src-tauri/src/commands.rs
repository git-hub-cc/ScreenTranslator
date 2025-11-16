use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use image::{ImageBuffer, Rgba};
use std::process::Command as StdCommand;

// 引入我们自己的模块
use crate::settings::{AppSettings, AppState}; // 修正：引入 AppSettings
use crate::translator;

// --- 1. 数据结构定义 ---

// 用于解析PaddleOCR-json输出的结构体
#[derive(Debug, Deserialize)]
struct OcrResult {
    code: i32,
    message: Option<String>,
    data: Option<Vec<OcrData>>,
}

#[derive(Debug, Deserialize)]
struct OcrData {
    text: String,
}

// 用于发送给前端的事件负载 (payload) 结构体
#[derive(Debug, Serialize, Clone)]
struct TranslationPayload {
    original_text: String,
    translated_text: String,
    error_message: Option<String>,
}

// --- 2. Tauri 指令 ---

/// [Tauri指令] 处理截图区域的核心入口函数
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

    // --- 核心修正：生命周期问题解决方案 ---
    // 在创建新线程之前，就从 `State` 中克隆出需要的数据。
    // `AppSettings` 结构体我们已经派生了 `Clone` trait，所以可以直接克隆。
    let settings_clone = state.settings.lock().unwrap().clone();

    // 将耗时任务放到一个新的异步线程中执行，防止UI线程被阻塞
    tokio::spawn(async move {
        // 首先创建并显示结果窗口，让用户立即看到反馈
        create_and_show_results_window(&app);

        // 调用核心处理流程，将克隆出来的 `settings_clone` 传入
        if let Err(e) = capture_ocr_translate(&app, settings_clone, x, y, width, height).await {
            eprintln!("处理流程出错: {}", e);
            // 如果出错，向前端发送一个包含错误信息的事件
            let payload = TranslationPayload {
                original_text: "处理失败".to_string(),
                translated_text: String::new(),
                error_message: Some(e),
            };
            app.emit_all("translation_result", payload).unwrap();
        }
    });

    Ok(())
}


// --- 3. 核心处理流程 ---

/// 核心函数：截图 -> OCR -> 翻译
// 修正：参数不再是 `State` 的引用，而是拥有所有权的 `AppSettings`
async fn capture_ocr_translate(
    app: &tauri::AppHandle,
    settings: AppSettings, // 接收克隆后的设置
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    // --- 步骤 1: 截图 ---
    let screen = screenshots::Screen::from_point(x as i32, y as i32)
        .map_err(|e| format!("无法找到屏幕: {}", e))?;
    let capture = screen.capture_area(x as i32, y as i32, width as u32, height as u32)
        .map_err(|e| format!("截图失败: {}", e))?;

    // --- 步骤 2: 保存到临时文件 ---
    let temp_dir = app.path_resolver().app_cache_dir().unwrap().join("tmp");
    tokio::fs::create_dir_all(&temp_dir).await
        .map_err(|e| format!("创建临时目录失败: {}", e))?;
    let image_path = temp_dir.join("screenshot.png");
    let img_buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(capture.width(), capture.height(), capture.rgba().to_vec())
        .ok_or_else(|| "无法从原始数据创建图像缓冲区".to_string())?;
    img_buffer.save(&image_path)
        .map_err(|e| format!("保存截图文件失败: {}", e))?;
    println!("截图已保存至: {:?}", image_path);

    // --- 步骤 3: 执行 OCR ---
    // 使用 tauri 的 API 来安全地解析资源路径，这在开发和打包后都能正常工作
    let cwd = std::env::current_dir().map_err(|e| format!("无法获取当前工作目录: {}", e))?;
    let ocr_exe_path = cwd
        .join("..")
        .join("external")
        .join("PaddleOCR-json")
        .join("PaddleOCR-json.exe")
        .canonicalize()
        .map_err(|e| format!("无法找到或规范化 OCR 可执行文件路径: {}. 请确认 external/PaddleOCR-json/PaddleOCR-json.exe 文件存在。", e))?;

    if !ocr_exe_path.exists() {
        return Err(format!("错误: OCR 可执行文件在路径 {:?} 下不存在!", ocr_exe_path));
    }

    let ocr_dir = ocr_exe_path.parent().ok_or("无法获取OCR程序的父目录")?;
    let image_path_str = image_path.to_str().unwrap();
    let args = vec![format!("--image_path={}", image_path_str)];

    let ocr_output = StdCommand::new(&ocr_exe_path)
        .args(&args)
        .current_dir(&ocr_dir) // 必须设置工作目录，否则OCR可能找不到模型文件
        .output()
        .map_err(|e| format!("执行 OCR 进程失败: {}", e))?;

    if !ocr_output.status.success() {
        let stderr = String::from_utf8_lossy(&ocr_output.stderr);
        return Err(format!("OCR 进程执行出错: {}", stderr));
    }

    // 解析OCR输出
    let stdout = String::from_utf8_lossy(&ocr_output.stdout);
    let json_str = stdout.lines().find(|line| line.starts_with('{')).unwrap_or("");
    let ocr_result: OcrResult = serde_json::from_str(json_str)
        .map_err(|e| format!("解析 OCR 结果失败: {}. 原始输出: {}", e, stdout))?;

    if ocr_result.code != 100 {
        return Err(ocr_result.message.unwrap_or_else(|| "OCR 返回了未知错误".to_string()));
    }

    // 将所有识别出的文本片段连接成一个字符串
    let original_text = ocr_result.data
        .unwrap_or_default()
        .into_iter()
        .map(|d| d.text)
        .collect::<Vec<String>>()
        .join(" ");

    if original_text.trim().is_empty() {
        return Err("未识别到任何文字".to_string());
    }
    println!("OCR 识别原文: {}", original_text);

    // --- 步骤 4: 调用翻译 ---
    // 直接使用传入的 settings 克隆体
    let api_key = settings.api_key;
    let target_lang = settings.target_lang;

    let translated_text: String;
    if api_key.trim().is_empty() {
        translated_text = "未配置翻译API Key，仅显示识别原文。".to_string();
    } else {
        // 使用translator模块进行翻译
        let translator = translator::get_translator(api_key); // 修正：直接传递api_key
        translated_text = translator.translate(&original_text, &target_lang).await?;
        println!("翻译结果: {}", translated_text);
    }

    // --- 步骤 5: 发送最终结果给前端 ---
    let payload = TranslationPayload {
        original_text,
        translated_text,
        error_message: None,
    };
    app.emit_all("translation_result", payload).unwrap();

    Ok(())
}

// --- 4. 辅助函数 ---

/// 辅助函数：创建并显示结果窗口
fn create_and_show_results_window(app: &tauri::AppHandle) {
    let handle = app.clone();
    // 检查窗口是否已存在
    if let Some(window) = handle.get_window("results") {
        window.show().unwrap();
        window.set_focus().unwrap();
    } else {
        // 如果不存在则创建
        tauri::WindowBuilder::new(
            &handle,
            "results",
            tauri::WindowUrl::App("results.html".into())
        )
            .build()
            .unwrap();
    }
}