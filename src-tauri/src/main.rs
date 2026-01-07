#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod commands;
mod settings;
mod translator;

use tauri::{
    AppHandle, GlobalShortcutManager, Manager, State,
    // 新增引用，用于精确控制窗口尺寸
    PhysicalSize, PhysicalPosition, Size, Position
};
use tauri_plugin_autostart::MacosLauncher;
use settings::{AppState, AppSettings};
use std::sync::atomic::{Ordering};
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
    tauri::Builder::default()
        // 注入全局状态，使用 Default 避免竞态条件
        .manage(AppState::default())

        // --- 插件初始化 ---
        // 1. 自动启动插件
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))

        // 2. 单实例插件初始化 (防止重复启动并唤醒窗口)
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            println!("检测到第二个实例启动，正在唤醒主窗口...");
            // 获取名为 "main" 的主设置窗口
            if let Some(window) = app.get_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))

        // --- 核心修复：窗口事件监听 ---
        // 监听所有窗口事件，专门处理主窗口的关闭逻辑
        .on_window_event(|event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event.event() {
                let window = event.window();
                // 只有当被关闭的窗口是 "main" (设置页面) 时，才执行完全退出
                if window.label() == "main" {
                    println!("主窗口关闭，正在终止所有进程...");
                    // 强制退出应用程序，清理所有隐藏窗口和后台线程
                    window.app_handle().exit(0);
                }
            }
        })

        // 注册命令处理程序
        .invoke_handler(tauri::generate_handler![
            // 核心功能
            commands::process_screenshot_area,
            commands::process_image_from_path,
            commands::cancel_screenshot,
            // 设置管理
            settings::get_settings,
            settings::set_settings,
            // 辅助功能
            settings::copy_image_to_clipboard,
            settings::save_image_to_desktop,
            // 引擎管理
            commands::check_ocr_status,
            commands::download_ocr,
            commands::check_translator_status,
            commands::download_translator
        ])
        // 应用程序初始化设置
        .setup(|app| {
            let state: State<AppState> = app.state();
            // 加载设置
            let settings = AppSettings::load(&app.path_resolver()).unwrap_or_default();

            // 更新内存中的状态
            *state.settings.lock().unwrap() = settings.clone();

            // 注册全局快捷键
            register_global_shortcut(app.handle(), &settings.shortcut).unwrap_or_else(|e| eprintln!("主快捷键注册失败: {}", e));
            register_view_image_shortcut(app.handle(), &settings.view_image_shortcut).unwrap_or_else(|e| eprintln!("查看快捷键注册失败: {}", e));

            // 显式显示主窗口
            if let Some(main_window) = app.get_window("main") {
                main_window.show()?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Tauri 构建失败")
        // 运行应用程序
        .run(|_app_handle, _event| {
            // run 闭包中的代码会在 application 退出前执行，但由于我们在 on_window_event 中调用了 exit(0)，
            // 这里通常不需要额外的逻辑。
        });
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

        // 发送 OCR 结果
        window.emit("ocr_result", OcrPayload {
            original_text: data.original_text,
            error_message: None,
            image_path: data.image_path,
        }).unwrap();

        // 如果缓存中有翻译结果，也发送
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
    // 如果已存在，先注销
    if manager.is_registered(shortcut)? { manager.unregister(shortcut)?; }

    let shortcut_clone = shortcut.to_string();

    manager.register(shortcut, move || {
        let state: State<AppState> = app_handle.state();
        // 使用原子操作检查并设置截图状态锁，防止重复触发
        if state.is_capturing.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            println!("[SHORTCUT] 截图正在进行中，忽略快捷键: {}", shortcut_clone);
            return;
        }
        println!("[SHORTCUT] 触发截图: {}", shortcut_clone);
        let handle = app_handle.clone();

        // 在主线程中执行 UI 相关操作
        app_handle.run_on_main_thread(move || {
            let inner_state: State<AppState> = handle.state();
            match crate::capture::capture_fullscreen() {
                Ok(image) => {
                    // 获取截图的物理尺寸
                    let img_width = image.width();
                    let img_height = image.height();

                    // 缓存全屏截图
                    *inner_state.fullscreen_capture.lock().unwrap() = Some(image.clone());
                    // 编码图像用于前端展示
                    let data_url = crate::capture::encode_image_to_data_url(&image).unwrap();

                    // 获取或创建截图窗口
                    // --- 核心修复：使用物理尺寸精确设置窗口大小，解决白边和标题栏问题 ---
                    if let Some(w) = handle.get_window("screenshot") {
                        // 确保现有窗口尺寸和位置覆盖全屏 (Physical 确保忽略 DPI 缩放带来的误差)
                        w.set_size(Size::Physical(PhysicalSize { width: img_width, height: img_height })).unwrap();
                        w.set_position(Position::Physical(PhysicalPosition { x: 0, y: 0 })).unwrap();

                        w.emit("initialize-screenshot", ScreenshotPayload{image_data_url: data_url}).unwrap();
                        w.show().unwrap();
                        w.set_focus().unwrap();
                    } else {
                        // 创建新窗口
                        let w = tauri::WindowBuilder::new(&handle, "screenshot", tauri::WindowUrl::App("screenshot.html".into()))
                            .title("") // 确保无标题
                            .decorations(false) // 无边框
                            .transparent(true) // 透明
                            .visible(false) // 先隐藏，设置好尺寸再显示
                            .skip_taskbar(true)
                            .always_on_top(true)
                            .resizable(false)
                            .build()
                            .unwrap();

                        // 强制设置为截图的物理尺寸，无视任务栏区域
                        w.set_size(Size::Physical(PhysicalSize { width: img_width, height: img_height })).unwrap();
                        w.set_position(Position::Physical(PhysicalPosition { x: 0, y: 0 })).unwrap();

                        w.emit("initialize-screenshot", ScreenshotPayload{image_data_url: data_url}).unwrap();
                        w.show().unwrap();
                        w.set_focus().unwrap();
                    }
                },
                Err(e) => {
                    eprintln!("全屏截图失败: {}", e);
                    // 发生错误时释放锁
                    inner_state.is_capturing.store(false, Ordering::SeqCst);
                }
            }
        }).unwrap();
    }).map_err(Into::into)
}

/// 注册查看上一次截图/结果的全局快捷键
/// --- 核心修改：支持循环查看历史截图 ---
pub fn register_view_image_shortcut(app_handle: AppHandle, shortcut: &str) -> Result<(), tauri::Error> {
    let mut manager = app_handle.global_shortcut_manager();
    if manager.is_registered(shortcut)? { let _ = manager.unregister(shortcut); }

    manager.register(shortcut, move || {
        let handle_for_thread = app_handle.clone();
        // 在新线程中处理文件读取，避免阻塞
        std::thread::spawn(move || {
            let state: State<AppState> = handle_for_thread.state();

            // --- 修改开始：历史记录循环逻辑 ---
            let path_to_show = {
                let history = state.screenshot_history.lock().unwrap();
                let mut index = state.history_index.lock().unwrap();

                if history.is_empty() {
                    println!("[VIEWER] 历史记录为空，无法查看。");
                    None
                } else {
                    // 确保索引在安全范围内 (防止列表被清空后索引越界)
                    if *index >= history.len() {
                        *index = 0;
                    }

                    let path = history[*index].clone();
                    println!("[VIEWER] 正在查看历史记录 [{}/{}]: {:?}", *index + 1, history.len(), path);

                    // 更新索引以指向下一张图片 (循环)
                    *index = (*index + 1) % history.len();

                    Some(path)
                }
            };
            // --- 修改结束 ---

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