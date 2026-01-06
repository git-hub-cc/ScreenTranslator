#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod commands;
mod settings;
mod translator;

use tauri::{
    AppHandle, GlobalShortcutManager, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu,
    CustomMenuItem, State,
};
use tauri_plugin_autostart::MacosLauncher;
use settings::{AppState, AppSettings};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use base64::{Engine as _, engine::general_purpose};
use std::fs;

// --- 事件 Payload 定义 ---
#[derive(Clone, serde::Serialize)]
struct ImageViewerPayload { image_data_url: String, image_path: String }
#[derive(Clone, serde::Serialize)]
struct ScreenshotPayload { image_data_url: String }
#[derive(Clone, serde::Serialize)]
struct OcrPayload { original_text: Option<String>, error_message: Option<String>, image_path: String }
#[derive(Clone, serde::Serialize)]
struct TranslationUpdatePayload { translated_text: Option<String>, error_message: Option<String> }

fn main() {
    let show_settings = CustomMenuItem::new("show_settings".to_string(), "设置");
    let quit = CustomMenuItem::new("quit".to_string(), "退出");
    let tray_menu = SystemTrayMenu::new()
        .add_item(show_settings)
        .add_native_item(tauri::SystemTrayMenuItem::Separator)
        .add_item(quit);
    let system_tray = SystemTray::new().with_menu(tray_menu);

    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .invoke_handler(tauri::generate_handler![
            // 核心功能
            commands::process_screenshot_area,
            commands::process_image_from_path,
            // 设置
            settings::get_settings,
            settings::set_settings,
            // 辅助功能
            settings::copy_image_to_clipboard,
            settings::save_image_to_desktop,
            // 引擎管理
            commands::check_ocr_status,       // 新增
            commands::download_ocr,          // 新增
            commands::check_translator_status,
            commands::download_translator
        ])
        .system_tray(system_tray)
        .on_system_tray_event(|app, event| match event {
            SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
                "quit" => std::process::exit(0),
                "show_settings" => { if let Some(w) = app.get_window("main") { w.show().unwrap(); w.set_focus().unwrap(); } }
                _ => {}
            },
            SystemTrayEvent::DoubleClick { .. } => {
                if let Some(w) = app.get_window("main") { w.show().unwrap(); w.set_focus().unwrap(); }
            }
            _ => {}
        })
        .setup(|app| {
            let state: State<AppState> = app.state();
            let settings = AppSettings::load(&app.path_resolver()).unwrap_or_default();

            *state.settings.lock().unwrap() = settings.clone();

            register_global_shortcut(app.handle(), &settings.shortcut).unwrap_or_else(|e| eprintln!("主快捷键失败: {}", e));
            register_view_image_shortcut(app.handle(), &settings.view_image_shortcut).unwrap_or_else(|e| eprintln!("查看快捷键失败: {}", e));

            if let Some(main_window) = app.get_window("main") {
                main_window.show()?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Tauri 构建失败")
        .run(|_app_handle, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}

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
                    *inner_state.fullscreen_capture.lock().unwrap() = Some(image.clone());
                    let data_url = crate::capture::encode_image_to_data_url(&image).unwrap();

                    if let Some(w) = handle.get_window("screenshot") {
                        w.emit("initialize-screenshot", ScreenshotPayload{image_data_url: data_url}).unwrap();
                        w.show().unwrap();
                        w.set_focus().unwrap();
                    } else {
                        let _ = tauri::WindowBuilder::new(&handle, "screenshot", tauri::WindowUrl::App("screenshot.html".into()))
                            .fullscreen(true).decorations(false).transparent(true).visible(false).skip_taskbar(true)
                            .build().unwrap().emit("initialize-screenshot", ScreenshotPayload{image_data_url: data_url});
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

pub fn register_view_image_shortcut(app_handle: AppHandle, shortcut: &str) -> Result<(), tauri::Error> {
    let mut manager = app_handle.global_shortcut_manager();
    if manager.is_registered(shortcut)? { let _ = manager.unregister(shortcut); }
    manager.register(shortcut, move || {
        let handle_for_thread = app_handle.clone();
        std::thread::spawn(move || {
            let path_to_show: Option<std::path::PathBuf> = {
                let state: State<AppState> = handle_for_thread.state();
                let lock = state.last_screenshot_path.lock().unwrap();
                lock.clone()
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
                }
            }
        });
    }).map_err(Into::into)
}