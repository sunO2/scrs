use thiserror::Error;

/// 应用程序统一错误类型
#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum AppError {
    /// 设备未找到
    #[error("设备未找到: {0}")]
    DeviceNotFound(String),

    /// 设备已连接
    #[error("设备已连接: {0}")]
    DeviceAlreadyConnected(String),

    /// 设备未连接
    #[error("设备未连接: {0}")]
    DeviceNotConnected(String),

    /// ADB 错误
    #[error("ADB 错误: {0}")]
    AdbError(String),

    /// Scrcpy 错误
    #[error("Scrcpy 错误: {0}")]
    ScrcpyError(String),

    /// IO 错误
    #[error("IO 错误: {0}")]
    IoError(#[from] std::io::Error),

    /// JSON 错误
    #[error("JSON 错误: {0}")]
    JsonError(#[from] serde_json::Error),

    /// 未知错误
    #[error("未知错误: {0}")]
    Unknown(String),

    /// Agent 错误
    #[error("Agent 错误: {0}")]
    AgentError(#[from] crate::agent::core::traits::AgentError),

    /// Model 错误
    #[error("Model 错误: {0}")]
    ModelError(#[from] crate::agent::core::traits::ModelError),

    /// Action 错误
    #[error("Action 错误: {0}")]
    ActionError(#[from] crate::agent::core::traits::ActionError),
}
