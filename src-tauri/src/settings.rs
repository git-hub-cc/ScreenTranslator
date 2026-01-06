use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, GlobalShortcutManager, PathResolver, State};
// 移除不再需要的 autostart 引用
// use tauri_plugin_autostart::ManagerExt;
use arboard::ImageData;
use image::ImageReader;
use image::RgbaImage;
use tauri::api::path as tauri_path;
use std::sync::atomic::AtomicBool;

use crate::{register_global_shortcut, register_view_image_shortcut};

//
// 应用的全局共享状态
//
pub struct AppState {
    pub settings: Mutex<AppSettings>,
    pub last_screenshot_path: Mutex<Option<PathBuf>>,
    pub fullscreen_capture: Mutex<Option<RgbaImage>>,
    pub is_capturing: AtomicBool,

    // 缓存最后一次处理结果
    // 用于在用户点击通知的"查看详情"时，能够向结果窗口填充数据
    pub last_ocr_result: Mutex<Option<LastOcrResult>>,
}

// 缓存的结果结构
#[derive(Clone, Debug, Serialize)]
pub struct LastOcrResult {
    pub original_text: Option<String>,
    pub translated_text: Option<String>,
    pub image_path: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppSettings {
    pub shortcut: String,
    pub view_image_shortcut: String,
    pub target_lang: String,
    // pub autostart: bool, // 已移除：不再管理开机自启动
    pub preserve_line_breaks: bool,

    // 核心操作模式
    // 取值: "ocr", "ocr_translate", "preview", "copy", "save"
    // 默认为 "ocr"
    pub primary_action: String,

    // 兼容性保留字段 (可标记为废弃)
    #[serde(default)]
    pub enable_ocr: bool,
    #[serde(default)]
    pub enable_translation: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            shortcut: "F1".to_string(),
            view_image_shortcut: "F3".to_string(),
            target_lang: "zh".to_string(),
            // autostart: false, // 已移除
            preserve_line_breaks: false,
            // 默认模式：OCR (静默复制)
            primary_action: "ocr".to_string(),
            enable_ocr: false,
            enable_translation: false,
        }
    }
}

impl AppSettings {
    fn get_config_path(path_resolver: &PathResolver) -> PathBuf {
        path_resolver.app_config_dir().expect("致命错误：无法获取应用配置目录").join("settings.json")
    }

    pub fn load(path_resolver: &PathResolver) -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Self::get_config_path(path_resolver);
        if config_path.exists() {
            let content = fs::read_to_string(config_path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, path_resolver: &PathResolver) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = Self::get_config_path(path_resolver);
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(config_path, content)?;
        Ok(())
    }
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<AppSettings, String> {
    Ok(state.settings.lock().unwrap().clone())
}

#[tauri::command]
pub async fn set_settings(app: AppHandle, state: State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    println!("接收到新设置: {:?}", settings);

    // 1. 保存设置到文件
    settings.save(&app.path_resolver()).map_err(|e| format!("保存设置文件失败: {}", e))?;

    let old_shortcut;
    let old_view_shortcut;
    {
        let mut app_settings = state.settings.lock().unwrap();
        old_shortcut = app_settings.shortcut.clone();
        old_view_shortcut = app_settings.view_image_shortcut.clone();
        *app_settings = settings.clone();
    }

    let mut shortcut_manager = app.global_shortcut_manager();

    // 2. 更新主截图快捷键
    if old_shortcut != settings.shortcut {
        let _ = shortcut_manager.unregister(&old_shortcut);
    }
    if let Err(e) = register_global_shortcut(app.clone(), &settings.shortcut) {
        return Err(format!("注册主快捷键失败: {}", e));
    }

    // 3. 更新查看图片快捷键
    if old_view_shortcut != settings.view_image_shortcut {
        let _ = shortcut_manager.unregister(&old_view_shortcut);
    }
    if let Err(e) = register_view_image_shortcut(app.clone(), &settings.view_image_shortcut) {
        return Err(format!("注册查看快捷键失败: {}", e));
    }

    // 已移除：开机自启动逻辑处理块

    Ok(())
}

#[tauri::command]
pub async fn copy_image_to_clipboard(path: String) -> Result<(), String> {
    let img = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?
        .to_rgba8();

    let image_data = ImageData {
        width: img.width() as usize,
        height: img.height() as usize,
        bytes: img.into_raw().into(),
    };

    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_image(image_data).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn save_image_to_desktop(path: String) -> Result<(), String> {
    let desktop_dir = tauri_path::desktop_dir().ok_or("无法获取桌面路径".to_string())?;
    let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let new_filename = format!("screenshot-{}.png", timestamp);
    let dest_path = desktop_dir.join(new_filename);
    fs::copy(&path, &dest_path).map_err(|e| format!("保存文件失败: {}", e))?;
    Ok(())
}