use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, GlobalShortcutManager, Manager, PathResolver, State};
use tauri_plugin_autostart::ManagerExt; // 引入插件的管理扩展

// --- 1. 应用状态结构体 ---
// 这个结构体用于在Tauri的全局状态中管理我们应用的配置。
// Mutex用于保证在多线程环境下对配置的访问是安全的。
pub struct AppState {
    pub settings: Mutex<AppSettings>,
}

// --- 2. 应用设置结构体 ---
// 这个结构体定义了所有可配置的选项。
// `serde`宏会自动为我们实现序列化和反序列化功能。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppSettings {
    pub shortcut: String,
    pub api_key: String,
    pub target_lang: String,
    pub autostart: bool,
}

// --- 3. 为AppSettings实现默认值 ---
// 当配置文件不存在或加载失败时，程序会使用这些默认值。
impl Default for AppSettings {
    fn default() -> Self {
        Self {
            shortcut: "Alt+Q".to_string(),
            api_key: "".to_string(),
            target_lang: "ZH".to_string(),
            autostart: false,
        }
    }
}

// --- 4. 为AppSettings实现加载和保存方法 ---
impl AppSettings {
    /// 获取配置文件的路径
    fn get_config_path(path_resolver: &PathResolver) -> PathBuf {
        path_resolver
            .app_config_dir()
            .expect("无法获取应用配置目录")
            .join("settings.json")
    }

    /// 从JSON文件中加载设置
    pub fn load(path_resolver: &PathResolver) -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Self::get_config_path(path_resolver);
        if config_path.exists() {
            let content = fs::read_to_string(config_path)?;
            let settings: AppSettings = serde_json::from_str(&content)?;
            Ok(settings)
        } else {
            // 如果文件不存在，返回默认设置
            Ok(Self::default())
        }
    }

    /// 将当前设置保存到JSON文件
    pub fn save(&self, path_resolver: &PathResolver) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = Self::get_config_path(path_resolver);
        // 确保目录存在
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(config_path, content)?;
        Ok(())
    }
}

// --- 5. 定义与前端交互的Tauri指令 ---

/// [Tauri指令] 获取当前的应用设置
#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<AppSettings, String> {
    // 从全局状态中安全地读取设置
    Ok(state.settings.lock().unwrap().clone())
}

/// [Tauri指令] 保存前端传来的新设置
#[tauri::command]
pub async fn set_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<(), String> {
    // 打印接收到的新设置，便于调试
    println!("接收到新设置: {:?}", settings);

    // --- 1. 更新并保存设置文件 ---
    let path_resolver = app.path_resolver();
    settings
        .save(&path_resolver)
        .map_err(|e| format!("保存设置文件失败: {}", e))?;

    // --- 2. 更新内存中的全局状态 ---
    let old_shortcut;
    {
        let mut app_settings = state.settings.lock().unwrap();
        old_shortcut = app_settings.shortcut.clone();
        *app_settings = settings.clone();
    } // Mutex锁在这里自动释放

    // --- 3. 处理快捷键变更 ---
    if old_shortcut != settings.shortcut {
        println!("快捷键已变更，从 {} 变为 {}", old_shortcut, settings.shortcut);
        let mut shortcut_manager = app.global_shortcut_manager();
        shortcut_manager.unregister_all().map_err(|e| e.to_string())?;

        // --- 核心修正：所有权问题解决方案 ---
        // 在创建 `move` 闭包之前，先克隆一份 `AppHandle`。
        // 这样，闭包将移动这个克隆体的所有权，而原始的 `app` 变量仍可在后续代码中使用。
        let app_for_closure = app.clone();

        shortcut_manager
            .register(&settings.shortcut, move || {
                // `move` 关键字捕获并移动了 `app_for_closure` 的所有权。
                let app_handle = app_for_closure.clone();
                if let Some(window) = app_handle.get_window("screenshot") {
                    window.show().unwrap();
                    window.set_focus().unwrap();
                } else {
                    // 理想情况下，这里应该也处理窗口不存在的情况，但为了简化，我们遵循之前的逻辑
                    eprintln!("未找到截图窗口，无法执行快捷键操作");
                }
            })
            .map_err(|e| e.to_string())?;
    }

    // --- 4. 处理开机自启设置变更 ---
    // 因为上面的所有权问题已解决，这里的 `app` 变量现在是有效的。
    let autostart_manager = app.autolaunch();

    let is_enabled = autostart_manager.is_enabled().unwrap_or(false);

    if settings.autostart && !is_enabled {
        autostart_manager.enable().map_err(|e| e.to_string())?;
        println!("开机自启已启用");
    } else if !settings.autostart && is_enabled {
        autostart_manager.disable().map_err(|e| e.to_string())?;
        println!("开机自启已禁用");
    }

    Ok(())
}