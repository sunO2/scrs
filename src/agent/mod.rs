//! 手机自动化 Agent 模块
//!
//! 提供基于 AI 的手机自动化操作能力，支持多设备并发执行任务。

pub mod core;
pub mod actions;
pub mod llm;
pub mod executor;
pub mod context;
pub mod config;
pub mod api;
pub mod pool;
pub mod socket_server;
pub mod logger;

// 重新导出核心类型
pub use core::{
    traits::{Device, Action, Agent, ModelClient, AgentError},
    state::{AgentConfig, AgentState, AgentRuntime},
    agent::PhoneAgent,
    agent_group::{AgentGroup, AgentGroupConfig, AgentGroupEvent},
};

pub use actions::{
    ActionEnum, TapAction, LongPressAction, DoubleTapAction, SwipeAction, ScrollAction,
    TypeAction, PressKeyAction, BackAction, HomeAction, RecentAction, NotificationAction,
    LaunchAction, WaitAction, ScreenshotAction, FinishAction,
};
pub use llm::{ModelConfig, create_model_client};
pub use executor::{ScrcpyDeviceWrapper, ActionHandler};
pub use context::{ConversationContext, ShortTermMemory};
pub use config::{FullAgentConfig};
pub use pool::{DevicePool, DevicePoolConfig, DevicePoolEvent, DeviceStatus};
pub use socket_server::AgentSocketServer;

