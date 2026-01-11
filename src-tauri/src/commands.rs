// --- 文件: src-tauri/src/commands.rs ---

use serde::{Serialize};
use tauri::{Manager, State};
use std::process::Command as StdCommand;
use std::fs;
use std::io::Write;
use base64::{Engine as _, engine::general_purpose};
use std::sync::atomic::Ordering;
use tauri::api::notification::Notification;
use futures_util::StreamExt;
use std::path::{Path, PathBuf};

use crate::ImageViewerPayload;
use crate::settings::{AppSettings, AppState, LastOcrResult, copy_image_to_clipboard, save_image_to_desktop};
use crate::translator;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

// --- 事件 Payload 定义 ---
#[derive(Clone, Serialize)]
struct DownloadProgressPayload {
    progress: u64,
    total: u64,
    status: String,
}

// --- 常量定义 ---
// OCR 引擎 (RapidOCR)
const OCR_URL: &str = "https://github.com/hiroi-sora/RapidOCR-json/releases/download/v0.2.0/RapidOCR-json_v0.2.0.7z";
const OCR_EXE_NAME: &str = "RapidOCR-json.exe";
// 定义解压后的子目录名
const OCR_DIR_NAME: &str = "RapidOCR-json_v0.2.0";

// 翻译引擎 (LocalTranslator)
// 修改：升级到 0.2.0 版本，使用 7z 格式
const TRANSLATOR_URL: &str = "https://github.com/git-hub-cc/LocalTranslator/releases/download/V0.2.0/LocalTranslator-0.2.0.7z";
const TRANSLATOR_EXE_NAME: &str = "translate_engine.exe";

// --- Tauri 命令定义 ---

// --- OCR 引擎管理 ---
#[tauri::command]
pub async fn check_ocr_status(app: tauri::AppHandle) -> Result<bool, String> {
    let local_data_dir = app.path_resolver().app_local_data_dir()
        .ok_or("无法获取本地数据目录")?;
    // 在路径中加入子目录
    let exe_path = local_data_dir.join(OCR_DIR_NAME).join(OCR_EXE_NAME);
    let exists = exe_path.exists();
    println!("[STATUS] 检查 OCR 状态: 路径='{:?}', 是否存在={}", exe_path, exists);
    Ok(exists)
}

#[tauri::command]
pub async fn download_ocr(app: tauri::AppHandle) -> Result<(), String> {
    println!("[DOWNLOAD_OCR] 开始下载 OCR 引擎...");
    let window = app.get_window("main").ok_or("找不到主窗口")?;
    let local_data_dir = app.path_resolver().app_local_data_dir().ok_or("无法获取本地数据目录")?;
    println!("[DOWNLOAD_OCR] 本地数据目录: {:?}", local_data_dir);
    if !local_data_dir.exists() {
        println!("[DOWNLOAD_OCR] 目录不存在，正在创建...");
        fs::create_dir_all(&local_data_dir).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    let archive_path = local_data_dir.join("ocr.7z");
    println!("[DOWNLOAD_OCR] 存档将保存到: {:?}", archive_path);

    // 1. 下载文件
    println!("[DOWNLOAD_OCR] 正在从 URL 下载: {}", OCR_URL);
    let client = reqwest::Client::new();
    let res = client.get(OCR_URL).send().await.map_err(|e| {
        let err_msg = format!("请求失败: {}", e);
        println!("[DOWNLOAD_OCR] 错误: {}", err_msg);
        err_msg
    })?;
    let total_size = res.content_length().unwrap_or(0);
    println!("[DOWNLOAD_OCR] 文件总大小: {} bytes", total_size);

    let mut downloaded: u64 = 0;
    let mut stream = res.bytes_stream();
    let mut file = fs::File::create(&archive_path).map_err(|e| {
        let err_msg = format!("创建文件失败: {}", e);
        println!("[DOWNLOAD_OCR] 错误: {}", err_msg);
        err_msg
    })?;

    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| {
            let err_msg = format!("下载流出错: {}", e);
            println!("[DOWNLOAD_OCR] 错误: {}", err_msg);
            err_msg
        })?;
        file.write_all(&chunk).map_err(|e| {
            let err_msg = format!("写入文件块失败: {}", e);
            println!("[DOWNLOAD_OCR] 错误: {}", err_msg);
            err_msg
        })?;
        downloaded += chunk.len() as u64;
        window.emit("ocr-download-progress", DownloadProgressPayload {
            progress: downloaded, total: total_size, status: "downloading".to_string(),
        }).unwrap_or(());
    }
    println!("[DOWNLOAD_OCR] 下载完成. 总共下载 {} bytes", downloaded);

    // 2. 解压文件 (.7z)
    println!("[DOWNLOAD_OCR] 开始解压文件: {:?}", archive_path);
    window.emit("ocr-download-progress", DownloadProgressPayload {
        progress: total_size, total: total_size, status: "extracting".to_string(),
    }).unwrap_or(());

    sevenz_rust::decompress_file(&archive_path, &local_data_dir)
        .map_err(|e| {
            let err_msg = format!("解压7z文件失败: {:?}", e);
            println!("[DOWNLOAD_OCR] 错误: {}", err_msg);
            err_msg
        })?;
    println!("[DOWNLOAD_OCR] 解压成功到: {:?}", local_data_dir);

    // 3. 清理并通知完成
    println!("[DOWNLOAD_OCR] 删除临时存档: {:?}", archive_path);
    let _ = fs::remove_file(archive_path);
    println!("[DOWNLOAD_OCR] OCR 引擎安装流程完成.");
    window.emit("ocr-download-progress", DownloadProgressPayload {
        progress: total_size, total: total_size, status: "completed".to_string(),
    }).unwrap_or(());

    Ok(())
}


