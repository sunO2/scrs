//! 设备池相关的类型定义

use serde::{Deserialize, Serialize};
use std::fmt;

/// 设备状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceStatus {
    /// 设备已注册但未连接
    Registered,
    /// 正在连接中
    Connecting,
    /// 已连接，ScrcpyConnect 可用
    Connected,
    /// Agent 正在运行
    Busy,
    /// 连接断开
    Disconnected,
    /// 设备离线
    Offline,
    /// 错误状态
    Error(String),
}

impl fmt::Display for DeviceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceStatus::Registered => write!(f, "已注册"),
            DeviceStatus::Connecting => write!(f, "连接中"),
            DeviceStatus::Connected => write!(f, "已连接"),
            DeviceStatus::Busy => write!(f, "忙碌"),
            DeviceStatus::Disconnected => write!(f, "已断开"),
            DeviceStatus::Offline => write!(f, "离线"),
            DeviceStatus::Error(msg) => write!(f, "错误: {}", msg),
        }
    }
}

/// 设备池配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePoolConfig {
    /// 最大并发连接数
    pub max_connections: usize,

    /// 连接超时时间（秒）
    pub connection_timeout: u64,

    /// 空闲设备清理时间（秒）
    pub idle_cleanup_threshold: u64,

    /// 是否自动重连
    pub auto_reconnect: bool,

    /// 健康检查间隔（秒）
    pub health_check_interval: u64,
}

impl Default for DevicePoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            connection_timeout: 30,
            idle_cleanup_threshold: 300, // 5 分钟
            auto_reconnect: true,
            health_check_interval: 60,
        }
    }
}

/// 设备池事件
#[derive(Debug, Clone)]
pub enum DevicePoolEvent {
    /// 设备注册
    DeviceRegistered { serial: String },

    /// 设备连接
    DeviceConnected { serial: String },

    /// 设备断开
    DeviceDisconnected { serial: String },

    /// Agent 创建
    AgentCreated { serial: String, agent_id: String },

    /// Agent 销毁
    AgentDestroyed { serial: String, agent_id: String },

    /// 设备空闲
    DeviceIdle { serial: String, idle_seconds: u64 },

    /// 任务开始
    TaskStarted { serial: String, task: String },

    /// 任务完成
    TaskCompleted { serial: String, result: String },

    /// 任务失败
    TaskFailed { serial: String, error: String },

    /// 错误事件
    Error { serial: String, error: String },
}

/// 设备池错误
#[derive(Debug, thiserror::Error)]
pub enum DevicePoolError {
    #[error("设备未找到: {0}")]
    DeviceNotFound(String),

    #[error("设备已存在: {0}")]
    DeviceAlreadyExists(String),

    #[error("连接超时")]
    ConnectionTimeout,

    #[error("连接失败: {0}")]
    ConnectionError(String),

    #[error("已达最大连接数限制: {0}")]
    MaxConnectionsReached(usize),

    #[error("设备状态错误: {0}")]
    InvalidState(String),

    #[error("Agent 创建失败: {0}")]
    AgentCreationFailed(String),
}

/// 设备信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub serial: String,
    pub name: Option<String>,
    pub status: DeviceStatus,
    pub has_agent: bool,
    pub last_used: i64, // timestamp
    pub idle_seconds: i64,
}
