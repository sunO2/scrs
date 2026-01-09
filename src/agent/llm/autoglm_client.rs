use async_trait::async_trait;
use reqwest::Client;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn, error};
use tokio_stream::StreamExt;
use crate::agent::core::traits::{ModelClient, ModelResponse, ModelError, ModelInfo, ChatMessage, MessageRole};
use crate::agent::llm::types::{ChatRequest, ModelConfig, MessageContent, ChatMessage as ApiChatMessage, MessageRole as ApiMessageRole};
use crate::agent::llm::prompts;
use serde::{Deserialize, Serialize};

// 导入 ActionEnum 用于解析响应
use crate::agent::actions::base::ActionEnum;

/// AutoGLM 流式响应的增量数据
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum StreamEvent {
    #[serde(rename = "token")]
    Token { token: String },
    #[serde(rename = "message_end")]
    MessageEnd,
}

/// AutoGLM 性能指标
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceMetrics {
    /// 首个 token 时间（秒）
    pub time_to_first_token: Option<f64>,
    /// 思考结束时间（秒）
    pub time_to_thinking_end: Option<f64>,
    /// 总推理时间（秒）
    pub total_time: f64,
}

/// AutoGLM 客户端，支持流式响应和特殊标记解析
pub struct AutoGLMClient {
    /// 主客户端，用于主要操作决策
    client: Client,
    /// 辅助客户端，用于修正和规范化主模型的输出
    auxiliary_client: Client,
    /// 模型配置
    config: ModelConfig,
}

