#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod commands;
mod settings;
mod translator;

use tauri::{
    AppHandle, GlobalShortcutManager, Manager, State,
    PhysicalSize, PhysicalPosition, Size, Position
};
use tauri_plugin_autostart::MacosLauncher;
use settings::{AppState, AppSettings};
use std::sync::atomic::{Ordering};
use base64::{Engine as _, engine::general_purpose};
use std::fs;
use std::path::PathBuf; // 引入 PathBuf 用于处理文件路径

// --- 事件 Payload 定义 ---
#[derive(Clone, serde::Serialize)]
struct ImageViewerPayload { image_data_url: String, image_path: String }
#[derive(Clone, serde::Serialize)]
struct ScreenshotPayload { image_data_url: String }
#[derive(Clone, serde::Serialize)]
struct OcrPayload { original_text: Option<String>, error_message: Option<String>, image_path: String }
#[derive(Clone, serde::Serialize)]
struct TranslationUpdatePayload { translated_text: Option<String>, error_message: Option<String> }

/// [新增] 辅助函数，用于处理命令行参数
///
/// 检查参数列表，如果发现文件路径，则调用处理函数。
///
/// # 返回
/// `bool`: 如果成功处理了一个文件路径参数，则返回 `true`，否则返回 `false`。
fn process_cli_args(app: &AppHandle, args: &[String]) -> bool {
    // 命令行参数的第一个元素通常是程序路径，我们关心的是第二个元素
    if let Some(path_str) = args.get(1) {
        let image_path = PathBuf::from(path_str);
        // 简单验证一下路径是否像一个存在的文件
        if image_path.is_file() {
            println!("[CLI] 检测到文件参数: {:?}", image_path);
            // 调用命令模块中的处理函数
            commands::handle_external_image_open(app, &image_path);
            return true; // 表示已处理
        }
    }
    false // 未处理任何文件
}

