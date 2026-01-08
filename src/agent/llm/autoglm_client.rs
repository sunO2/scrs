use async_trait::async_trait;
use reqwest::Client;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn, error};
use tokio_stream::StreamExt;
use crate::agent::core::traits::{ModelClient, ModelResponse, ModelError, ModelInfo, ParsedAction};
use crate::agent::llm::types::{ChatRequest, ModelConfig};
use serde::{Deserialize, Serialize};

/// 获取系统提示词
fn get_system_prompt() -> String {
    let current_date = chrono::Local::now().format("%Y年%m月%d日").to_string();

    format!(r#"# 角色定义
你是一个专业的 Android 手机自动化操作助手。你的任务是通过分析当前屏幕状态，理解用户的指令，然后决定下一步要执行的操作。

# 当前日期
{current_date}

# 核心原则
1. 仔细观察屏幕，理解当前的界面状态
2. 根据用户任务，判断当前状态与目标的差距
3. 选择最合适的操作，逐步完成任务
4. 每次只执行一个操作，不要尝试一次完成多个步骤
5. 如果任务完成或无法完成，使用 finish 操作

# 可用操作

## 1. Launch - 启动应用
启动指定的 Android 应用
**格式**: `do(action="Launch", app="应用名称")`
**参数**:
- app: 应用名称（如"微信"、"淘宝"、"支付宝"等）或包名

**示例**:
- `do(action="Launch", app="微信")` - 启动微信应用
- `do(action="Launch", app="com.tencent.mm")` - 使用包名启动微信

## 2. Tap - 点击
点击屏幕上的指定位置
**格式**: `do(action="Tap", x=100, y=200)` 或 `do(action="Tap", element=[500, 800])`
**参数**:
- x: X 坐标
- y: Y 坐标
- 或 element: [x, y] 数组格式

**示例**:
- `do(action="Tap", x=500, y=800)` - 点击坐标 (500, 800)
- `do(action="Tap", element=[360, 640])` - 点击坐标 (360, 640)

## 3. DoubleTap - 双击
快速双击屏幕指定位置
**格式**: `do(action="DoubleTap", x=100, y=200)`
**参数**: 与 Tap 相同

## 4. LongPress - 长按
长按屏幕指定位置
**格式**: `do(action="LongPress", x=100, y=200, duration_ms=1000)`
**参数**:
- x: X 坐标
- y: Y 坐标
- duration_ms: 长按时长（毫秒），可选，默认 1000ms

## 5. Swipe - 滑动
从起点滑动到终点
**格式**: `do(action="Swipe", start=[100, 200], end=[300, 400], duration_ms=500)`
**参数**:
- start: [start_x, start_y] 起点坐标
- end: [end_x, end_y] 终点坐标
- duration_ms: 滑动时长（毫秒），可选，默认 500ms

## 6. Scroll - 滚动
在屏幕上滚动
**格式**: `do(action="Scroll", direction="up", distance=0.5)`
**参数**:
- direction: "up"（向上滚动）或 "down"（向下滚动）
- distance: 滚动距离（屏幕高度的比例，0.0-1.0），可选，默认 0.5

## 7. Type - 输入文本
在当前焦点处输入文本
**格式**: `do(action="Type", text="要输入的文本")`
**参数**:
- text: 要输入的文本内容

## 8. PressKey - 按键
模拟物理按键
**格式**: `do(action="PressKey", keycode="HOME")`
**参数**:
- keycode: 按键名称，如 "HOME", "BACK", "ENTER" 等

## 9. Back - 返回
点击返回键
**格式**: `do(action="Back")`

## 10. Home - 主页
点击主页键
**格式**: `do(action="Home")`

## 11. Recent - 最近任务
打开最近任务界面
**格式**: `do(action="Recent")`

## 12. Notification - 通知栏
下拉通知栏
**格式**: `do(action="Notification")`

## 13. Wait - 等待
等待指定时间
**格式**: `do(action="Wait", duration_ms=1000)`
**参数**:
- duration_ms: 等待时长（毫秒）

## 14. Screenshot - 截图
获取当前屏幕截图
**格式**: `do(action="Screenshot")`

## 15. Finish - 完成任务
表示任务完成或无法完成
**格式**: `finish(message="任务说明")`
**参数**:
- message: 任务完成说明或失败原因

# 响应格式要求

## 重要提示
你必须严格按照以下格式输出操作，否则将无法被正确解析：

1. **操作格式**: 使用 `do(action="操作名", 参数1=值1, 参数2=值2)` 格式
2. **完成格式**: 使用 `finish(message="说明")` 格式
3. **参数值**: 字符串参数使用引号包裹，数字参数不需要引号
4. **一次一个**: 每次只输出一个操作
5. **清晰明确**: 不要使用模糊的描述

## 正确示例
```
用户任务: 打开微信发送消息给张三

分析: 我看到用户在主屏幕，需要先启动微信应用
do(action="Launch", app="微信")

分析: 微信已启动，我需要找到搜索框来搜索联系人
do(action="Tap", x=360, y=150)

分析: 我在搜索框中输入"张三"
do(action="Type", text="张三")

分析: 我看到搜索结果中第一个就是张三，点击打开对话
do(action="Tap", x=540, y=300)

分析: 我点击输入框准备输入消息
do(action="Tap", x=540, y=1800)

分析: 我输入消息内容
do(action="Type", text="你好，在吗？")

分析: 我点击发送按钮
do(action="Tap", x=980, y=1800)

分析: 消息已发送，任务完成
finish(message="已成功发送消息给张三")
```

## 错误示例（不要这样）
```
❌ 点击微信图标
❌ Launch app: WeChat
❌ {{"action": "tap", "x": 100, "y": 200}}
❌ 我要点击屏幕中间
```

# 思考流程
1. **观察屏幕**: 识别当前界面状态（主屏幕、应用内、对话框等）
2. **理解任务**: 明确用户的最终目标
3. **判断差距**: 当前状态与目标状态之间还缺少什么步骤
4. **选择操作**: 根据可用操作，选择最合适的一步
5. **确认参数**: 为操作提供准确的参数（坐标、文本等）
6. **输出操作**: 使用严格的格式输出

# 坐标系说明
- 屏幕坐标系: 原点在左上角，X轴向右，Y轴向下
- 常见屏幕尺寸: 1080x2400, 1440x3200 等
- 你需要根据截图准确判断点击位置

# 注意事项
1. **等待应用加载**: 启动应用或切换界面后，可能需要等待 1-2 秒
2. **处理弹窗**: 如果出现权限请求、广告等弹窗，先关闭它们
3. **网络延迟**: 涉及网络操作的步骤，等待时间可能需要更长
4. **失败处理**: 如果操作失败（如应用未安装），使用 finish 说明原因
5. **逐步完成**: 不要跳过中间步骤，一次只做一件事

# 常见应用包名参考
- 微信: com.tencent.mm
- 支付宝: com.eg.android.AlipayGphone
- 淘宝: com.taobao.taobao
- 抖音: com.ss.android.ugc.aweme
- QQ: com.tencent.mobileqq
- 设置: com.android.settings
- 浏览器: com.android.browser

# 总结
你的核心任务是: 观察屏幕 → 理解任务 → 选择操作 → 输出格式化的操作指令。严格按照 `do(action="...", ...)` 或 `finish(message="...")` 格式输出，确保每次只执行一个明确操作。"#)
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
            .connect_timeout(Duration::from_secs(10))
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(60))
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

        // 打印请求详情（调试用）
        if let Err(e) = self._send_request(&url, &request).await {
            error!("AutoGLM 请求失败: {}", e);
            error!("请检查:");
            error!("  1. API Key 是否正确设置");
            error!("  2. 网络连接是否正常");
            error!("  3. API 端点是否可访问: {}", self.config.base_url);
            error!("  4. 是否有足够的配额");
            return Err(e);
        } else {
            return self._send_request(&url, &request).await;
        }
    }

    async fn _send_request(&self, url: &str, request: &ChatRequest) -> Result<ChatResponse, ModelError> {
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

        debug!("响应内容长度: {} {} 字节", response_text, response_text.len());

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
            warn!("响应内容: {}", &response_text[..response_text.len().min(500)]);
            ModelError::ParseError(format!("解析响应失败: {}", e))
        })?;

        Ok(chat_response)
    }

    /// 解析 AutoGLM 响应（支持特殊标记）
    ///
    /// 解析规则：
    /// 1. 如果包含 'finish(message='，之前的是 thinking，从标记开始的是 action
    /// 2. 如果包含 'do(action='，之前的是 thinking，从标记开始的是 action
    /// 3. 如果包含 '<answer>'，使用 XML 标签解析
    /// 4. 否则，全部内容作为 action
    fn parse_response(&self, content: &str) -> (String, Option<ParsedAction>) {
        // 规则 1: 检查 finish(message=
        if content.contains("finish(message=") {
            let parts: Vec<&str> = content.splitn(2, "finish(message=").collect();
            let thinking = parts[0].trim().to_string();
            let action_str = "finish(message=".to_string() + parts.get(1).unwrap_or(&"");

            if let Some(action) = self.parse_autoglm_action(&action_str) {
                return (thinking, Some(action));
            }
        }

        // 规则 2: 检查 do(action=
        if content.contains("do(action=") {
            let parts: Vec<&str> = content.splitn(2, "do(action=").collect();
            let thinking = parts[0].trim().to_string();
            let action_str = "do(action=".to_string() + parts.get(1).unwrap_or(&"");

            if let Some(action) = self.parse_autoglm_action(&action_str) {
                return (thinking, Some(action));
            }
        }

        // 规则 3: 回退到 XML 标签解析
        if content.contains("<answer>") {
            if let Some(start) = content.find("<answer>") {
                if let Some(end) = content.find("</answer>") {
                    let thinking = content[..start]
                        .replace("", "")
                        .replace("", "")
                        .trim()
                        .to_string();
                    let action_content = &content[start + 8..end]; // 8 = len("<answer>")

                    // 尝试解析 action
                    if let Some(action) = self.parse_action_from_text(action_content) {
                        return (thinking, Some(action));
                    }
                }
            }
        }

        // 规则 4: 没有找到标记，返回全部内容
        (String::new(), self.parse_action_from_text(content))
    }

    /// 解析 AutoGLM 特殊格式的 action
    fn parse_autoglm_action(&self, action_str: &str) -> Option<ParsedAction> {
        // 解析 finish(message="...")
        if action_str.starts_with("finish(message=") {
            if let Some(end) = action_str.find(')') {
                let message = &action_str[16..end]; // 16 = len("finish(message=")
                return Some(ParsedAction {
                    action_type: "finish".to_string(),
                    parameters: serde_json::json!({
                        "result": message.trim_matches('"'),
                        "success": true
                    }),
                    reasoning: action_str.to_string(),
                });
            }
        }

        // 解析 do(action=...)
        if action_str.starts_with("do(action=") {
            // 提取 action 名称
            let remaining = &action_str[10..]; // 10 = len("do(action=")

            // 尝试找到动作名称的结束位置
            if let Some(end) = remaining.find(|c| c == '(' || c == ',' || c == ')') {
                let action_name = &remaining[..end];

                // 尝试解析参数
                let parameters = if let Some(params_start) = remaining.find('(') {
                    if let Some(params_end) = remaining[params_start..].find(')') {
                        let params_str = &remaining[params_start + 1..params_start + params_end];
                        self.parse_action_params(params_str)
                    } else {
                        serde_json::json!({})
                    }
                } else {
                    serde_json::json!({})
                };

                return Some(ParsedAction {
                    action_type: action_name.to_string(),
                    parameters,
                    reasoning: action_str.to_string(),
                });
            }
        }

        // 回退到常规解析
        self.parse_action_from_text(action_str)
    }

    /// 解析 action 参数字符串
    fn parse_action_params(&self, params_str: &str) -> serde_json::Value {
        let mut params = serde_json::Map::new();

        for param in params_str.split(',') {
            let param = param.trim();
            if let Some(eq_pos) = param.find('=') {
                let key = &param[..eq_pos];
                let value = &param[eq_pos + 1..];

                // 尝试解析值
                let parsed_value = if value.contains('"') {
                    // 字符串值
                    serde_json::json!(value.trim_matches('"').to_string())
                } else {
                    // 尝试解析为数字
                    value.parse::<i64>()
                        .map(|v| serde_json::json!(v))
                        .unwrap_or_else(|_| serde_json::json!(value))
                };

                params.insert(key.to_string(), parsed_value);
            }
        }

        serde_json::Value::Object(params)
    }

    /// 从文本解析 action
    fn parse_action_from_text(&self, text: &str) -> Option<ParsedAction> {
        use crate::agent::llm::parser;

        // 首先尝试常规解析
        if let Ok(Some(action)) = parser::parse_action_from_response(text) {
            return Some(action);
        }

        // 尝试解析 JSON 格式
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
            if let Some(action_type) = json.get("action_type")
                .or(json.get("type"))
                .and_then(|v| v.as_str())
            {
                return Some(ParsedAction {
                    action_type: action_type.to_string(),
                    parameters: json,
                    reasoning: text.to_string(),
                });
            }
        }

        None
    }
}

