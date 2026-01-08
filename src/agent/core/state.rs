use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Agent 状态机
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentState {
    Idle,
    Initializing,
    Analyzing { step: usize },
    Executing { step: usize, action: String },
    Waiting { step: usize, reason: String },
    Paused { step: usize },
    Completed { steps: usize, duration_ms: u64 },
    Failed { step: usize, error: String },
}

/// Agent 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// 每个任务的最大步数
    pub max_steps: usize,

    /// 每个任务的最大执行时间（秒）
    pub max_execution_time: u64,

    /// 操作之间的延迟（毫秒）
    pub action_delay: u32,

    /// 截图质量 (1-100)
    pub screenshot_quality: u8,

    /// 启用自动重试
    pub enable_retry: bool,

    /// 最大重试次数
    pub max_retries: u32,

    /// 启用安全验证
    pub enable_safety: bool,

    /// 启用操作回滚
    pub enable_rollback: bool,

    /// 日志文件路径
    pub log_file: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_steps: 50,
            max_execution_time: 300, // 5 分钟
            action_delay: 1000,
            screenshot_quality: 80,
            enable_retry: true,
            max_retries: 3,
            enable_safety: true,
            enable_rollback: false,
            log_file: "logs/agent.log".to_string(),
        }
    }
}

/// 线程安全的 Agent 运行时状态
#[derive(Clone)]
pub struct AgentRuntime {
    pub state: Arc<RwLock<AgentState>>,
    pub config: AgentConfig,
    pub current_task: Arc<RwLock<Option<String>>>,
    pub execution_history: Arc<RwLock<Vec<super::traits::ExecutionStep>>>,
    pub step_counter: Arc<RwLock<usize>>,
    pub start_time: Arc<RwLock<Option<chrono::DateTime<chrono::Utc>>>>,
}

impl AgentRuntime {
    pub fn new(config: AgentConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(AgentState::Idle)),
            config,
            current_task: Arc::new(RwLock::new(None)),
            execution_history: Arc::new(RwLock::new(Vec::new())),
            step_counter: Arc::new(RwLock::new(0)),
            start_time: Arc::new(RwLock::new(None)),
        }
    }

    /// 重置运行时状态
    pub async fn reset(&self) {
        *self.state.write().await = AgentState::Idle;
        *self.current_task.write().await = None;
        self.execution_history.write().await.clear();
        *self.step_counter.write().await = 0;
        *self.start_time.write().await = None;
    }

    /// 获取已用时间（毫秒）
    pub async fn elapsed_ms(&self) -> u64 {
        if let Some(start) = *self.start_time.read().await {
            (chrono::Utc::now() - start).num_milliseconds() as u64
        } else {
            0
        }
    }

    /// 添加执行步骤
    pub async fn add_step(&self, step: super::traits::ExecutionStep) {
        self.execution_history.write().await.push(step);
    }

    /// 增加步数计数器
    pub async fn increment_step(&self) -> usize {
        let mut counter = self.step_counter.write().await;
        *counter += 1;
        *counter
    }

    /// 获取当前步数
    pub async fn current_step(&self) -> usize {
        *self.step_counter.read().await
    }
}
