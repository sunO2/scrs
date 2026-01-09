use async_trait::async_trait;
use crate::error::AppError;

/// 设备抽象 trait，定义手机自动化操作接口
#[async_trait]
pub trait Device: Send + Sync {
    /// 获取设备序列号
    fn serial(&self) -> &str;

    /// 获取设备名称
    fn name(&self) -> &str;

    /// 检查设备是否连接
    async fn is_connected(&self) -> bool;

    /// 截取屏幕截图，返回 base64 编码的图片
    async fn screenshot(&self) -> Result<String, AppError>;

    /// 获取屏幕尺寸 (宽度, 高度)
    async fn screen_size(&self) -> Result<(u32, u32), AppError>;

    /// 发送点击事件
    async fn tap(&self, x: u32, y: u32) -> Result<(), AppError>;

    /// 发送滑动事件
    async fn swipe(
        &self,
        start_x: u32,
        start_y: u32,
        end_x: u32,
        end_y: u32,
        duration_ms: u32,
    ) -> Result<(), AppError>;

    /// 发送长按事件
    async fn long_press(&self, x: u32, y: u32, duration_ms: u32) -> Result<(), AppError>;

    /// 发送双击事件
    async fn double_tap(&self, x: u32, y: u32) -> Result<(), AppError>;

    /// 输入文本
    async fn input_text(&self, text: &str) -> Result<(), AppError>;

    /// 发送按键事件
    async fn press_key(&self, keycode: u32) -> Result<(), AppError>;

    /// 按下返回键
    async fn back(&self) -> Result<(), AppError>;

    /// 按下 Home 键
    async fn home(&self) -> Result<(), AppError>;

    /// 打开最近任务
    async fn recent(&self) -> Result<(), AppError>;

    /// 打开通知栏
    async fn notification(&self) -> Result<(), AppError>;

    /// 启动应用
    async fn launch_app(&self, package: &str) -> Result<(), AppError>;

    /// 获取当前应用包名
    async fn current_app(&self) -> Result<String, AppError>;
}

/// 操作 trait，定义所有设备操作的接口
pub trait Action: Send + Sync + std::fmt::Debug {
    /// 执行操作
    async fn execute(&self, device: &dyn Device) -> Result<ActionResult, AppError>;

    /// 验证操作参数
    fn validate(&self) -> Result<(), ActionError>;

    /// 获取操作描述
    fn description(&self) -> String;

    /// 获取操作类型（必需方法，用于 dyn 兼容）
    fn action_type(&self) -> String;

    /// 检查操作是否可逆
    fn is_reversible(&self) -> bool {
        false
    }

    /// 估算执行时间（毫秒）
    fn estimated_duration(&self) -> u32 {
        100
    }
}

/// 操作执行结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActionResult {
    pub success: bool,
    pub message: String,
    pub duration_ms: u32,
    pub screenshot_before: Option<String>,
    pub screenshot_after: Option<String>,
}

impl ActionResult {
    pub fn success(message: String, duration_ms: u32) -> Self {
        Self {
            success: true,
            message,
            duration_ms,
            screenshot_before: None,
            screenshot_after: None,
        }
    }

    pub fn failure(message: String, duration_ms: u32) -> Self {
        Self {
            success: false,
            message,
            duration_ms,
            screenshot_before: None,
            screenshot_after: None,
        }
    }
}

/// 操作相关错误
#[derive(thiserror::Error, Debug)]
pub enum ActionError {
    #[error("无效的参数: {0}")]
    InvalidParameters(String),

    #[error("坐标超出边界: ({x}, {y})")]
    OutOfBounds { x: u32, y: u32 },

    #[error("文本包含无效字符: {0}")]
    InvalidText(String),

    #[error("持续时间过短: {0}ms")]
    DurationTooShort(u32),

    #[error("持续时间过长: {0}ms")]
    DurationTooLong(u32),
}