impl AutoGLMClient {
    /// 创建新的 AutoGLM 客户端
    pub fn new(config: ModelConfig) -> Result<Self, ModelError> {
        info!("创建 AutoGLM 客户端: {}", config.model_name);
        info!("  API 端点: {}", config.base_url);
        info!("  超时时间: {}s", config.timeout);
        info!("  API Key: {}...", &config.api_key[..config.api_key.len().min(10)]);

        // 显示辅助模型配置
        if let Some(ref aux_name) = config.auxiliary_model_name {
            info!("  辅助模型: {}", aux_name);
        } else {
            info!("  未配置辅助模型");
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout))
            .connect_timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(120))
            .tcp_keepalive(Duration::from_secs(600))
            .build()
            .map_err(|e| ModelError::ApiError(format!("创建 HTTP 客户端失败: {}", e)))?;

        // 创建辅助客户端（使用相同的配置）
        let auxiliary_client = Client::builder()
            .timeout(Duration::from_secs(config.timeout))
            .connect_timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(120))
            .tcp_keepalive(Duration::from_secs(600))
            .build()
            .map_err(|e| ModelError::ApiError(format!("创建辅助 HTTP 客户端失败: {}", e)))?;

        Ok(Self {
            client,
            auxiliary_client,
            config,
        })
    }

    /// 发送流式聊天请求
    async fn send_stream_request(&self, request: ChatRequest) -> Result<String, ModelError> {
        let url = format!("{}/chat/completions", self.config.base_url);

        debug!("发送 AutoGLM 流式请求到: {}", url);

        let mut stream_request = request.clone();
        stream_request.stream = Some(true);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&stream_request)
            .send()
            .await
            .map_err(|e| ModelError::NetworkError(format!("发送请求失败: {}", e)))?;

        let status = response.status();

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "无法读取错误响应".to_string());

            error!("AutoGLM 请求失败: {} - {}", status, error_text);

            if status.as_u16() == 401 {
                return Err(ModelError::InvalidApiKey);
            }

            if status.as_u16() == 429 {
                return Err(ModelError::RateLimit);
            }

            return Err(ModelError::ApiError(format!(
                "请求失败: {} - {}",
                status, error_text
            )));
        }

        // 处理流式响应
        let mut full_content = String::new();
        let mut byte_stream = response.bytes_stream();

        while let Some(chunk_result) = byte_stream.next().await {
            let chunk = chunk_result
                .map_err(|e| ModelError::NetworkError(format!("读取流数据失败: {}", e)))?;

            let chunk_str = String::from_utf8_lossy(&chunk);
            full_content.push_str(&chunk_str);
        }

        Ok(full_content)
    }

    /// 发送非流式聊天请求
    async fn send_request(&self, request: ChatRequest) -> Result<ChatResponse, ModelError> {
        let url = format!("{}/chat/completions", self.config.base_url);

        info!("发送 AutoGLM 请求到: {}", url);
        info!("  模型: {}", request.model);
        info!("  消息数: {}", request.messages.len());

        // 发送请求并处理错误
        match self._send_request(&url, &request, &self.client, &self.config.api_key).await {
            Ok(response) => Ok(response),
            Err(e) => {
                error!("AutoGLM 请求失败: {}", e);
                error!("请检查:");
                error!("  1. API Key 是否正确设置");
                error!("  2. 网络连接是否正常");
                error!("  3. API 端点是否可访问: {}", self.config.base_url);
                error!("  4. 是否有足够的配额");
                Err(e)
            }
        }
    }

    /// 使用辅助模型发送请求以修正响应
    async fn send_auxiliary_request(&self, original_content: &str) -> Result<String, ModelError> {
        // 如果没有配置辅助模型名称，直接返回原始内容
        let aux_model_name = match &self.config.auxiliary_model_name {
            Some(name) => name,
            None => {
                debug!("未配置辅助模型，跳过响应修正");
                return Ok(original_content.to_string());
            }
        };

        info!("使用辅助模型修正响应: {}", aux_model_name);

        let url = format!("{}/chat/completions", self.config.base_url);

        // 构建辅助模型请求
        let system_prompt = prompts::get_auxiliary_system_prompt();
        let user_message = format!("请修正以下输出，使其符合格式要求：\n\n{}", original_content);

        let api_messages = vec![
            ApiChatMessage {
                role: ApiMessageRole::System,
                content: MessageContent::Text(system_prompt),
            },
            ApiChatMessage {
                role: ApiMessageRole::User,
                content: MessageContent::Text(user_message),
            },
        ];

        let request = ChatRequest {
            model: aux_model_name.clone(),
            messages: api_messages,
            max_tokens: Some(2048),
            temperature: Some(0.0),
            top_p: Some(0.85),
            stream: Some(false),
        };

        let chat_response = self._send_request(&url, &request, &self.auxiliary_client, &self.config.api_key).await?;

        // 提取修正后的内容
        let choice = chat_response.choices.first().ok_or_else(|| {
            ModelError::ParseError("辅助模型响应中没有选择项".to_string())
        })?;

        let corrected_content = match &choice.message.content {
            MessageContent::Text(text) => text.clone(),
            _ => original_content.to_string(),
        };

        info!("辅助模型修正完成");
        debug!("原始内容: {}", original_content);
        debug!("修正后内容: {}", corrected_content);

        Ok(corrected_content)
    }

    async fn _send_request(
        &self,
        url: &str,
        request: &ChatRequest,
        client: &Client,
        api_key: &str,
    ) -> Result<ChatResponse, ModelError> {
        // 打印请求详情（选择性输出，过滤图片数据）
        info!("========== AutoGLM 请求 ==========");
        info!("URL: {}", url);
        info!("模型: {}", request.model);
        info!("参数: max_tokens={:?}, temperature={:?}, top_p={:?}, stream={:?}",
            request.max_tokens, request.temperature, request.top_p, request.stream);
        info!("消息数量: {}", request.messages.len());
        info!("================================");

        let response = client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(|e| {
                error!("🔴 AutoGLM 网络请求失败");
                error!("   URL: {}", url);
                error!("   错误类型: {:?}", e);

                // 提供更详细的诊断信息
                if e.is_timeout() {
                    error!("   错误: 请求超时");
                    error!("   可能的原因:");
                    error!("   1. 网络连接不稳定");
                    error!("   2. API 服务器响应缓慢");
                    error!("   3. 请求太大，处理时间过长");
                    error!("   建议:");
                    error!("   - 检查网络连接");
                    error!("   - 增加 timeout 时间");
                    error!("   - 减小请求大小（如减少图片数量）");
                } else if e.is_connect() {
                    error!("   错误: 无法连接到服务器");
                    error!("   可能的原因:");
                    error!("   1. 网络未连接");
                    error!("   2. API 服务器地址错误: {}", url);
                    error!("   3. 防火墙或代理阻止连接");
                    error!("   4. DNS 解析失败");
                    error!("   建议:");
                    error!("   - 检查网络连接");
                    error!("   - 验证 API URL 是否正确");
                    error!("   - 检查防火墙设置");
                    error!("   - 尝试使用 VPN");
                } else {
                    error!("   其他网络错误");
                    error!("   原始错误: {}", e);
                }

                ModelError::NetworkError(format!("发送请求失败: {}", e))
            })?;

        let status = response.status();
        debug!("响应状态: {}", status);

        let response_text = response
            .text()
            .await
            .map_err(|e| ModelError::NetworkError(format!("读取响应失败: {}", e)))?;

        // 打印响应详情
        info!("========== AutoGLM 响应 ==========");
        info!("状态码: {}", status);
        info!("响应体 ({} 字节):", response_text.len());
        info!("================================");

        if !status.is_success() {
            warn!("AutoGLM 请求失败: {} - {}", status, response_text);

            if status.as_u16() == 401 {
                error!("API Key 无效");
                return Err(ModelError::InvalidApiKey);
            }

            if status.as_u16() == 429 {
                error!("请求过于频繁，触发限流");
                return Err(ModelError::RateLimit);
            }

            return Err(ModelError::ApiError(format!(
                "请求失败: {} - {}",
                status, response_text
            )));
        }

        let chat_response: ChatResponse = serde_json::from_str(&response_text).map_err(|e| {
            warn!("解析 AutoGLM 响应失败: {}", e);
            warn!("响应内容: {}", &response_text);
            ModelError::ParseError(format!("解析响应失败: {}", e))
        })?;

        Ok(chat_response)
    }

    /// 解析 AutoGLM 响应（使用 ActionEnum 的通用解析方法）
    fn parse_response(&self, content: &str) -> (Option<String>, Vec<ActionEnum>) {
        ActionEnum::parse_from_response(content)
    }
}