// --- 翻译引擎管理 ---
#[tauri::command]
pub async fn check_translator_status(app: tauri::AppHandle) -> Result<bool, String> {
    let local_data_dir = app.path_resolver().app_local_data_dir()
        .ok_or("无法获取本地数据目录")?;
    // 注意：如果新版 7z 解压后有一层文件夹（如 LocalTranslator-0.2.0），可能需要调整此路径
    // 目前保持原逻辑，假设 exe 直接或间接位于我们预期的位置
    let exe_path = local_data_dir.join(TRANSLATOR_EXE_NAME);
    Ok(exe_path.exists())
}

#[tauri::command]
pub async fn download_translator(app: tauri::AppHandle) -> Result<(), String> {
    let window = app.get_window("main").ok_or("找不到主窗口")?;
    let local_data_dir = app.path_resolver().app_local_data_dir().ok_or("无法获取本地数据目录")?;
    if !local_data_dir.exists() {
        fs::create_dir_all(&local_data_dir).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    // 修改：文件后缀改为 .7z
    let archive_path = local_data_dir.join("translator.7z");

    // 1. 下载文件
    println!("[DOWNLOAD_TRANS] 正在从 URL 下载: {}", TRANSLATOR_URL);
    let client = reqwest::Client::new();
    let res = client.get(TRANSLATOR_URL).send().await.map_err(|e| format!("请求失败: {}", e))?;
    let total_size = res.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut stream = res.bytes_stream();
    let mut file = fs::File::create(&archive_path).map_err(|e| format!("创建文件失败: {}", e))?;

    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| format!("下载出错: {}", e))?;
        file.write_all(&chunk).map_err(|e| format!("写入文件失败: {}", e))?;
        downloaded += chunk.len() as u64;
        window.emit("download-progress", DownloadProgressPayload {
            progress: downloaded, total: total_size, status: "downloading".to_string(),
        }).unwrap_or(());
    }

    // 2. 解压文件 (.7z) - 修改为使用 sevenz_rust
    window.emit("download-progress", DownloadProgressPayload {
        progress: total_size, total: total_size, status: "extracting".to_string(),
    }).unwrap_or(());

    // 使用 sevenz-rust 进行解压，替代原来的 zip 逻辑
    sevenz_rust::decompress_file(&archive_path, &local_data_dir)
        .map_err(|e| format!("解压7z文件失败: {:?}", e))?;

    // 3. 清理并通知完成
    let _ = fs::remove_file(archive_path);
    window.emit("download-progress", DownloadProgressPayload {
        progress: total_size, total: total_size, status: "completed".to_string(),
    }).unwrap_or(());
    Ok(())
}

// --- 核心功能命令 ---

