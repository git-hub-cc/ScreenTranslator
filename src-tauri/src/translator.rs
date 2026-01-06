use serde::{Deserialize};
use tauri::AppHandle;
use std::process::Command;
use encoding_rs::GBK;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[derive(Debug, Deserialize)]
struct LocalTranslationResponse {
    code: i32,
    translated_text: Option<String>,
    error_message: Option<String>,
}

#[async_trait::async_trait]
pub trait Translator {
    async fn translate(
        &self,
        text: &str,
        target_lang: &str,
    ) -> Result<String, String>;
}

pub struct LocalTranslator {
    app_handle: AppHandle,
}

impl LocalTranslator {
    pub fn new(app_handle: AppHandle) -> Self {
        Self { app_handle }
    }
}

#[async_trait::async_trait]
impl Translator for LocalTranslator {
    async fn translate(
        &self,
        text: &str,
        target_lang: &str,
    ) -> Result<String, String> {
        // --- 修改核心逻辑：指向本地数据目录 ---
        let local_data_dir = self.app_handle.path_resolver().app_local_data_dir()
            .ok_or_else(|| "无法获取本地数据目录".to_string())?;

        // 可执行文件名更改为 translate_engine.exe
        let translator_exe_path = local_data_dir.join("translate_engine.exe");

        println!("[TRANSLATOR] 检查翻译引擎: 路径='{:?}', 是否存在={}", translator_exe_path, translator_exe_path.exists());

        if !translator_exe_path.exists() {
            return Err("找不到翻译引擎，请在设置页面下载安装。".to_string());
        }

        let source_lang = if target_lang == "en" { "zh" } else { "en" };

        println!("[TRANSLATOR] 翻译请求: 源语言='{}', 目标语言='{}', 文本='{}...'", source_lang, target_lang, text.chars().take(50).collect::<String>());

        // 确保工作目录为可执行文件所在目录，以便加载依赖
        let working_dir = translator_exe_path.parent().unwrap();

        let mut command = Command::new(&translator_exe_path);
        command.current_dir(working_dir)
            .args(&[
                "--text", text,
                "--source", source_lang,
                "--target", target_lang,
            ]);

        #[cfg(windows)]
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW

        let output = command
            .output()
            .map_err(|e| format!("执行翻译进程失败: {}", e))?;

        println!("[TRANSLATOR] 进程执行完毕. Status: {:?}", output.status);

        if !output.status.success() {
            let stderr = GBK.decode(&output.stderr).0.into_owned();
            eprintln!("[TRANSLATOR] 进程执行出错, Status: {:?}, Stderr: {}", output.status, stderr);
            return Err(format!("翻译进程执行出错: {}", stderr));
        }

        let (decoded_stdout, _, _) = GBK.decode(&output.stdout);
        let stdout = decoded_stdout.into_owned();
        println!("[TRANSLATOR] 原始输出 (stdout): {}", stdout);

        let response: LocalTranslationResponse = serde_json::from_str(&stdout)
            .map_err(|e| format!("解析翻译结果JSON失败: {}. 原始输出: {}", e, stdout))?;
        println!("[TRANSLATOR] 解析到的响应: {:?}", response);

        match response.code {
            200 => response.translated_text.ok_or_else(|| "翻译成功但未返回文本".to_string()),
            _ => Err(response.error_message.unwrap_or_else(|| "翻译器返回未知错误".to_string())),
        }
    }
}

pub fn get_translator(app: &AppHandle) -> Box<dyn Translator + Send + Sync> {
    Box::new(LocalTranslator::new(app.clone()))
}