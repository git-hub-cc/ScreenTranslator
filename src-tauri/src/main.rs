// 在非调试模式下（即发布版），隐藏Windows系统下的控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// 引入我们自己定义的模块
mod commands;
mod settings;
mod translator;

// 引入所需的Tauri和其他库的模块
use tauri::{
    AppHandle, GlobalShortcutManager, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu,
    CustomMenuItem,
};
use tauri_plugin_autostart::MacosLauncher;
use settings::{AppSettings, AppState}; // 引入我们定义的状态和设置结构体

// 程序的入口函数
fn main() {
    // --- 1. 定义系统托盘菜单 ---
    let show_settings = CustomMenuItem::new("show_settings".to_string(), "显示设置");
    let quit = CustomMenuItem::new("quit".to_string(), "退出");
    let tray_menu = SystemTrayMenu::new()
        .add_item(show_settings)
        .add_native_item(tauri::SystemTrayMenuItem::Separator)
        .add_item(quit);
    let system_tray = SystemTray::new().with_menu(tray_menu);

    // --- 2. 构建并运行Tauri应用 ---
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .invoke_handler(tauri::generate_handler![
            commands::process_screenshot_area,
            settings::get_settings,
            settings::set_settings
        ])
        .system_tray(system_tray)
        .on_system_tray_event(|app, event| match event {
            SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
                "quit" => {
                    std::process::exit(0);
                }
                "show_settings" => {
                    if let Some(window) = app.get_window("main") {
                        window.show().unwrap();
                        window.set_focus().unwrap();
                    }
                }
                _ => {}
            },
            _ => {}
        })
        // 在应用启动时执行的设置钩子
        .setup(|app| {
            // 步骤 1: 立即创建并注册状态。
            let settings = AppSettings::load(&app.path_resolver()).unwrap_or_default();
            let app_state = AppState {
                settings: std::sync::Mutex::new(settings),
            };
            app.manage(app_state);

            // 步骤 2: 从已注册的状态中安全地获取初始快捷键配置。
            let state: tauri::State<AppState> = app.state();
            let shortcut = state.settings.lock().unwrap().shortcut.clone();

            // 步骤 3: 使用获取到的配置来执行其他初始化操作，比如注册快捷键。
            if let Err(e) = register_global_shortcut(app.handle(), &shortcut) {
                eprintln!("注册全局快捷键失败: {}", e);
            }

            // 步骤 4: 显示主窗口。
            if let Some(window) = app.get_window("main") {
                window.show()?;
            }

            // --- 核心修正：发送后端就绪事件 ---
            // 在所有后端初始化工作完成后，向前端发送一个信号，
            // 告诉前端可以安全地调用需要状态的指令了。
            let main_window = app.get_window("main").unwrap();
            main_window.emit("backend-ready", ()).unwrap();
            println!("后端已就绪，已发送 'backend-ready' 事件。");

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("运行Tauri应用时出错")
        .run(|_app_handle, event| match event {
            // 防止关闭最后一个窗口时程序退出
            tauri::RunEvent::ExitRequested { api, .. } => {
                api.prevent_exit();
            }
            _ => {}
        });
}

/// 注册或重新注册全局快捷键的辅助函数
pub fn register_global_shortcut(app_handle: AppHandle, shortcut: &str) -> Result<(), tauri::Error> {
    let handle = app_handle.clone();
    let mut shortcut_manager = handle.global_shortcut_manager();

    shortcut_manager.unregister_all()?;

    let shortcut_owned = shortcut.to_string();

    shortcut_manager.register(shortcut, move || {
        println!("全局快捷键 {} 被按下", shortcut_owned);
        let handle_clone = handle.clone();

        if let Some(window) = handle_clone.get_window("screenshot") {
            if let Ok(is_visible) = window.is_visible() {
                if !is_visible {
                    window.show().unwrap();
                    window.set_focus().unwrap();
                }
            }
        } else {
            tauri::WindowBuilder::new(
                &handle_clone,
                "screenshot",
                tauri::WindowUrl::App("screenshot.html".into()),
            )
                .fullscreen(true)
                .decorations(false)
                .transparent(true)
                .build()
                .unwrap();
        }
    })?;

    Ok(())
}