use async_trait::async_trait;
use reqwest::Client;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn, error};
use tokio_stream::StreamExt;
use crate::agent::core::traits::{ModelClient, ModelResponse, ModelError, ModelInfo, ParsedAction};
use crate::agent::llm::types::{ChatRequest, ModelConfig};
use serde::{Deserialize, Serialize};

/// 获取系统提示词
pub fn get_system_prompt(screen_width: u32, screen_height: u32) -> String {
    let current_date = chrono::Local::now().format("%Y年%m月%d日").to_string();
    format!(r#"#
The current date:  {current_date}

# Device Information
- Screen Resolution: {screen_width}x{screen_height}
- Screen Width: {screen_width} pixels
- Screen Height: {screen_height} pixels

# Setup
You are a professional Android operation agent assistant that can fulfill the user's high-level instructions. Given a screenshot of the Android interface at each step, you first analyze the situation, then plan the best course of action using Python-style pseudo-code.

# More details about the code
Your response format must be structured as follows:

Think first: Use <think>...</think> to analyze the current screen, identify key elements, and determine the most efficient action.
Provide the action: Use <answer>...</answer> to return a single line of pseudo-code representing the operation.

Your output should STRICTLY follow the format:
<think>
[Your thought]
</think>
<answer>
[Your operation code]
</answer>

- **Tap**
  Perform a tap action on a specified screen area. The element is a list of 2 integers, representing the coordinates of the tap point.
  **Example**:
  <answer>
  do(action="Tap", element=[x,y])
  </answer>
- **Type**
  Enter text into the currently focused input field.
  **Example**:
  <answer>
  do(action="Type", text="Hello World")
  </answer>
- **Swipe**
  Perform a swipe action with start point and end point.
  **Examples**:
  <answer>
  do(action="Swipe", start=[x1,y1], end=[x2,y2])
  </answer>
- **Long Press**
  Perform a long press action on a specified screen area.
  You can add the element to the action to specify the long press area. The element is a list of 2 integers, representing the coordinates of the long press point.
  **Example**:
  <answer>
  do(action="Long Press", element=[x,y])
  </answer>
- **Launch**
  Launch an app. Try to use launch action when you need to launch an app. Check the instruction to choose the right app before you use this action.
  **Example**:
  <answer>
  do(action="Launch", app="Settings")
  </answer>
- **Back**
  Press the Back button to navigate to the previous screen.
  **Example**:
  <answer>
  do(action="Back")
  </answer>
- **Finish**
  Terminate the program and optionally print a message.
  **Example**:
  <answer>
  finish(message="Task completed.")
  </answer>


REMEMBER:
- Think before you act: Always analyze the current UI and the best course of action before executing any step, and output in <think> part.
- Only ONE LINE of action in <answer> part per response: Each step must contain exactly one line of executable code.
- Generate execution code strictly according to format requirements."#,)
}

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
    client: Client,
    config: ModelConfig,
}