// 处理用户取消截图的命令
#[tauri::command]
pub fn cancel_screenshot(state: State<'_, AppState>) {
    println!("[COMMANDS] 用户取消截图，释放锁。");
    state.is_capturing.store(false, Ordering::SeqCst);
}

#[tauri::command]
pub async fn process_screenshot_area(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    x: f64, y: f64, width: f64, height: f64,
) -> Result<(), String> {
    println!("[COMMANDS] 处理截图区域: x={}, y={}, w={}, h={}", x, y, width, height);

    if let Some(loading_window) = app.get_window("loading") {
        let _ = loading_window.center();
        let _ = loading_window.show();
    }

    let fullscreen_image = {
        let mut capture_cache = state.fullscreen_capture.lock().unwrap();
        capture_cache.take().ok_or("错误：在 AppState 中未找到缓存的全屏截图。")?
    };

    let cropped_image_buffer = image::imageops::crop_imm(
        &fullscreen_image, x as u32, y as u32, width as u32, height as u32,
    ).to_image();

    let settings = state.settings.lock().unwrap().clone();
    let app_for_task = app.clone();

    tokio::spawn(async move {
        let temp_dir = app_for_task.path_resolver().app_cache_dir().unwrap().join("tmp");
        let _ = tokio::fs::create_dir_all(&temp_dir).await;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let image_filename = format!("screenshot-{}.png", timestamp);
        let image_path = temp_dir.join(image_filename);

        if let Err(e) = cropped_image_buffer.save(&image_path) {
            eprintln!("[COMMANDS] 保存截图失败: {}", e);
            hide_loading_and_release_lock(&app_for_task);
            return;
        }

        let image_path_str = image_path.to_str().unwrap().to_string();

        add_image_to_history(&app_for_task.state(), image_path.clone());

        match settings.primary_action.as_str() {
            "ocr" => handle_ocr_mode(&app_for_task, &image_path_str, &settings, false).await,
            "ocr_translate" => handle_ocr_mode(&app_for_task, &image_path_str, &settings, true).await,
            "copy" => handle_copy_mode(&app_for_task, image_path_str).await,
            "save" => handle_save_mode(&app_for_task, image_path_str).await,
            "preview" | _ => handle_preview_mode(&app_for_task, &image_path, image_path_str).await,
        }

        hide_loading_and_release_lock(&app_for_task);
    });

    Ok(())
}

#[tauri::command]
pub async fn process_image_from_path(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
    action: String
) -> Result<(), String> {
    println!("[COMMANDS] 手动处理图片: {}, 动作: {}", path, action);
    let settings = state.settings.lock().unwrap().clone();

    let do_translate = match action.as_str() {
        "ocr_translate" => true,
        "ocr" => false,
        _ => {
            println!("[COMMANDS] 未知动作: '{}', 操作已取消。", action);
            return Ok(());
        }
    };

    handle_ocr_mode(&app, &path, &settings, do_translate).await;

    let app_handle_for_main_thread = app.clone();
    app.run_on_main_thread(move || {
        crate::show_results_window_with_cache(&app_handle_for_main_thread);
    }).map_err(|e| format!("无法在主线程上运行任务: {}", e))?;

    Ok(())
}

// --- 辅助函数 ---

