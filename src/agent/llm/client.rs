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

    async fn query_with_messages(
        &self,
        messages: Vec<crate::agent::core::traits::ChatMessage>,
        screenshot: Option<&str>,
    ) -> Result<ModelResponse, ModelError> {
        debug!("查询 LLM，消息数量: {}", messages.len());

        // 转换消息格式
        let mut api_messages = vec![];

        // 找到最后一条用户消息的索引（用于添加截图）
        let last_user_msg_index = messages.iter().rposition(|msg| {
            matches!(msg.role, crate::agent::core::traits::MessageRole::User)
        });

        for (idx, msg) in messages.iter().enumerate() {
            let role = match msg.role {
                crate::agent::core::traits::MessageRole::System => {
                    crate::agent::llm::types::MessageRole::System
                }
                crate::agent::core::traits::MessageRole::User => {
                    crate::agent::llm::types::MessageRole::User
                }
                crate::agent::core::traits::MessageRole::Assistant => {
                    crate::agent::llm::types::MessageRole::Assistant
                }
            };

            // 只在最后一条用户消息中添加截图
            let is_last_user_msg = last_user_msg_index == Some(idx);

            let content = if is_last_user_msg && screenshot.is_some() {
                crate::agent::llm::types::MessageContent::Multimodal(vec![
                    crate::agent::llm::types::ContentBlock {
                        block_type: "image_url".to_string(),
                        text: None,
                        image_url: Some(crate::agent::llm::types::ImageUrl::from_base64(screenshot.unwrap())),
                    },
                    crate::agent::llm::types::ContentBlock {
                        block_type: "text".to_string(),
                        text: Some(msg.content.clone()),
                        image_url: None,
                    },
                ])
            } else {
                crate::agent::llm::types::MessageContent::Text(msg.content.clone())
            };

            api_messages.push(crate::agent::llm::types::ChatMessage { role, content });
        }

        // 构建请求
        let request = ChatRequest {
            model: self.config.model_name.clone(),
            messages: api_messages,
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
            confidence: 0.8,
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
