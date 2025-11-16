use serde::{Deserialize, Serialize};

// --- 1. 定义与DeepL API交互的数据结构 ---

// 发送给DeepL API的请求体结构
#[derive(Debug, Serialize)]
struct DeepLRequest {
    text: Vec<String>,
    target_lang: String,
    source_lang: Option<String>, // (可选) 指定源语言
}

// 从DeepL API接收的响应体结构
#[derive(Debug, Deserialize)]
struct DeepLResponse {
    translations: Vec<Translation>,
}

// 单个翻译结果的结构
#[derive(Debug, Deserialize)]
struct Translation {
    // `serde(rename)` 属性用于将JSON中的字段名映射到Rust结构体的字段名
    #[serde(rename = "detected_source_language")]
    _detected_source_language: String, // 我们暂时用不到这个字段，所以用下划线开头
    text: String,
}


// --- 2. 定义统一的翻译器Trait（接口） ---
#[async_trait::async_trait]
pub trait Translator {
    async fn translate(
        &self,
        text: &str,
        target_lang: &str,
    ) -> Result<String, String>;
}


// --- 3. 实现DeepL翻译器 ---

pub struct DeepLTranslator {
    api_key: String,
}

impl DeepLTranslator {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

#[async_trait::async_trait]
impl Translator for DeepLTranslator {
    /// 实现翻译方法
    async fn translate(
        &self,
        text: &str,
        target_lang: &str,
    ) -> Result<String, String> {
        // --- a. 确定API URL ---
        let api_url = if self.api_key.ends_with(":fx") {
            "https://api-free.deepl.com/v2/translate"
        } else {
            "https://api.deepl.com/v2/translate"
        };

        // --- b. 构建请求体 ---
        let request_body = DeepLRequest {
            text: vec![text.to_string()],
            target_lang: target_lang.to_string(),
            source_lang: None,
        };

        // --- c. 发送HTTP请求 ---
        let client = reqwest::Client::new();
        let res = client
            .post(api_url)
            .header("Authorization", format!("DeepL-Auth-Key {}", self.api_key))
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("发送翻译请求失败: {}", e))?;

        // --- d. 处理响应 ---
        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(format!("翻译API返回错误: {} - {}", status, body));
        }

        // --- e. 解析JSON并返回结果 ---
        let deepl_response: DeepLResponse = res
            .json()
            .await
            .map_err(|e| format!("解析翻译结果失败: {}", e))?;

        deepl_response
            .translations
            .get(0)
            .map(|t| t.text.clone())
            .ok_or_else(|| "API响应中未找到翻译结果".to_string())
    }
}

/// 辅助函数，根据API Key创建并返回一个翻译器实例
// 核心修正：
// 1. 参数名改为 `api_key` 以反映其真实内容。
// 2. 函数体直接使用传入的 `api_key`。
pub fn get_translator(api_key: String) -> Box<dyn Translator + Send + Sync> {
    // 删除错误的行: `let settings = state.settings.lock().unwrap();`

    // 直接使用传入的 api_key 创建 DeepLTranslator 实例
    Box::new(DeepLTranslator::new(api_key))
}