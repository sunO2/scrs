use std::sync::Arc;
use tokio::sync::{RwLock, Mutex};
use tokio::task::AbortHandle;
use tracing::{debug, info, warn, error};
use crate::agent::core::traits::{Device, Agent, AgentStatus, AgentFeedback, ExecutionStep, ModelClient};
use crate::agent::core::traits::{ModelResponse, ParsedAction};
use crate::agent::core::state::{AgentRuntime, AgentConfig, AgentState};
use crate::agent::executor::ActionHandler;
use crate::agent::context::{ConversationContext, MessageRole, ShortTermMemory};
use crate::error::AppError;

/// 手机自动化 Agent
pub struct PhoneAgent {
    id: String,
    device: Arc<dyn Device>,
    runtime: AgentRuntime,
    model_client: Arc<dyn ModelClient>,
    action_handler: Arc<ActionHandler>,
    conversation: Arc<ConversationContext>,
    memory: Arc<ShortTermMemory>,
    abort_handle: Arc<Mutex<Option<AbortHandle>>>,
}

impl PhoneAgent {
    /// 创建新的 PhoneAgent
    pub fn new(
        id: String,
        device: Arc<dyn Device>,
        model_client: Arc<dyn ModelClient>,
        config: AgentConfig,
    ) -> Result<Self, AppError> {
        let action_handler = Arc::new(ActionHandler::new(Arc::clone(&device)));

        Ok(Self {
            id,
            device,
            runtime: AgentRuntime::new(config),
            model_client,
            action_handler,
            conversation: Arc::new(ConversationContext::new(50)),
            memory: Arc::new(ShortTermMemory::new(3600)), // 默认 TTL 1小时
            abort_handle: Arc::new(Mutex::new(None)),
        })
    }

    /// 获取 Agent ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 构建提示词
    async fn build_prompt(&self, task: &str, step: usize) -> String {
        let mut prompt = self.conversation.build_prompt(task).await;

        // 添加当前状态信息
        prompt.push_str(&format!("\n当前步骤: {}\n", step));

        // 添加屏幕信息
        if let Ok((width, height)) = self.device.screen_size().await {
            prompt.push_str(&format!("屏幕尺寸: {}x{}\n", width, height));
        }

        prompt.push_str("\n请分析当前屏幕并决定下一步操作。");

        prompt
    }

    /// 运行 Agent 主循环
    async fn run_agent_loop(&self, task: String) {
        info!("Agent {} 开始执行任务: {}", self.id, task);

        let mut step = 0;
        let mut no_action_count = 0; // 连续无操作计数

        loop {
            // 检查是否超过最大步数
            if step >= self.runtime.config.max_steps {
                self.fail(format!("超过最大步数限制: {}", step)).await;
                break;
            }

            // 检查连续无操作次数（防止无限循环）
            if no_action_count >= 3 {
                self.fail(format!("连续 {} 次未返回有效操作，停止执行", no_action_count)).await;
                break;
            }

            // 检查是否超时
            let elapsed = self.runtime.elapsed_ms().await;
            let max_time_ms = self.runtime.config.max_execution_time * 1000;
            if elapsed > max_time_ms {
                self.fail(format!("执行超时: {}ms > {}ms", elapsed, max_time_ms)).await;
                break;
            }

            // 更新状态为分析中
            *self.runtime.state.write().await = AgentState::Analyzing { step };

            // 截取屏幕
            debug!("步骤 {}: 截取屏幕", step);
            let screenshot = match self.device.screenshot().await {
                Ok(s) => s,
                Err(e) => {
                    self.fail(format!("截图失败: {}", e)).await;
                    break;
                }
            };

            // 构建提示词
            let prompt = self.build_prompt(&task, step).await;

            // 查询 LLM
            debug!("步骤 {}: 查询 LLM", step);
            let model_response = match self.model_client.query(&prompt, Some(&screenshot)).await {
                Ok(r) => r,
                Err(e) => {
                    self.fail(format!("LLM 查询失败: {}", e)).await;
                    break;
                }
            };

            // 检查是否有操作
            let parsed_action = match model_response.action {
                Some(action) => action,
                None => {
                    // 没有操作，可能只是思考，继续循环
                    debug!("步骤 {}: LLM 没有返回操作，继续", step);
                    no_action_count += 1;
                    step = self.runtime.increment_step().await;
                    continue;
                }
            };

            // 重置无操作计数
            no_action_count = 0;

            // 检查是否完成
            if parsed_action.action_type == "finish" {
                self.complete(step, model_response.content).await;
                break;
            }

            // 更新状态为执行中
            *self.runtime.state.write().await = AgentState::Executing {
                step,
                action: parsed_action.action_type.clone(),
            };

            // 执行操作
            debug!("步骤 {}: 执行操作 {:?}", step, parsed_action.action_type);
            let action_result = match self.action_handler.execute_parsed_action(&parsed_action).await {
                Ok(r) => r,
                Err(e) => {
                    warn!("操作执行失败: {}", e);
                    // 继续执行，不中断
                    crate::agent::core::traits::ActionResult::failure(
                        format!("操作失败: {}", e),
                        0
                    )
                }
            };

            // 记录步骤
            let execution_step = ExecutionStep {
                step_number: step,
                action_type: parsed_action.action_type.clone(),
                action_description: parsed_action.reasoning.clone(),
                result: action_result,
                timestamp: chrono::Utc::now(),
                screenshot: screenshot.clone(),
                reasoning: model_response.reasoning.unwrap_or_default(),
            };

            self.runtime.add_step(execution_step).await;

            // 添加到对话上下文
            self.conversation.add_message(
                crate::agent::context::MessageRole::Assistant,
                format!("执行操作: {}", parsed_action.action_type),
                Some(screenshot),
            ).await;

            // 增加步数
            step = self.runtime.increment_step().await;

            // 等待一段时间再继续
            tokio::time::sleep(std::time::Duration::from_millis(
                self.runtime.config.action_delay as u64,
            )).await;
        }
    }

