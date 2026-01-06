use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, GlobalShortcutManager, PathResolver, State};
use tauri_plugin_autostart::ManagerExt;
use arboard::ImageData;
use image::ImageReader;
use image::RgbaImage;
use tauri::api::path as tauri_path;
// --- 新增：引入 AtomicBool 用于线程安全的状态标志 ---
use std::sync::atomic::AtomicBool;

// 引入在 main.rs 中定义的两个快捷键注册函数
use crate::{register_global_shortcut, register_view_image_shortcut};

//
// 应用的全局共享状态
// AppState 用于在 Tauri 的不同命令和事件处理器之间安全地共享数据。
//
pub struct AppState {
    /// 当前的应用配置，使用 Mutex 以支持线程安全的读写。
    pub settings: Mutex<AppSettings>,
    /// 最后一次截图的临时文件路径，用于“查看截图”功能。
    pub last_screenshot_path: Mutex<Option<PathBuf>>,
    /// 在内存中缓存的全屏截图。
    /// Option<RgbaImage> 表示可能还没有截图（初始状态为 None）。
    /// 这是实现高性能、无延迟截图体验的关键。
    pub fullscreen_capture: Mutex<Option<RgbaImage>>,
    // --- 新增：截图状态标志 ---
    // 这个原子标志用于防止用户在一次截图未完成时发起新的截图请求，解决并发问题。
    pub is_capturing: AtomicBool,
}

//
// 应用设置结构体
// 定义了所有用户可以配置的选项，并能被序列化为 JSON 文件。
//
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppSettings {
    /// 主截图功能的快捷键 (例如 "F1", "Ctrl+Alt+A")。
    pub shortcut: String,
    /// 查看上次截图功能的快捷键。
    pub view_image_shortcut: String,
    /// 目标翻译语言 (例如 "zh", "en")。
    pub target_lang: String,
    /// 是否开机自启动。
    pub autostart: bool,
    /// 是否开启 OCR 文字识别。
    pub enable_ocr: bool,
    /// 是否开启翻译功能。
    pub enable_translation: bool,
    /// 是否在 OCR 结果中保留原文的换行符。
    pub preserve_line_breaks: bool,
}

/// 为 AppSettings 实现 Default trait，提供一套安全的默认配置。
/// 当配置文件不存在或解析失败时，程序会使用这些默认值。
impl Default for AppSettings {
    fn default() -> Self {
        Self {
            shortcut: "F1".to_string(),
            view_image_shortcut: "F3".to_string(),
            target_lang: "zh".to_string(),
            autostart: false,
            enable_ocr: false,
            enable_translation: false,
            preserve_line_breaks: false,
        }
    }
}

/// AppSettings 的实现块，包含加载和保存设置的核心逻辑。
impl AppSettings {
    /// 获取配置文件的标准路径 (例如 C:\Users\YourUser\AppData\Roaming\com.tauri.screentranslator\settings.json)。
    fn get_config_path(path_resolver: &PathResolver) -> PathBuf {
        path_resolver.app_config_dir().expect("致命错误：无法获取应用配置目录").join("settings.json")
    }

    /// 从文件加载设置。
    /// 如果配置文件存在，则读取并解析；如果不存在，则返回默认设置。
    pub fn load(path_resolver: &PathResolver) -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Self::get_config_path(path_resolver);
        if config_path.exists() {
            let content = fs::read_to_string(config_path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    /// 将当前设置保存到文件。
    /// 如果目录不存在，会先尝试创建。
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

/// Tauri 命令：获取当前的应用设置。
/// 这个命令被前端调用以初始化设置界面的显示。
#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<AppSettings, String> {
    // 从共享状态中克隆一份设置并返回
    Ok(state.settings.lock().unwrap().clone())
}

/// Tauri 命令：接收前端传来的新设置并应用。
#[tauri::command]
pub async fn set_settings(app: AppHandle, state: State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    println!("接收到新设置: {:?}", settings);

    // 1. 将新设置持久化保存到 settings.json 文件
    settings.save(&app.path_resolver()).map_err(|e| format!("保存设置文件失败: {}", e))?;

    // 2. 更新内存中的应用状态，并记录下旧的快捷键以便后续注销
    let old_shortcut;
    let old_view_shortcut;
    {
        let mut app_settings = state.settings.lock().unwrap();
        old_shortcut = app_settings.shortcut.clone();
        old_view_shortcut = app_settings.view_image_shortcut.clone();
        *app_settings = settings.clone();
    }

    let mut shortcut_manager = app.global_shortcut_manager();

    // 3. 更新主截图快捷键
    // 如果快捷键发生了变化，先注销旧的，再注册新的
    if old_shortcut != settings.shortcut {
        let _ = shortcut_manager.unregister(&old_shortcut);
        println!("主快捷键已更改，注销旧快捷键: {}", old_shortcut);
    }
    if let Err(e) = register_global_shortcut(app.clone(), &settings.shortcut) {
        let error_msg = format!("注册主快捷键 {} 失败: {}", &settings.shortcut, e);
        eprintln!("{}", error_msg);
        return Err(error_msg);
    }

    // 4. 更新“查看截图”快捷键
    if old_view_shortcut != settings.view_image_shortcut {
        let _ = shortcut_manager.unregister(&old_view_shortcut);
        println!("查看截图快捷键已更改，注销旧快捷键: {}", old_view_shortcut);
    }
    if let Err(e) = register_view_image_shortcut(app.clone(), &settings.view_image_shortcut) {
        let error_msg = format!("注册查看截图快捷键 {} 失败: {}", &settings.view_image_shortcut, e);
        eprintln!("{}", error_msg);
        return Err(error_msg);
    }

    // 5. 根据设置同步开机自启动状态
    let autostart_manager = app.autolaunch();
    let is_enabled = autostart_manager.is_enabled().unwrap_or(false);

    if settings.autostart && !is_enabled {
        autostart_manager.enable().map_err(|e| e.to_string())?;
    } else if !settings.autostart && is_enabled {
        autostart_manager.disable().map_err(|e| e.to_string())?;
    }

    Ok(())
}


/// Tauri 命令：将指定路径的图片复制到系统剪贴板。
#[tauri::command]
pub async fn copy_image_to_clipboard(path: String) -> Result<(), String> {
    // 使用 image 库打开并解码图片文件
    let img = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?
        .to_rgba8(); // 转换为剪贴板库所需的 RGBA8 格式

    // 构造 arboard 库所需的 ImageData 结构
    let image_data = ImageData {
        width: img.width() as usize,
        height: img.height() as usize,
        bytes: img.into_raw().into(),
    };

    // 获取剪贴板实例并设置图片
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_image(image_data).map_err(|e| e.to_string())?;

    Ok(())
}

/// Tauri 命令：将指定路径的图片另存到用户桌面。
#[tauri::command]
pub async fn save_image_to_desktop(path: String) -> Result<(), String> {
    // 1. 获取用户桌面目录的路径
    let desktop_dir = tauri_path::desktop_dir().ok_or("无法获取桌面路径".to_string())?;

    // 2. 使用当前时间戳生成一个独一无二的文件名，防止覆盖
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let new_filename = format!("screenshot-{}.png", timestamp);
    let dest_path = desktop_dir.join(new_filename);

    // 3. 执行文件复制操作
    fs::copy(&path, &dest_path).map_err(|e| format!("保存文件到桌面失败: {}", e))?;

    println!("图片已保存至: {:?}", dest_path);
    Ok(())
}