/// [核心修改] 处理通过文件关联打开的外部图片
///
/// 此函数将文件I/O操作（复制、读取）放到后台线程中，避免阻塞UI。
/// 完成后，在主线程上显示预览窗口。
pub fn handle_external_image_open(app: &tauri::AppHandle, external_path: &Path) {
    println!("[COMMANDS] 检测到通过文件关联打开图片: {:?}", external_path);

    // 克隆需要在新线程中使用的数据
    let app_handle = app.clone();
    let path_buf = external_path.to_path_buf();

    // 启动一个后台线程来处理耗时的文件操作
    std::thread::spawn(move || {
        // 1. 获取应用的缓存目录
        let cache_dir = match app_handle.path_resolver().app_cache_dir() {
            Some(dir) => dir.join("tmp"),
            None => {
                eprintln!("[THREAD] 错误: 无法获取应用缓存目录");
                return;
            }
        };
        // 确保目录存在
        if let Err(e) = fs::create_dir_all(&cache_dir) {
            eprintln!("[THREAD] 错误: 创建缓存目录失败: {}", e);
            return;
        }

        // 2. 为复制的文件生成一个唯一的新名称
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let original_filename = path_buf.file_stem().unwrap_or_default().to_string_lossy();
        let extension = path_buf.extension().unwrap_or_default().to_string_lossy();
        let new_filename = format!("external-{}-{}.{}", original_filename, timestamp, extension);
        let dest_path = cache_dir.join(new_filename);

        // 3. 将外部图片复制到缓存目录
        if let Err(e) = fs::copy(&path_buf, &dest_path) {
            eprintln!("[THREAD] 错误: 复制外部图片失败: {}", e);
            send_notification(&app_handle, "❌ 打开失败", &format!("无法复制文件: {}", e));
            return;
        }
        println!("[THREAD] 外部图片已成功复制到: {:?}", dest_path);

        // 4. 将新路径添加到历史记录中
        add_image_to_history(&app_handle.state(), dest_path.clone());

        // 5. 读取、编码并准备显示图片
        match fs::read(&dest_path) {
            Ok(bytes) => {
                let b64 = general_purpose::STANDARD.encode(&bytes);
                let dest_path_str = dest_path.to_str().unwrap_or_default().to_string();
                let payload = ImageViewerPayload {
                    image_data_url: format!("data:image/png;base64,{}", b64),
                    image_path: dest_path_str,
                };
                // 文件处理完成，现在通知主线程显示窗口
                create_and_show_image_viewer_window(&app_handle, payload);
            },
            Err(e) => {
                eprintln!("[THREAD] 错误: 无法读取复制后的图片文件: {}", e);
                send_notification(&app_handle, "❌ 打开失败", "无法读取图片文件以供预览。");
            }
        }
    });
}


/// 将图片路径添加到历史记录的辅助函数
fn add_image_to_history(state: &AppState, image_path: PathBuf) {
    // 1. 更新最后一张截图的路径
    *state.last_screenshot_path.lock().unwrap() = Some(image_path.clone());

    // 2. 将新截图添加到历史记录列表的头部
    let mut history = state.screenshot_history.lock().unwrap();
    history.insert(0, image_path);

    // 3. 限制历史记录数量（例如保留最近 20 张）
    if history.len() > 20 {
        if let Some(old_path) = history.pop() {
            // 删除最旧的文件以节省空间
            let _ = fs::remove_file(old_path);
        }
    }

    // 4. 重置查看历史的索引，确保下一次按 F3 显示最新的图片
    *state.history_index.lock().unwrap() = 0;

    println!("[COMMANDS] 图片已保存至历史记录，当前历史数: {}", history.len());
}

// 隐藏加载窗口并释放截图锁的辅助函数
fn hide_loading_and_release_lock(app: &tauri::AppHandle) {
    if let Some(loading_window) = app.get_window("loading") {
        let _ = loading_window.hide();
    }
    release_lock(app);
}

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
                        let err_msg = if e.contains("找不到翻译引擎") { "未安装翻译引擎，请在设置中下载".to_string() } else { format!("OCR成功但翻译出错: {}", e) };
                        send_notification(app, "⚠️ 翻译失败", &err_msg);
                        cache_result(app, Some(text), Some(err_msg), image_path.to_string());
                    }
                }
            }
        },
        Err(e) => {
            send_notification(app, "❌ 识别失败", &format!("{}", e));
            cache_result(app, None, None, image_path.to_string());
        }
    }
}

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
    let _ = Notification::new(&app.config().tauri.bundle.identifier).title(title).body(body).show();
}