#[async_trait]
impl ModelClient for AutoGLMClient {
    async fn query_with_messages(
        &self,
        messages: Vec<ChatMessage>,
        screenshot: Option<&str>,
    ) -> Result<ModelResponse, ModelError> {
        debug!("查询 AutoGLM，消息数量: {}", messages.len());

        let start_time = Instant::now();

        // 转换消息格式
        let mut api_messages = vec![];

        // 找到最后一条用户消息的索引（用于添加截图）
        let last_user_msg_index = messages.iter().rposition(|msg| {
            matches!(msg.role, MessageRole::User)
        });

        for (idx, msg) in messages.iter().enumerate() {
            let role = match msg.role {
                MessageRole::System => ApiMessageRole::System,
                MessageRole::User => ApiMessageRole::User,
                MessageRole::Assistant => ApiMessageRole::Assistant,
            };

            // 只在最后一条用户消息中添加截图
            let is_last_user_msg = last_user_msg_index == Some(idx);

            let content = if is_last_user_msg && screenshot.is_some() {
                MessageContent::Multimodal(vec![
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
                MessageContent::Text(msg.content.clone())
            };

            api_messages.push(ApiChatMessage { role, content });
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

        let mut content = match &choice.message.content {
            MessageContent::Text(text) => text.clone(),
            _ => "".to_string(),
        };

        // 使用辅助模型优化响应（如果配置了辅助模型名称）
        if self.config.auxiliary_model_name.is_some() {
            info!("主模型响应无法解析，使用辅助模型修正");
            match self.send_auxiliary_request(&content).await {
                Ok(corrected_content) => {
                    content = corrected_content;
                },
                Err(e) => {
                    warn!("辅助模型修正失败: {}, 使用原始响应", e);
                    // 继续使用原始响应
                }
            }
        }

        let total_time = start_time.elapsed().as_secs_f64();

        // 使用 AutoGLM 特殊解析
        let (thinking, parsed_actions) = self.parse_response(&content);

        let usage = chat_response.usage.unwrap_or(Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        // 打印性能指标
        info!("📊 AutoGLM 性能指标:");
        info!("   总推理时间: {:.3}s", total_time);
        info!("   使用 tokens: {}", usage.total_tokens);
        if let Some(ref t) = thinking {
            info!("   思考过程: {}", t);
        }
        info!("   解析到的操作数: {}", parsed_actions.len());
        info!("   完整响应: {}", &content);

        Ok(ModelResponse {
            content: content.clone(),
            actions: parsed_actions,
            confidence: 0.8,
            reasoning: thinking,
            tokens_used: usage.total_tokens,
        })
    }

    fn info(&self) -> ModelInfo {
        ModelInfo {
            name: self.config.model_name.clone(),
            provider: self.config.provider.clone(),
            supports_vision: true,
            max_tokens: self.config.max_tokens,
            context_window: 8192, // AutoGLM-Phone-9B 的上下文窗口
        }
    }
}

/// ChatResponse 类型（如果未在 types.rs 中定义）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: Option<String>,
    pub object: Option<String>,
    pub created: Option<u64>,
    pub model: Option<String>,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub index: usize,
    pub message: ApiChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::core::traits::Action;

    #[test]
    fn test_parse_finish_action() {
        let client = AutoGLMClient::new(ModelConfig::default()).unwrap();
        let response = r#"Thinking...
finish(message="Task completed successfully")"#;

        let (thinking, actions) = client.parse_response(response);

        // 验证 action 解析成功
        assert!(!actions.is_empty());
        assert_eq!(actions.len(), 1);
        // 验证 action 类型为 FinishAction
        assert_eq!(actions[0].action_type(), "finish");
        // thinking 可能是 None（因为没有 <thinking> 标签）
        assert!(thinking.is_none() || thinking.as_ref().unwrap() == "Thinking...");
    }

    #[test]
    fn test_parse_do_action() {
        let client = AutoGLMClient::new(ModelConfig::default()).unwrap();
        let response = r#"Analyzing screen...
do(action="Tap", element=[500, 800])"#;

        let (thinking, actions) = client.parse_response(response);

        // 验证 thinking 部分（应该是 None，因为没有 <thinking> 标签）
        assert!(thinking.is_none());

        // 验证 action 解析成功
        assert!(!actions.is_empty());
        assert_eq!(actions.len(), 1);
        // 验证 action 类型为 TapAction
        assert_eq!(actions[0].action_type(), "tap");
    }

    #[test]
    fn test_parse_thinking_with_do() {
        let client = AutoGLMClient::new(ModelConfig::default()).unwrap();
        let response = r#"<thinking>I should tap the button at coordinates 100, 200</thinking>
do(action="Tap", element=[100, 200])"#;

        let (thinking, actions) = client.parse_response(response);

        // 验证 thinking 部分（从 <thinking> 标签提取）
        assert_eq!(thinking, Some("I should tap the button at coordinates 100, 200".to_string()));

        // 验证 action 解析成功
        assert!(!actions.is_empty());
        assert_eq!(actions.len(), 1);
        // 验证 action 类型为 TapAction
        assert_eq!(actions[0].action_type(), "tap");
    }

    #[test]
    fn test_parse_no_markers() {
        let client = AutoGLMClient::new(ModelConfig::default()).unwrap();
        let response = r#"Some random text without markers"#;

        let (thinking, actions) = client.parse_response(response);

        // thinking 应该为 None（没有 <thinking> 标签），actions 应该为空
        assert!(thinking.is_none());
        assert!(actions.is_empty());
    }

    #[test]
    fn test_parse_priority() {
        let client = AutoGLMClient::new(ModelConfig::default()).unwrap();

        // finish(message= 优先级最高
        let response1 = r#"Text...
do(action=tap)
finish(message="done")"#;
        let (thinking, actions) = client.parse_response(response1);
        // thinking 应该是 None（没有 <thinking> 标签）
        assert!(thinking.is_none());
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action_type(), "finish");

        // do(action= 第二优先级
        let response2 = r#"<thinking>Thought</thinking>
<answer>answer content</answer>
do(action="Launch", app="微信")"#;
        let (thinking, actions) = client.parse_response(response2);
        // thinking 应该是 Some("Thought")
        assert_eq!(thinking, Some("Thought".to_string()));
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action_type(), "launch");
    }

    #[test]
    fn test_parse_do_action_launch() {
        let client = AutoGLMClient::new(ModelConfig::default()).unwrap();
        let response = r#"I need to open WeChat.
do(action="Launch", app="微信")"#;

        println!("Testing response: {:?}", response);
        let (thinking, actions) = client.parse_response(response);

        println!("Got thinking: {:?}", thinking);
        println!("Got actions: {:?}", actions);

        // thinking 应该是 None（因为没有 <thinking> 标签）
        assert!(thinking.is_none());
        assert!(!actions.is_empty());
        // 验证 action 类型为 LaunchAction
        assert_eq!(actions[0].action_type(), "launch");
    }

    #[test]
    fn test_parse_do_action_wait() {
        let client = AutoGLMClient::new(ModelConfig::default()).unwrap();
        let response = r#"应用正在加载中
do(action="Wait", duration=1, message="应用正在加载中，请稍等。")"#;

        let (thinking, actions) = client.parse_response(response);

        // thinking 应该是 None（没有 <thinking> 标签）
        assert!(thinking.is_none());
        assert!(!actions.is_empty());
        // 验证 action 类型为 WaitAction
        assert_eq!(actions[0].action_type(), "wait");
    }

    #[test]
    fn test_parse_finish_multiline() {
        let client = AutoGLMClient::new(ModelConfig::default()).unwrap();
        let response = r#"finish(message="抱歉，我无法找到"什么值得买"这个应用。

不过，我可以为您打开一些类似的应用来浏览购物或推荐内容，比如：
- 淘宝
- 美团

您想打开哪个应用来浏览？")"#;

        let (thinking, actions) = client.parse_response(response);

        // thinking 应该是 None（没有 <thinking> 标签）
        assert!(thinking.is_none());
        assert!(!actions.is_empty());
        // 验证 action 类型为 FinishAction
        assert_eq!(actions[0].action_type(), "finish");

        // 验证多行消息被正确解析
        if let ActionEnum::Finish(ref finish) = actions[0] {
            assert!(finish.result.contains("抱歉，我无法找到"));
            assert!(finish.result.contains("什么值得买"));
            assert!(finish.result.contains("淘宝"));
            assert!(finish.result.contains("美团"));
        } else {
            panic!("Expected FinishAction");
        }
    }
}