impl AutoGLMClient {
    /// 创建新的 AutoGLM 客户端
    pub fn new(config: ModelConfig) -> Result<Self, ModelError> {
        info!("创建 AutoGLM 客户端: {}", config.model_name);
        info!("  API 端点: {}", config.base_url);
        info!("  超时时间: {}s", config.timeout);
        info!("  API Key: {}...", &config.api_key[..config.api_key.len().min(10)]);

        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout))
            .connect_timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(120))
            .tcp_keepalive(Duration::from_secs(600))
            .build()
            .map_err(|e| ModelError::ApiError(format!("创建 HTTP 客户端失败: {}", e)))?;

        Ok(Self { client, config })
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
        match self._send_request(&url, &request).await {
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

    async fn _send_request(&self, url: &str, request: &ChatRequest) -> Result<ChatResponse, ModelError> {
        // 打印请求详情（选择性输出，过滤图片数据）
        info!("========== AutoGLM 请求 ==========");
        info!("URL: {}", url);
        info!("模型: {}", request.model);
        info!("参数: max_tokens={:?}, temperature={:?}, top_p={:?}, stream={:?}",
            request.max_tokens, request.temperature, request.top_p, request.stream);
        info!("消息数量: {}", request.messages.len());
        info!("================================");

        let response = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(|e| {
                error!("网络请求错误: {}", e);
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

    /// 解析 AutoGLM 响应（支持特殊标记）
    ///
    /// 解析规则（严格按照 Python 代码）：
    /// 1. 如果包含 'finish(message='，之前的是 thinking，使用 parser 解析 action
    /// 2. 如果包含 'do(action='，之前的是 thinking，使用 parser 解析 action
    /// 3. 如果包含 '<answer>'，使用 XML 标签解析，然后用 parser 解析 action
    /// 4. 否则，thinking 为空，尝试用 parser 解析全部内容
    fn parse_response(&self, content: &str) -> (String, Option<ParsedAction>) {
        use crate::agent::llm::parser::{try_parse_do_action, try_parse_finish_action, parse_action_from_response};

        // 规则 1: 检查 finish(message=
        if let Some(pos) = content.find("finish(message=") {
            let thinking = content[..pos].trim().to_string();
            let action_str = "finish(message=".to_string() + &content[pos + 16..];

            // 使用 parser 解析 finish action
            if let Some(action) = try_parse_finish_action(&action_str) {
                return (thinking, Some(action));
            }
            // 如果解析失败，返回原始 action 字符串
            let action = ParsedAction {
                action_type: "raw".to_string(),
                parameters: serde_json::json!({ "raw": action_str }),
                reasoning: action_str.clone(),
            };
            return (thinking, Some(action));
        }

        // 规则 2: 检查 do(action=
        if let Some(pos) = content.find("do(action=") {
            let thinking = content[..pos].trim().to_string();
            let action_str = "do(action=".to_string() + &content[pos + 10..];

            // 使用 parser 解析 do action
            if let Some(action) = try_parse_do_action(&action_str) {
                return (thinking, Some(action));
            }
            // 如果解析失败，返回原始 action 字符串
            let action = ParsedAction {
                action_type: "raw".to_string(),
                parameters: serde_json::json!({ "raw": action_str }),
                reasoning: action_str.clone(),
            };
            return (thinking, Some(action));
        }

        // 规则 3: 回退到 XML 标签解析
        // Python 代码: thinking = parts[0].replace("<thinking>", "").replace("</thinking>", "").strip()
        if let Some(start) = content.find("<answer>") {
            if let Some(end) = content.find("</answer>") {
                // 提取 <answer> 之前的内容作为 thinking
                let thinking_raw = &content[..start];
                // 移除 <thinking> 和 </thinking> 标签（只移除标签，保留中间内容）
                let thinking = thinking_raw
                    .replace("<thinking>", "")
                    .replace("</thinking>", "")
                    .trim()
                    .to_string();
                let action_content = content[start + 8..end].to_string(); // 8 = len("<answer>")

                // 尝试解析 action
                if let Ok(Some(action)) = parse_action_from_response(&action_content) {
                    return (thinking, Some(action));
                }

                // 如果解析失败，返回原始 action 字符串
                let action = ParsedAction {
                    action_type: "raw".to_string(),
                    parameters: serde_json::json!({ "raw": action_content }),
                    reasoning: action_content,
                };
                return (thinking, Some(action));
            }
        }

        // 规则 4: 没有找到标记，thinking 为空，尝试用 parser 解析全部内容
        if let Ok(Some(action)) = parse_action_from_response(content) {
            return (String::new(), Some(action));
        }

        // 如果所有解析都失败，返回原始内容作为 action
        let action = ParsedAction {
            action_type: "raw".to_string(),
            parameters: serde_json::json!({ "raw": content }),
            reasoning: content.to_string(),
        };
        (String::new(), Some(action))
    }

}

#[async_trait]
impl ModelClient for AutoGLMClient {
    async fn query_with_messages(
        &self,
        messages: Vec<crate::agent::core::traits::ChatMessage>,
        screenshot: Option<&str>,
    ) -> Result<ModelResponse, ModelError> {
        debug!("查询 AutoGLM，消息数量: {}", messages.len());

        let start_time = Instant::now();

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

        let total_time = start_time.elapsed().as_secs_f64();

        // 使用 AutoGLM 特殊解析
        let (thinking, parsed_action) = self.parse_response(&content);

        let usage = chat_response.usage.unwrap_or(Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        // 打印性能指标
        info!("📊 AutoGLM 性能指标:");
        info!("   总推理时间: {:.3}s", total_time);
        info!("   使用 tokens: {}", usage.total_tokens);
        info!("   思考过程: {}", &content);

        Ok(ModelResponse {
            content: content.clone(),
            action: parsed_action,
            confidence: 0.8,
            reasoning: if thinking.is_empty() { None } else { Some(thinking) },
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
    pub message: crate::agent::llm::types::ChatMessage,
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

    #[test]
    fn test_parse_finish_action() {
        let client = AutoGLMClient::new(ModelConfig::default()).unwrap();
        let response = r#"Thinking...
finish(message="Task completed successfully")"#;

        let (thinking, action) = client.parse_response(response);

        // 验证 thinking 部分
        assert_eq!(thinking, "Thinking...");

        // 验证 action 解析成功
        assert!(action.is_some());
        let action = action.unwrap();
        assert_eq!(action.action_type, "finish");
        assert_eq!(action.parameters.get("result").unwrap().as_str().unwrap(), "Task completed successfully");
    }

    #[test]
    fn test_parse_do_action() {
        let client = AutoGLMClient::new(ModelConfig::default()).unwrap();
        let response = r#"Analyzing screen...
do(action="Tap", element=[500, 800])"#;

        let (thinking, action) = client.parse_response(response);

        // 验证 thinking 部分
        assert_eq!(thinking, "Analyzing screen...");

        // 验证 action 解析成功
        assert!(action.is_some());
        let action = action.unwrap();
        assert_eq!(action.action_type, "tap");
        // element 应该被解析为数组
        assert!(action.parameters.get("element").is_some());
    }

    #[test]
    fn test_parse_xml_answer_with_json() {
        let client = AutoGLMClient::new(ModelConfig::default()).unwrap();
        let response = r#"<thinking>I should tap the button</thinking>
<answer>{"action_type": "tap", "x": 100, "y": 200}</answer>"#;

        let (thinking, action) = client.parse_response(response);

        // 验证 thinking 部分（移除 <thinking> 标签后）
        assert_eq!(thinking.trim(), "I should tap the button");

        // 验证 action 解析成功
        assert!(action.is_some());
        let action = action.unwrap();
        assert_eq!(action.action_type, "tap");
        assert_eq!(action.parameters.get("x").unwrap().as_u64().unwrap(), 100);
        assert_eq!(action.parameters.get("y").unwrap().as_u64().unwrap(), 200);
    }

    #[test]
    fn test_parse_no_markers() {
        let client = AutoGLMClient::new(ModelConfig::default()).unwrap();
        let response = r#"Some random text without markers"#;

        let (thinking, action) = client.parse_response(response);

        // 规则 4: thinking 应该为空
        assert!(thinking.is_empty());

        // 无法解析的内容返回 raw
        assert!(action.is_some());
        let action = action.unwrap();
        assert_eq!(action.action_type, "raw");
        assert_eq!(action.reasoning, "Some random text without markers");
    }

    #[test]
    fn test_parse_priority() {
        let client = AutoGLMClient::new(ModelConfig::default()).unwrap();

        // finish(message= 优先级最高
        let response1 = r#"Text...
do(action=tap)
finish(message="done")"#;
        let (thinking, action) = client.parse_response(response1);
        assert!(thinking.contains("Text..."));
        assert_eq!(action.unwrap().action_type, "finish");

        // do(action= 第二优先级
        let response2 = r#"<thinking>Thought</thinking>
<answer>answer content</answer>
do(action="Launch", app="微信")"#;
        let (_thinking, action) = client.parse_response(response2);
        assert_eq!(action.unwrap().action_type, "launch");
    }

    #[test]
    fn test_parse_do_action_launch() {
        let client = AutoGLMClient::new(ModelConfig::default()).unwrap();
        let response = r#"I need to open WeChat.
do(action="Launch", app="微信")"#;

        let (thinking, action) = client.parse_response(response);

        assert_eq!(thinking, "I need to open WeChat.");
        assert!(action.is_some());
        let action = action.unwrap();
        assert_eq!(action.action_type, "launch");
        assert_eq!(action.parameters.get("app").unwrap().as_str().unwrap(), "微信");
    }

    #[test]
    fn test_parse_do_action_wait() {
        let client = AutoGLMClient::new(ModelConfig::default()).unwrap();
        let response = r#"应用正在加载中
do(action="Wait", duration=1, message="应用正在加载中，请稍等。")"#;

        let (thinking, action) = client.parse_response(response);

        assert_eq!(thinking, "应用正在加载中");
        assert!(action.is_some());
        let action = action.unwrap();
        assert_eq!(action.action_type, "wait");
        assert_eq!(action.parameters.get("duration").unwrap().as_u64().unwrap(), 1);
        assert_eq!(action.parameters.get("message").unwrap().as_str().unwrap(), "应用正在加载中，请稍等。");
    }
}