fn main() {
    tauri::Builder::default()
        // 注入全局状态
        .manage(AppState::default())

        // --- 插件初始化 ---
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))

        // --- 核心修改：扩展单实例插件逻辑 ---
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            println!("[SingleInstance] 检测到新实例启动，参数: {:?}", argv);
            // 尝试将参数作为图片路径处理
            if !process_cli_args(app, &argv) {
                // 如果没有成功处理文件（即正常启动），则唤醒主窗口
                println!("[SingleInstance] 未检测到文件参数，正在唤醒主设置窗口...");
                if let Some(window) = app.get_window("main") {
                    let _ = window.unminimize();
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        }))

        // 监听窗口事件，确保主窗口关闭时程序完全退出
        .on_window_event(|event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event.event() {
                let window = event.window();
                if window.label() == "main" {
                    println!("主窗口关闭，正在终止所有进程...");
                    window.app_handle().exit(0);
                }
            }
        })

        // 注册命令处理程序
        .invoke_handler(tauri::generate_handler![
            commands::process_screenshot_area,
            commands::process_image_from_path,
            commands::cancel_screenshot,
            settings::get_settings,
            settings::set_settings,
            settings::copy_image_to_clipboard,
            settings::save_image_to_desktop,
            commands::check_ocr_status,
            commands::download_ocr,
            commands::check_translator_status,
            commands::download_translator
        ])
        // 应用程序初始化设置
        .setup(|app| {
            // --- 核心修改：处理首次启动时的文件关联 ---
            // 检查程序启动时是否附带了命令行参数（例如，通过 "打开方式" 启动）
            let cli_args: Vec<String> = std::env::args().collect();
            if cli_args.len() > 1 {
                process_cli_args(&app.handle(), &cli_args);
            }

            let state: State<AppState> = app.state();
            let settings = AppSettings::load(&app.path_resolver()).unwrap_or_default();

            *state.settings.lock().unwrap() = settings.clone();

            register_global_shortcut(app.handle(), &settings.shortcut).unwrap_or_else(|e| eprintln!("主快捷键注册失败: {}", e));
            register_view_image_shortcut(app.handle(), &settings.view_image_shortcut).unwrap_or_else(|e| eprintln!("查看快捷键注册失败: {}", e));

            if let Some(main_window) = app.get_window("main") {
                main_window.show()?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Tauri 构建失败")
        .run(|_app_handle, _event| {});
}

/// 显示结果窗口并填充缓存的数据
pub fn show_results_window_with_cache(app: &AppHandle) {
    let state: State<AppState> = app.state();
    let cache_opt = state.last_ocr_result.lock().unwrap().clone();

    if let Some(data) = cache_opt {
        let window = if let Some(w) = app.get_window("results") {
            w
        } else {
            tauri::WindowBuilder::new(app, "results", tauri::WindowUrl::App("results.html".into()))
                .inner_size(500.0, 700.0)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .build()
                .expect("无法创建结果窗口")
        };

        window.show().unwrap();
        window.set_focus().unwrap();

        window.emit("ocr_result", OcrPayload {
            original_text: data.original_text,
            error_message: None,
            image_path: data.image_path,
        }).unwrap();

        if let Some(trans) = data.translated_text {
            window.emit("translation_update", TranslationUpdatePayload {
                translated_text: Some(trans),
                error_message: None,
            }).unwrap();
        }
    }
}

/// 注册主截图功能的全局快捷键
pub fn register_global_shortcut(app_handle: AppHandle, shortcut: &str) -> Result<(), tauri::Error> {
    let mut manager = app_handle.global_shortcut_manager();
    if manager.is_registered(shortcut)? { manager.unregister(shortcut)?; }

    let shortcut_clone = shortcut.to_string();

    manager.register(shortcut, move || {
        let state: State<AppState> = app_handle.state();
        if state.is_capturing.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            println!("[SHORTCUT] 截图正在进行中，忽略快捷键: {}", shortcut_clone);
            return;
        }
        println!("[SHORTCUT] 触发截图: {}", shortcut_clone);
        let handle = app_handle.clone();

        app_handle.run_on_main_thread(move || {
            let inner_state: State<AppState> = handle.state();
            match crate::capture::capture_fullscreen() {
                Ok(image) => {
                    let img_width = image.width();
                    let img_height = image.height();

                    *inner_state.fullscreen_capture.lock().unwrap() = Some(image.clone());
                    let data_url = crate::capture::encode_image_to_data_url(&image).unwrap();

                    if let Some(w) = handle.get_window("screenshot") {
                        w.set_size(Size::Physical(PhysicalSize { width: img_width, height: img_height })).unwrap();
                        w.set_position(Position::Physical(PhysicalPosition { x: 0, y: 0 })).unwrap();
                        w.emit("initialize-screenshot", ScreenshotPayload{image_data_url: data_url}).unwrap();
                        w.show().unwrap();
                        w.set_focus().unwrap();
                    } else {
                        let w = tauri::WindowBuilder::new(&handle, "screenshot", tauri::WindowUrl::App("screenshot.html".into()))
                            .title("").decorations(false).transparent(true).visible(false).skip_taskbar(true)
                            .always_on_top(true).resizable(false).build().unwrap();
                        w.set_size(Size::Physical(PhysicalSize { width: img_width, height: img_height })).unwrap();
                        w.set_position(Position::Physical(PhysicalPosition { x: 0, y: 0 })).unwrap();
                        w.emit("initialize-screenshot", ScreenshotPayload{image_data_url: data_url}).unwrap();
                        w.show().unwrap();
                        w.set_focus().unwrap();
                    }
                },
                Err(e) => {
                    eprintln!("全屏截图失败: {}", e);
                    inner_state.is_capturing.store(false, Ordering::SeqCst);
                }
            }
        }).unwrap();
    }).map_err(Into::into)
}

/// 注册查看上一次截图/结果的全局快捷键
pub fn register_view_image_shortcut(app_handle: AppHandle, shortcut: &str) -> Result<(), tauri::Error> {
    let mut manager = app_handle.global_shortcut_manager();
    if manager.is_registered(shortcut)? { let _ = manager.unregister(shortcut); }

    manager.register(shortcut, move || {
        let handle_for_thread = app_handle.clone();
        std::thread::spawn(move || {
            let state: State<AppState> = handle_for_thread.state();
            let path_to_show = {
                let history = state.screenshot_history.lock().unwrap();
                let mut index = state.history_index.lock().unwrap();
                if history.is_empty() {
                    println!("[VIEWER] 历史记录为空，无法查看。");
                    None
                } else {
                    if *index >= history.len() { *index = 0; }
                    let path = history[*index].clone();
                    println!("[VIEWER] 正在查看历史记录 [{}/{}]: {:?}", *index + 1, history.len(), path);
                    *index = (*index + 1) % history.len();
                    Some(path)
                }
            };

            if let Some(path) = path_to_show {
                if let Ok(bytes) = fs::read(&path) {
                    let b64 = general_purpose::STANDARD.encode(&bytes);
                    let payload = ImageViewerPayload {
                        image_data_url: format!("data:image/png;base64,{}", b64),
                        image_path: path.to_str().unwrap().to_string(),
                    };

                    let handle_main = handle_for_thread.clone();
                    handle_for_thread.run_on_main_thread(move || {
                        if let Some(window) = handle_main.get_window("image_viewer") {
                            window.emit("display-image", payload).unwrap();
                            window.show().unwrap(); window.set_focus().unwrap();
                        } else {
                            let _ = tauri::WindowBuilder::new(&handle_main, "image_viewer", tauri::WindowUrl::App("image_viewer.html".into()))
                                .title("截图预览").decorations(false).transparent(true).resizable(true).skip_taskbar(true).visible(false)
                                .build().unwrap().emit("display-image", payload);
                        }
                    }).unwrap();
                } else {
                    eprintln!("[VIEWER] 错误：无法读取历史图片文件 {:?}", path);
                }
            }
        });
    }).map_err(Into::into)
}