fn perform_ocr(app: &tauri::AppHandle, image_path_str: &str, settings: &AppSettings) -> Result<String, String> {
    println!("[OCR] 开始执行 OCR 流程...");
    println!("[OCR] 待识别图片路径: {}", image_path_str);

    let ocr_exe_path = app.path_resolver().app_local_data_dir()
        .ok_or_else(|| "无法获取本地数据目录".to_string())?
        .join(OCR_DIR_NAME)
        .join(OCR_EXE_NAME);

    println!("[OCR] 预期的 OCR 执行文件路径: {:?}", ocr_exe_path);

    if !ocr_exe_path.exists() {
        let err_msg = "未找到OCR引擎，请在设置页面下载。".to_string();
        println!("[OCR] 错误: {}", err_msg);
        return Err(err_msg);
    }
    println!("[OCR] OCR 执行文件存在, 准备调用.");

    let ocr_dir = ocr_exe_path.parent().ok_or("无法获取OCR目录")?;
    println!("[OCR] OCR 工作目录: {:?}", ocr_dir);

    #[cfg(windows)] const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut command = StdCommand::new(&ocr_exe_path);
    let arg = format!("--image_path={}", image_path_str);
    command.args(&[arg.clone()]).current_dir(&ocr_dir);
    #[cfg(windows)] command.creation_flags(CREATE_NO_WINDOW);

    println!("[OCR] 准备执行命令: {:?} with arg: '{}'", ocr_exe_path, arg);

    let ocr_output = command.output().map_err(|e| {
        let err_msg = format!("执行OCR进程失败: {}", e);
        println!("[OCR] 错误: {}", err_msg);
        err_msg
    })?;

    println!("[OCR] 进程执行完毕. Status: {:?}", ocr_output.status);

    if !ocr_output.status.success() {
        let stderr = String::from_utf8_lossy(&ocr_output.stderr).into_owned();
        let err_msg = format!("OCR进程返回错误: {}", stderr);
        println!("[OCR] 错误: {}", err_msg);
        println!("[OCR] Stderr (raw bytes): {:?}", &ocr_output.stderr);
        return Err(err_msg);
    }

    let stdout = String::from_utf8_lossy(&ocr_output.stdout).into_owned();
    println!("[OCR] Stdout (decoded): '{}'", stdout);
    println!("[OCR] Stdout (raw bytes): {:?}", &ocr_output.stdout);

    let json_start = stdout.lines().find(|line| line.starts_with('{')).unwrap_or("{}");
    println!("[OCR] 提取到的 JSON 字符串: '{}'", json_start);

    let ocr_value: serde_json::Value = serde_json::from_str(json_start).map_err(|e| {
        let err_msg = format!("解析OCR结果JSON失败: {}", e);
        println!("[OCR] 错误: {}", err_msg);
        err_msg
    })?;
    println!("[OCR] 解析到的 JSON 值: {}", serde_json::to_string_pretty(&ocr_value).unwrap_or_default());

    if ocr_value["code"].as_i64().unwrap_or(0) == 100 {
        let separator = if settings.preserve_line_breaks { "\n" } else { " " };
        let text = ocr_value["data"].as_array().unwrap_or(&vec![]).iter()
            .filter_map(|item| item["text"].as_str()).collect::<Vec<_>>().join(separator);
        if text.trim().is_empty() {
            println!("[OCR] 警告: 未识别到任何文字.");
            Err("未识别到文字".to_string())
        } else {
            println!("[OCR] 识别成功, 文本内容: '{}'", text);
            Ok(text)
        }
    } else {
        let err_msg = ocr_value["data"].as_str().unwrap_or("未知OCR错误").to_string();
        println!("[OCR] 错误: OCR 引擎返回错误码: {}", err_msg);
        Err(err_msg)
    }
}

fn create_and_show_image_viewer_window(app: &tauri::AppHandle, payload: ImageViewerPayload) {
    let handle = app.clone();
    let handle_for_closure = handle.clone();
    // 确保窗口操作在主线程上执行
    handle.run_on_main_thread(move || {
        if let Some(window) = handle_for_closure.get_window("image_viewer") {
            // 如果窗口已存在，直接发送数据并显示
            let _ = window.emit("display-image", payload);
            let _ = window.show();
            let _ = window.set_focus();
        } else {
            // 如果窗口不存在，则创建它
            let builder = tauri::WindowBuilder::new(&handle_for_closure, "image_viewer", tauri::WindowUrl::App("image_viewer.html".into()))
                .title("截图预览").decorations(false).transparent(true).resizable(true).skip_taskbar(true).visible(false);
            if let Ok(window) = builder.build() {
                let window_clone = window.clone();
                // 监听 "tauri://created" 事件，确保在 webview 加载完成后再发送数据
                window.once("tauri://created", move |_| {
                    let _ = window_clone.emit("display-image", payload);
                });
            }
        }
    }).unwrap_or_else(|e| eprintln!("[UI] 无法在主线程上创建或显示预览窗口: {}", e));
}
