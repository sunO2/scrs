use thiserror::Error;

/// 应用程序统一错误类型
#[derive(Error, Debug)]
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
}

/// AppError 的 Result 类型别名
pub type Result<T> = std::result::Result<T, AppError>;

impl AppError {
    /// 将错误转换为 HTTP 状态码
    pub fn status_code(&self) -> u16 {
        match self {
            AppError::DeviceNotFound(_) => 404,
            AppError::DeviceAlreadyConnected(_) => 409,
            AppError::DeviceNotConnected(_) => 400,
            AppError::AdbError(_) => 500,
            AppError::ScrcpyError(_) => 500,
            AppError::IoError(_) => 500,
            AppError::JsonError(_) => 400,
            AppError::Unknown(_) => 500,
        }
    }
}
