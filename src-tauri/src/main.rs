#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod settings;
mod translator;

use tauri::{
    AppHandle, GlobalShortcutManager, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu,
    CustomMenuItem,
};
use tauri_plugin_autostart::MacosLauncher;
use settings::{AppSettings, AppState};

use std::fs;
use base64::{Engine as _, engine::general_purpose};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Clone, Serialize)]
struct ImageViewerPayload {
    image_data_url: String,
    image_path: String,
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
                settings: std::sync::Mutex::new(settings),
                last_screenshot_path: std::sync::Mutex::new(None),
            });
            let state: tauri::State<AppState> = app.state();
            let shortcut = state.settings.lock().unwrap().shortcut.clone();
            register_global_shortcut(app.handle(), &shortcut).unwrap_or_else(|e| eprintln!("注册截图快捷键失败: {}", e));
            register_f3_shortcut(app.handle()).unwrap_or_else(|e| eprintln!("注册F3快捷键失败: {}", e));
            let main_window = app.get_window("main").unwrap();
            main_window.show()?;
            main_window.emit("backend-ready", ()).unwrap();
            Ok(())
        })
        .build(tauri::generate_context!()).expect("运行Tauri应用时出错")
        .run(|_app_handle, event| { if let tauri::RunEvent::ExitRequested { api, .. } = event { api.prevent_exit(); } });
}

pub fn register_global_shortcut(app_handle: AppHandle, shortcut: &str) -> Result<(), tauri::Error> {
    let mut manager = app_handle.global_shortcut_manager();
    let _ = manager.unregister(shortcut);

    let shortcut_for_closure = shortcut.to_string();

    manager.register(shortcut, move || {
        println!("全局快捷键 {} 被按下", shortcut_for_closure);

        let handle_for_closure = app_handle.clone();
        app_handle.run_on_main_thread(move || {
            if let Some(window) = handle_for_closure.get_window("screenshot") {
                window.show().unwrap();
                window.set_focus().unwrap();
            } else {
                tauri::WindowBuilder::new(&handle_for_closure, "screenshot", tauri::WindowUrl::App("screenshot.html".into()))
                    .fullscreen(true)
                    .decorations(false)
                    .transparent(true)
                    // --- BUG修复：明确禁止调整窗口大小，防止操作系统窗口贴靠功能干扰 ---
                    .resizable(false)
                    .build()
                    .unwrap();
            }
        }).unwrap();
    }).map_err(Into::into)
}

fn register_f3_shortcut(app_handle: AppHandle) -> Result<(), tauri::Error> {
    let mut manager = app_handle.global_shortcut_manager();
    let _ = manager.unregister("F3");

    manager.register("F3", move || {
        println!("F3 快捷键被按下");

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