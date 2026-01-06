#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// --- 模块引入 ---
mod capture;
mod commands;
mod settings;
mod translator;

use tauri::{
    AppHandle, GlobalShortcutManager, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu,
    CustomMenuItem,
};
use tauri_plugin_autostart::MacosLauncher;
use settings::{AppState, AppSettings};
use base64::{Engine as _, engine::general_purpose};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

// 用于“查看截图”功能的事件载荷
#[derive(Clone, Serialize)]
struct ImageViewerPayload {
    image_data_url: String,
    image_path: String,
}

// 用于截图窗口初始化的事件载荷
#[derive(Clone, Serialize)]
struct ScreenshotPayload {
    image_data_url: String,
}


fn main() {
    let show_settings = CustomMenuItem::new("show_settings".to_string(), "显示设置");
    let quit = CustomMenuItem::new("quit".to_string(), "退出");
    let tray_menu = SystemTrayMenu::new().add_item(show_settings).add_native_item(tauri::SystemTrayMenuItem::Separator).add_item(quit);
    let system_tray = SystemTray::new().with_menu(tray_menu);

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .invoke_handler(tauri::generate_handler![
            commands::process_screenshot_area,
            settings::get_settings,
            settings::set_settings,
            settings::copy_image_to_clipboard,
            settings::save_image_to_desktop
        ])
        .system_tray(system_tray)
        .on_system_tray_event(|app, event| match event {
            SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
                "quit" => std::process::exit(0),
                "show_settings" => { if let Some(window) = app.get_window("main") { window.show().unwrap(); window.set_focus().unwrap(); } }
                _ => {}
            },
            _ => {}
        })
        .setup(|app| {
            let settings = AppSettings::load(&app.path_resolver()).unwrap_or_default();

            app.manage(AppState {
                settings: std::sync::Mutex::new(settings.clone()),
                last_screenshot_path: std::sync::Mutex::new(None),
                fullscreen_capture: std::sync::Mutex::new(None),
                is_capturing: AtomicBool::new(false),
            });

            println!("[SETUP] 应用启动，注册主截图快捷键: {}", &settings.shortcut);
            register_global_shortcut(app.handle(), &settings.shortcut)
                .unwrap_or_else(|e| eprintln!("[SETUP] 启动时注册主截图快捷键失败: {}", e));

            println!("[SETUP] 应用启动，注册查看截图快捷键: {}", &settings.view_image_shortcut);
            register_view_image_shortcut(app.handle(), &settings.view_image_shortcut)
                .unwrap_or_else(|e| eprintln!("[SETUP] 启动时注册查看截图快捷键失败: {}", e));

            let main_window = app.get_window("main").unwrap();
            main_window.show()?;
            main_window.emit("backend-ready", ()).unwrap();
            Ok(())
        })
        .build(tauri::generate_context!()).expect("运行Tauri应用时出错")
        .run(|_app_handle, event| { if let tauri::RunEvent::ExitRequested { api, .. } = event { api.prevent_exit(); } });
}


