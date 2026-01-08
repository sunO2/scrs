use async_trait::async_trait;
use reqwest::Client;
use std::time::Duration;
use tracing::{debug, error, info};
use crate::agent::core::traits::{ModelClient, ModelResponse, ModelError, ModelInfo};
use crate::agent::llm::types::{ChatRequest, ChatResponse, ModelConfig};
use crate::agent::llm::parser::parse_action_from_response;

/// OpenAI 兼容的 LLM 客户端
pub struct OpenAIClient {
    client: Client,
    config: ModelConfig,
}

impl OpenAIClient {
    /// 创建新的 OpenAI 客户端
    pub fn new(config: ModelConfig) -> Result<Self, ModelError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout))
            .build()
            .map_err(|e| ModelError::ApiError(format!("创建 HTTP 客户端失败: {}", e)))?;

        Ok(Self { client, config })
    }

    /// 发送聊天请求
    async fn send_request(&self, request: ChatRequest) -> Result<ChatResponse, ModelError> {
        let url = format!("{}/chat/completions", self.config.base_url);

        debug!("发送 LLM 请求到: {}", url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ModelError::NetworkError(format!("发送请求失败: {}", e)))?;

        let status = response.status();
        let response_text = response
            .text()
            .await
            .map_err(|e| ModelError::NetworkError(format!("读取响应失败: {}", e)))?;

        if !status.is_success() {
            error!("LLM 请求失败: {} - {}", status, response_text);

            if status.as_u16() == 401 {
                return Err(ModelError::InvalidApiKey);
            }

            if status.as_u16() == 429 {
                return Err(ModelError::RateLimit);
            }

            return Err(ModelError::ApiError(format!(
                "请求失败: {} - {}",
                status, response_text
            )));
        }

        let chat_response: ChatResponse = serde_json::from_str(&response_text).map_err(|e| {
            error!("解析 LLM 响应失败: {}", e);
            ModelError::ParseError(format!("解析响应失败: {}", e))
        })?;

        Ok(chat_response)
    }
}

#[async_trait]
impl ModelClient for OpenAIClient {
    async fn query(
        &self,
        prompt: &str,
        screenshot: Option<&str>,
    ) -> Result<ModelResponse, ModelError> {
        debug!("查询 LLM，提示词长度: {}", prompt.len());

        // 构建消息
        let mut messages = vec![];

        // 添加系统提示
        messages.push(crate::agent::llm::types::ChatMessage {
            role: crate::agent::llm::types::MessageRole::System,
            content: crate::agent::llm::types::MessageContent::Text(
                "你是一个手机自动化助手。根据用户任务和当前屏幕截图，决定下一步要执行的操作。"
                    .to_string(),
            ),
        });

        // 添加用户消息（可能包含图片）
        let user_content = if let Some(screenshot) = screenshot {
            crate::agent::llm::types::MessageContent::Multimodal(vec![
                crate::agent::llm::types::ContentBlock {
                    block_type: "image_url".to_string(),
                    text: None,
                    image_url: Some(crate::agent::llm::types::ImageUrl::from_base64(screenshot)),
                },
                crate::agent::llm::types::ContentBlock {
                    block_type: "text".to_string(),
                    text: Some(prompt.to_string()),
                    image_url: None,
                },
            ])
        } else {
            crate::agent::llm::types::MessageContent::Text(prompt.to_string())
        };

        messages.push(crate::agent::llm::types::ChatMessage {
            role: crate::agent::llm::types::MessageRole::User,
            content: user_content,
        });

        // 构建请求
        let request = ChatRequest {
            model: self.config.model_name.clone(),
            messages,
            max_tokens: Some(self.config.max_tokens),
            temperature: Some(self.config.temperature),
            top_p: Some(self.config.top_p),
            stream: Some(false),
        };

        // 发送请求
        let chat_response = self.send_request(request).await?;

        // 解析响应
        let choice = chat_response.choices.first().ok_or_else(|| {
            ModelError::ParseError("响应中没有选择项".to_string())
        })?;

        let content = match &choice.message.content {
            crate::agent::llm::types::MessageContent::Text(text) => text.clone(),
            _ => "".to_string(),
        };

        let usage = chat_response.usage.unwrap_or(crate::agent::llm::types::Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        // 解析操作
        let action = parse_action_from_response(&content)?;

        Ok(ModelResponse {
            content: content.clone(),
            action,
            confidence: 0.8, // 可以从响应中解析
            reasoning: None,
            tokens_used: usage.total_tokens,
        })
    }

    fn info(&self) -> ModelInfo {
        ModelInfo {
            name: self.config.model_name.clone(),
            provider: self.config.provider.clone(),
            supports_vision: true,
            max_tokens: self.config.max_tokens,
            context_window: 128000, // GPT-4o 的上下文窗口
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_config_default() {
        let config = ModelConfig::default();
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model_name, "gpt-4o");
    }

    #[test]
    fn test_model_config_local() {
        let config = ModelConfig::local(
            "http://localhost:8000/v1".to_string(),
            "autoglm-phone-9b".to_string(),
        );
        assert_eq!(config.provider, "local");
        assert_eq!(config.base_url, "http://localhost:8000/v1");
    }
}
