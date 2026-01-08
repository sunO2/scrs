use serde::{Deserialize, Serialize};

/// LLM 请求消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: MessageContent,
}

/// 消息角色
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

/// 消息内容（支持文本和图片）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Multimodal(Vec<ContentBlock>),
}

/// 内容块
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<ImageUrl>,
}

/// 图片 URL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
}

impl ImageUrl {
    /// 从 base64 创建图片 URL
    pub fn from_base64(base64_data: &str) -> Self {
        Self {
            url: format!("data:image/png;base64,{}", base64_data),
        }
    }
}

/// LLM 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub stream: Option<bool>,
}

/// LLM 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: Option<String>,
    pub object: Option<String>,
    pub created: Option<u64>,
    pub model: Option<String>,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

/// 选择项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub index: usize,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

/// Token 使用情况
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// 模型配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// 模型提供商（openai, local, azure）
    pub provider: String,

    /// 模型名称
    pub model_name: String,

    /// API 密钥
    pub api_key: String,

    /// API 基础 URL
    pub base_url: String,

    /// 最大 tokens
    pub max_tokens: u32,

    /// 温度
    pub temperature: f32,

    /// Top P
    pub top_p: f32,

    /// 超时时间（秒）
    pub timeout: u64,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: "openai".to_string(),
            model_name: "gpt-4o".to_string(),
            api_key: "".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            top_p: 0.85,
            timeout: 30,
        }
    }
}

impl ModelConfig {
    /// 从环境变量加载配置
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
            config.api_key = api_key;
        }

        if let Ok(base_url) = std::env::var("OPENAI_BASE_URL") {
            config.base_url = base_url;
        }

        if let Ok(model_name) = std::env::var("OPENAI_MODEL_NAME") {
            config.model_name = model_name;
        }

        config
    }

    /// 创建本地模型配置
    pub fn local(base_url: String, model_name: String) -> Self {
        Self {
            provider: "local".to_string(),
            model_name,
            api_key: "EMPTY".to_string(),
            base_url,
            max_tokens: 4096,
            temperature: 0.0,
            top_p: 0.85,
            timeout: 60,
        }
    }

    /// 创建 Azure OpenAI 配置
    pub fn azure(
        api_key: String,
        endpoint: String,
        deployment: String,
        api_version: String,
    ) -> Self {
        Self {
            provider: "azure".to_string(),
            model_name: deployment,
            api_key,
            base_url: format!("{}/openai/deployments/{}", endpoint, api_version),
            max_tokens: 4096,
            temperature: 0.0,
            top_p: 0.85,
            timeout: 30,
        }
    }
}