/// 主截图快捷键的注册与处理逻辑
pub fn register_global_shortcut(app_handle: AppHandle, shortcut: &str) -> Result<(), tauri::Error> {
    let mut manager = app_handle.global_shortcut_manager();

    if manager.is_registered(shortcut)? {
        manager.unregister(shortcut)?;
    }

    let shortcut_for_closure = shortcut.to_string();

    manager.register(shortcut, move || {
        let state: tauri::State<AppState> = app_handle.state();

        // --- 日志：检查截图锁 ---
        if state.is_capturing.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            println!("[SHORTCUT] 截图正在进行中，忽略本次快捷键 '{}' 操作。", shortcut_for_closure);
            return;
        }

        println!("[SHORTCUT] 全局快捷键 '{}' 被按下，已设置 is_capturing=true，开始截图流程。", shortcut_for_closure);

        let handle_for_main_thread = app_handle.clone();
        app_handle.run_on_main_thread(move || {
            let image_result = capture::capture_fullscreen();

            match image_result {
                Ok(image) => {
                    let data_url_result = capture::encode_image_to_data_url(&image);
                    if let Ok(data_url) = data_url_result {
                        let state: tauri::State<AppState> = handle_for_main_thread.state();
                        *state.fullscreen_capture.lock().unwrap() = Some(image);
                        println!("[CAPTURE] 全屏截图已成功捕获并缓存到 AppState。");

                        let payload = ScreenshotPayload { image_data_url: data_url };

                        if let Some(window) = handle_for_main_thread.get_window("screenshot") {
                            println!("[WINDOW] 'screenshot' 窗口已存在，发送 'initialize-screenshot' 事件。");
                            window.emit("initialize-screenshot", payload).unwrap();
                            window.show().unwrap();
                            window.set_focus().unwrap();
                        } else {
                            println!("[WINDOW] 'screenshot' 窗口不存在，创建新窗口。");
                            let builder = tauri::WindowBuilder::new(
                                &handle_for_main_thread,
                                "screenshot",
                                tauri::WindowUrl::App("screenshot.html".into())
                            )
                                .fullscreen(true).decorations(false).transparent(true)
                                .resizable(false).visible(false);

                            if let Ok(window) = builder.build() {
                                let window_clone = window.clone();
                                window.once("tauri://created", move |_| {
                                    println!("[WINDOW] 'screenshot' 窗口已创建，发送 'initialize-screenshot' 事件。");
                                    window_clone.emit("initialize-screenshot", payload).unwrap();
                                    window_clone.show().unwrap();
                                    window_clone.set_focus().unwrap();
                                });
                            }
                        }
                    } else {
                        eprintln!("[CAPTURE] 错误：将截图编码为 Data URL 失败。释放截图锁。");
                        let state: tauri::State<AppState> = handle_for_main_thread.state();
                        state.is_capturing.store(false, Ordering::SeqCst);
                    }
                },
                Err(e) => {
                    eprintln!("[CAPTURE] 错误：执行全屏截图失败: {}。释放截图锁。", e);
                    let state: tauri::State<AppState> = handle_for_main_thread.state();
                    state.is_capturing.store(false, Ordering::SeqCst);
                }
            }
        }).unwrap();
    }).map_err(Into::into)
}


/// “查看截图”快捷键的注册函数 (无修改)
pub fn register_view_image_shortcut(app_handle: AppHandle, shortcut: &str) -> Result<(), tauri::Error> {
    let mut manager = app_handle.global_shortcut_manager();

    if manager.is_registered(shortcut)? {
        let _ = manager.unregister(shortcut);
    }

    let shortcut_for_closure = shortcut.to_string();

    manager.register(shortcut, move || {
        println!("[SHORTCUT] 查看截图快捷键 {} 被按下", shortcut_for_closure);

        let handle_for_thread = app_handle.clone();

        std::thread::spawn(move || {
            let path_to_show: Option<PathBuf> = {
                let state: tauri::State<AppState> = handle_for_thread.state();
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

                    let handle_for_main_thread = handle_for_thread.clone();
                    handle_for_thread.run_on_main_thread(move || {
                        if let Some(window) = handle_for_main_thread.get_window("image_viewer") {
                            window.emit("display-image", payload).unwrap();
                            window.show().unwrap();
                            window.set_focus().unwrap();
                        } else {
                            let builder = tauri::WindowBuilder::new(&handle_for_main_thread, "image_viewer", tauri::WindowUrl::App("image_viewer.html".into()))
                                .title("截图预览").decorations(false).transparent(true)
                                .resizable(true).skip_taskbar(true).visible(false);

                            if let Ok(window) = builder.build() {
                                let window_for_closure = window.clone();
                                window.once("tauri://created", move |_| {
                                    window_for_closure.emit("display-image", payload).unwrap();
                                });
                            }
                        }
                    }).unwrap();
                }
            }
        });
    }).map_err(Into::into)
}