/// Agent 相关错误
#[derive(thiserror::Error, Debug)]
pub enum AgentError {
    #[error("Agent 未找到: {0}")]
    NotFound(String),

    #[error("设备未找到: {0}")]
    DeviceNotFound(String),

    #[error("验证错误: {0}")]
    ValidationError(String),

    #[error("连接错误: {0}")]
    ConnectionError(String),

    #[error("超时错误: {0}")]
    TimeoutError(String),

    #[error("Agent 已在运行")]
    AlreadyRunning,

    #[error("Agent 未运行")]
    NotRunning,

    #[error("超过最大步数: {0}")]
    MaxStepsExceeded(usize),

    #[error("执行超时: {0} 秒")]
    ExecutionTimeout(u64),

    #[error("任务失败: {0}")]
    TaskFailed(String),

    #[error("无效的状态转换: 从 {0} 到 {1}")]
    InvalidStateTransition(String, String),

    #[error("恢复失败: {0}")]
    RecoveryFailed(String),
}

/// Agent trait，定义自主任务执行接口
#[async_trait]
pub trait Agent: Send + Sync {
    /// 启动 agent 执行任务
    async fn start(&self, task: String) -> Result<String, AppError>;

    /// 停止 agent 执行
    async fn stop(&self) -> Result<(), AppError>;

    /// 暂停 agent 执行
    async fn pause(&self) -> Result<(), AppError>;

    /// 恢复 agent 执行
    async fn resume(&self) -> Result<(), AppError>;

    /// 获取 agent 状态
    async fn status(&self) -> AgentStatus;

    /// 获取执行历史
    async fn history(&self) -> Vec<ExecutionStep>;

    /// 发送反馈给 agent
    async fn feedback(&self, feedback: AgentFeedback) -> Result<(), AppError>;
}

/// Agent 执行状态
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum AgentStatus {
    Idle,
    Running { task: String, step: usize },
    Paused { task: String, step: usize },
    Completed {
        task: String,
        steps: usize,
        duration_ms: u64,
    },
    Failed { task: String, error: String },
}

/// 单个执行步骤
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionStep {
    pub step_number: usize,
    pub action_type: String,
    pub action_description: String,
    pub result: ActionResult,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub screenshot: String,
    pub reasoning: String,
}

/// Agent 用户反馈
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum AgentFeedback {
    Positive,
    Negative { reason: String },
    Correction { correct_action: String },
}

/// LLM 客户端 trait
#[async_trait]
pub trait ModelClient: Send + Sync {
    /// 使用消息历史查询模型（支持多轮对话）
    async fn query_with_messages(
        &self,
        messages: Vec<ChatMessage>,
        screenshot: Option<&str>,
    ) -> Result<ModelResponse, ModelError>;

    /// 获取模型信息
    fn info(&self) -> ModelInfo;
}

/// 聊天消息（用于多轮对话）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

/// 消息角色
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

/// 模型响应
#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub content: String,
    pub action: Option<crate::agent::actions::base::ActionEnum>,
    pub confidence: f32,
    pub reasoning: Option<String>,
    pub tokens_used: u32,
}

/// 从模型响应中解析出的操作
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParsedAction {
    pub action_type: String,
    pub parameters: serde_json::Value,
    pub reasoning: String,
}

/// 模型相关错误
#[derive(thiserror::Error, Debug)]
pub enum ModelError {
    #[error("API 请求失败: {0}")]
    ApiError(String),

    #[error("解析响应失败: {0}")]
    ParseError(String),

    #[error("超出速率限制")]
    RateLimit,

    #[error("无效的 API 密钥")]
    InvalidApiKey,

    #[error("网络错误: {0}")]
    NetworkError(String),

    #[error("超时")]
    Timeout,
}

/// 模型信息
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub provider: String,
    pub supports_vision: bool,
    pub max_tokens: u32,
    pub context_window: u32,
}
