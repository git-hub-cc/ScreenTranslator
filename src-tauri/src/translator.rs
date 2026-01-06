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
        let translator_exe_path = self.app_handle
            .path_resolver()
            .resolve_resource("external/Translator/translate.exe")
            .ok_or_else(|| "在应用资源中找不到翻译器可执行文件".to_string())?;

        let source_lang = if target_lang == "en" { "zh" } else { "en" };

        // --- 日志：记录翻译请求详情 ---
        println!("[TRANSLATOR] 翻译请求: 源语言='{}', 目标语言='{}', 文本='{}...'", source_lang, target_lang, text.chars().take(50).collect::<String>());

        let mut command = Command::new(&translator_exe_path);
        command.args(&[
            "--text", text,
            "--source", source_lang,
            "--target", target_lang,
        ]);

        #[cfg(windows)]
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW

        let output = command
            .output()
            .map_err(|e| format!("执行翻译进程失败: {}", e))?;

        if !output.status.success() {
            let stderr = GBK.decode(&output.stderr).0.into_owned();
            // --- 日志：记录进程执行失败的详细信息 ---
            eprintln!("[TRANSLATOR] 进程执行出错, Status: {:?}, Stderr: {}", output.status, stderr);
            return Err(format!("翻译进程执行出错: {}", stderr));
        }

        let (decoded_stdout, _, _) = GBK.decode(&output.stdout);
        let stdout = decoded_stdout.into_owned();
        // --- 日志：记录进程的原始标准输出 ---
        println!("[TRANSLATOR] 原始输出 (stdout): {}", stdout);

        let response: LocalTranslationResponse = serde_json::from_str(&stdout)
            .map_err(|e| format!("解析翻译结果JSON失败: {}. 原始输出: {}", e, stdout))?;
        // --- 日志：记录解析后的响应 ---
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