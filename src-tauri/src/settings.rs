use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, GlobalShortcutManager, PathResolver, State};
use tauri_plugin_autostart::ManagerExt;
use arboard::ImageData;
use image::io::Reader as ImageReader;
use tauri::api::path as tauri_path;
// 引入两个快捷键注册函数，一个用于主功能，一个用于查看图片
use crate::{register_global_shortcut, register_view_image_shortcut};

// 应用的共享状态，包含设置和上一次截图的路径
pub struct AppState {
    pub settings: Mutex<AppSettings>,
    pub last_screenshot_path: Mutex<Option<PathBuf>>,
}

// 应用设置结构体，所有可配置项都在这里定义
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppSettings {
    // 主截图功能的快捷键
    pub shortcut: String,
    // 查看上次截图功能的快捷键
    pub view_image_shortcut: String,
    // 目标翻译语言
    pub target_lang: String,
    // 是否开机自启动
    pub autostart: bool,
    // 是否开启OCR文字识别
    pub enable_ocr: bool,
    // 是否开启翻译
    pub enable_translation: bool,
    // --- 新增字段 ---
    // 是否在OCR结果中保留原文的换行符
    pub preserve_line_breaks: bool,
}

// 为 AppSettings 提供默认值
impl Default for AppSettings {
    fn default() -> Self {
        Self {
            shortcut: "F1".to_string(),
            view_image_shortcut: "F3".to_string(),
            target_lang: "zh".to_string(),
            autostart: false,
            enable_ocr: false,
            enable_translation: false,
            // --- 新增字段的默认值 ---
            // 默认为 false，即不保留换行，将多行文本合并为空格分隔的一行
            preserve_line_breaks: false,
        }
    }
}

// AppSettings 的实现块，包含加载和保存设置的方法
impl AppSettings {
    // 获取配置文件的标准路径
    fn get_config_path(path_resolver: &PathResolver) -> PathBuf {
        path_resolver.app_config_dir().expect("无法获取应用配置目录").join("settings.json")
    }

    // 从文件加载设置，如果文件不存在则返回默认设置
    pub fn load(path_resolver: &PathResolver) -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Self::get_config_path(path_resolver);
        if config_path.exists() {
            let content = fs::read_to_string(config_path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    // 将当前设置保存到文件
    pub fn save(&self, path_resolver: &PathResolver) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = Self::get_config_path(path_resolver);
        if let Some(parent) = config_path.parent() { fs::create_dir_all(parent)?; }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(config_path, content)?;
        Ok(())
    }
}

// Tauri 命令：获取当前设置
#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<AppSettings, String> {
    Ok(state.settings.lock().unwrap().clone())
}

// Tauri 命令：更新设置
#[tauri::command]
pub async fn set_settings(app: AppHandle, state: State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    println!("接收到新设置: {:?}", settings);

    // 1. 保存设置到文件
    settings.save(&app.path_resolver()).map_err(|e| format!("保存设置文件失败: {}", e))?;

    // 2. 更新内存中的应用状态，并记录旧的快捷键
    let old_shortcut;
    let old_view_shortcut;
    {
        let mut app_settings = state.settings.lock().unwrap();
        old_shortcut = app_settings.shortcut.clone();
        old_view_shortcut = app_settings.view_image_shortcut.clone();
        // 新的 settings 对象（包含 preserve_line_breaks 字段）被完整地赋值给应用状态
        *app_settings = settings.clone();
    }

    let mut shortcut_manager = app.global_shortcut_manager();

    // 3. 更新主截图快捷键
    if old_shortcut != settings.shortcut {
        let _ = shortcut_manager.unregister(&old_shortcut);
        println!("主快捷键已更改，注销旧快捷键: {}", old_shortcut);
    }
    if let Err(e) = register_global_shortcut(app.clone(), &settings.shortcut) {
        eprintln!("动态更新主快捷键 {} 失败: {}", &settings.shortcut, e);
        return Err(format!("注册主快捷键失败: {}", e));
    }

    // 4. 更新“查看截图”快捷键
    if old_view_shortcut != settings.view_image_shortcut {
        let _ = shortcut_manager.unregister(&old_view_shortcut);
        println!("查看截图快捷键已更改，注销旧快捷键: {}", old_view_shortcut);
    }
    if let Err(e) = register_view_image_shortcut(app.clone(), &settings.view_image_shortcut) {
        eprintln!("动态更新查看截图快捷键 {} 失败: {}", &settings.view_image_shortcut, e);
        return Err(format!("注册查看截图快捷键失败: {}", e));
    }

    // 5. 处理开机自启动
    let autostart_manager = app.autolaunch();
    let is_enabled = autostart_manager.is_enabled().unwrap_or(false);
    if settings.autostart && !is_enabled {
        autostart_manager.enable().map_err(|e| e.to_string())?;
    } else if !settings.autostart && is_enabled {
        autostart_manager.disable().map_err(|e| e.to_string())?;
    }

    Ok(())
}


// Tauri 命令：复制图片到剪贴板
#[tauri::command]
pub async fn copy_image_to_clipboard(path: String) -> Result<(), String> {
    let img = ImageReader::open(path).map_err(|e| e.to_string())?.decode().map_err(|e| e.to_string())?.to_rgba8();
    let image_data = ImageData { width: img.width() as usize, height: img.height() as usize, bytes: img.into_raw().into() };
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_image(image_data).map_err(|e| e.to_string())?;
    Ok(())
}

// Tauri 命令：保存图片到桌面
#[tauri::command]
pub async fn save_image_to_desktop(path: String) -> Result<(), String> {
    let desktop_dir = tauri_path::desktop_dir().ok_or("无法获取桌面路径".to_string())?;
    let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let new_filename = format!("screenshot-{}.png", timestamp);
    let dest_path = desktop_dir.join(new_filename);
    fs::copy(&path, &dest_path).map_err(|e| format!("保存文件到桌面失败: {}", e))?;
    println!("图片已保存至: {:?}", dest_path);
    Ok(())
}