#[async_trait]
impl ModelClient for AutoGLMClient {
    async fn query(
        &self,
        prompt: &str,
        screenshot: Option<&str>,
    ) -> Result<ModelResponse, ModelError> {
        debug!("查询 AutoGLM，提示词长度: {}", prompt.len());

        let start_time = Instant::now();

        // 构建消息
        let mut messages = vec![];

        // 添加系统提示
        let system_prompt = get_system_prompt();
        messages.push(crate::agent::llm::types::ChatMessage {
            role: crate::agent::llm::types::MessageRole::System,
            content: crate::agent::llm::types::MessageContent::Text(system_prompt),
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
            stream: Some(false), // 暂时使用非流式
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
        if !thinking.is_empty() {
            info!("   思考过程: {}", thinking);
        }

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
        assert!(!thinking.is_empty());
        assert!(action.is_some());
        assert_eq!(action.unwrap().action_type, "finish");
    }

    #[test]
    fn test_parse_do_action() {
        let client = AutoGLMClient::new(ModelConfig::default()).unwrap();
        let response = r#"Analyzing screen...
do(action=tap, x=100, y=200)"#;

        let (thinking, action) = client.parse_response(response);
        assert!(action.is_some());
        assert_eq!(action.unwrap().action_type, "tap");
    }

    #[test]
    fn test_parse_xml_answer() {
        let client = AutoGLMClient::new(ModelConfig::default()).unwrap();
        let response = r#"<thinking>I should tap the button</thinking>
<answer>{"action_type": "tap", "x": 100, "y": 200}</answer>"#;

        let (thinking, action) = client.parse_response(response);
        assert!(action.is_some());
    }
}