    /// 标记为完成
    async fn complete(&self, steps: usize, result: String) {
        *self.runtime.state.write().await = AgentState::Completed {
            steps,
            duration_ms: self.runtime.elapsed_ms().await,
        };

        info!("Agent {} 完成任务: {}", self.id, result);
    }

    /// 标记为失败
    async fn fail(&self, error: String) {
        let step = self.runtime.current_step().await;
        let error_msg = error.clone();
        *self.runtime.state.write().await = AgentState::Failed {
            step,
            error,
        };

        error!("Agent {} 失败: {}", self.id, error_msg);
    }
}

#[async_trait::async_trait]
impl Agent for PhoneAgent {
    async fn start(&self, task: String) -> Result<String, AppError> {
        // 检查当前状态
        let state = self.runtime.state.read().await;
        if !matches!(*state, AgentState::Idle) {
            return Err(AppError::AgentError(
                crate::agent::core::traits::AgentError::AlreadyRunning,
            ));
        }
        drop(state);

        // 初始化
        *self.runtime.state.write().await = AgentState::Initializing;
        *self.runtime.current_task.write().await = Some(task.clone());
        *self.runtime.start_time.write().await = Some(chrono::Utc::now());

        // 在后台运行
        let agent_clone = PhoneAgent {
            id: self.id.clone(),
            device: Arc::clone(&self.device),
            runtime: self.runtime.clone(),
            model_client: Arc::clone(&self.model_client),
            action_handler: Arc::clone(&self.action_handler),
            conversation: Arc::clone(&self.conversation),
            memory: Arc::clone(&self.memory),
            abort_handle: Arc::clone(&self.abort_handle),
        };

        let handle = tokio::spawn(async move {
            agent_clone.run_agent_loop(task).await;
        });

        // 保存 abort handle
        *self.abort_handle.lock().await = Some(handle.abort_handle());

        Ok(self.id.clone())
    }

    async fn stop(&self) -> Result<(), AppError> {
        // 中止运行中的任务
        let mut handle_guard = self.abort_handle.lock().await;
        if let Some(handle) = handle_guard.take() {
            handle.abort();
        }

        // 重置状态
        self.runtime.reset().await;

        Ok(())
    }

    async fn pause(&self) -> Result<(), AppError> {
        let state = self.runtime.state.read().await;
        match &*state {
            AgentState::Analyzing { .. } | AgentState::Executing { .. } => {
                drop(state);
                let step = self.runtime.current_step().await;
                let task = self.runtime.current_task.read().await.clone().unwrap_or_default();
                *self.runtime.state.write().await = AgentState::Paused { step };
                Ok(())
            }
            _ => Err(AppError::AgentError(
                crate::agent::core::traits::AgentError::NotRunning,
            )),
        }
    }

    async fn resume(&self) -> Result<(), AppError> {
        let state = self.runtime.state.read().await;
        match &*state {
            AgentState::Paused { .. } => {
                drop(state);
                // TODO: 实现恢复逻辑
                Ok(())
            }
            _ => Err(AppError::AgentError(
                crate::agent::core::traits::AgentError::InvalidStateTransition(
                    "NotPaused".to_string(),
                    "Running".to_string(),
                ),
            )),
        }
    }

    async fn status(&self) -> AgentStatus {
        let state = self.runtime.state.read().await;
        let task = self.runtime.current_task.read().await.clone();

        match &*state {
            AgentState::Idle => AgentStatus::Idle,
            AgentState::Initializing => AgentStatus::Running {
                task: task.unwrap_or_default(),
                step: 0,
            },
            AgentState::Analyzing { step } => AgentStatus::Running {
                task: task.unwrap_or_default(),
                step: *step,
            },
            AgentState::Executing { step, .. } => AgentStatus::Running {
                task: task.unwrap_or_default(),
                step: *step,
            },
            AgentState::Waiting { step, .. } => AgentStatus::Running {
                task: task.unwrap_or_default(),
                step: *step,
            },
            AgentState::Paused { step } => AgentStatus::Paused {
                task: task.unwrap_or_default(),
                step: *step,
            },
            AgentState::Completed { steps, duration_ms } => AgentStatus::Completed {
                task: task.unwrap_or_default(),
                steps: *steps,
                duration_ms: *duration_ms,
            },
            AgentState::Failed { error, .. } => AgentStatus::Failed {
                task: task.unwrap_or_default(),
                error: error.clone(),
            },
        }
    }

    async fn history(&self) -> Vec<ExecutionStep> {
        self.runtime.execution_history.read().await.clone()
    }

    async fn feedback(&self, _feedback: AgentFeedback) -> Result<(), AppError> {
        // TODO: 实现反馈处理
        Ok(())
    }
}
