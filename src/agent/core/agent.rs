use std::sync::Arc;
use tokio::sync::{RwLock, Mutex};
use tokio::task::AbortHandle;
use tracing::{debug, info, warn, error};
use crate::agent::core::traits::{Device, Agent, AgentStatus, AgentFeedback, ExecutionStep, ModelClient, Action};
use crate::agent::core::state::{AgentRuntime, AgentConfig, AgentState};
use crate::agent::executor::ActionHandler;
use crate::agent::context::{ConversationContext, ShortTermMemory};
use crate::agent::logger::AgentLogger;
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
    messages: Arc<RwLock<Vec<crate::agent::core::traits::ChatMessage>>>,
    logger: Arc<AgentLogger>,
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

        // 创建日志记录器
        let log_dir = "logs/agent";
        let logger = Arc::new(AgentLogger::new(&id, log_dir)
            .map_err(|e| AppError::Unknown(format!("创建日志记录器失败: {}", e)))?);

        Ok(Self {
            id,
            device,
            runtime: AgentRuntime::new(config),
            model_client,
            action_handler,
            conversation: Arc::new(ConversationContext::new(50)),
            memory: Arc::new(ShortTermMemory::new(3600)), // 默认 TTL 1小时
            abort_handle: Arc::new(Mutex::new(None)),
            messages: Arc::new(RwLock::new(Vec::new())),
            logger,
        })
    }

    /// 获取 Agent ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 初始化消息列表（添加系统提示词）
    async fn initialize_messages(&self, system_prompt: String) {
        let mut messages = self.messages.write().await;
        messages.clear();
        messages.push(crate::agent::core::traits::ChatMessage {
            role: crate::agent::core::traits::MessageRole::System,
            content: system_prompt,
        });
    }

    /// 添加用户消息
    async fn add_user_message(&self, content: String) {
        let mut messages = self.messages.write().await;
        messages.push(crate::agent::core::traits::ChatMessage {
            role: crate::agent::core::traits::MessageRole::User,
            content,
        });
    }

    /// 添加助手消息（操作执行结果）
    async fn add_assistant_message(&self, content: String) {
        let mut messages = self.messages.write().await;
        messages.push(crate::agent::core::traits::ChatMessage {
            role: crate::agent::core::traits::MessageRole::Assistant,
            content,
        });
    }

    /// 运行 Agent 主循环
    async fn run_agent_loop(&self, task: String) {
        info!("Agent {} 开始执行任务: {}", self.id, task);

        // 记录任务开始
        if let Err(e) = self.logger.log_task_start(&task).await {
            warn!("记录任务开始失败: {}", e);
        }

        // 获取屏幕尺寸
        let (screen_width, screen_height) = match self.device.screen_size().await {
            Ok((w, h)) => (w, h),
            Err(e) => {
                warn!("获取屏幕尺寸失败: {}, 使用默认值 1080x2400", e);
                (1080, 2400) // 使用默认值
            }
        };

        // 初始化消息列表（添加系统提示词和用户任务）
        let system_prompt = crate::agent::llm::prompts::get_main_system_prompt(screen_width, screen_height);
        self.initialize_messages(system_prompt).await;

        // 添加初始用户任务
        let initial_user_message = format!(
            "任务: {}\n\n 请分析当前屏幕并决定 需要怎么操作。告诉我的操作尽量简洁 并且需要严格按照do(action= 格式回复 否则我无法解析",
            task
        );
        self.add_user_message(initial_user_message.clone()).await;

        let mut step = 0;
        let mut no_action_count = 0; // 连续无操作计数
        let loop_start_time = std::time::Instant::now();

        loop {
            // 检查是否超过最大步数
            if step >= self.runtime.config.max_steps {
                let error = format!("超过最大步数限制: {}", step);
                self.fail(error.clone()).await;
                if let Err(e) = self.logger.log_task_failed(&error, step).await {
                    warn!("记录任务失败失败: {}", e);
                }
                break;
            }

            // 检查连续无操作次数（防止无限循环）
            if no_action_count >= 3 {
                let error = format!("连续 {} 次未返回有效操作，停止执行", no_action_count);
                self.fail(error.clone()).await;
                if let Err(e) = self.logger.log_task_failed(&error, step).await {
                    warn!("记录任务失败失败: {}", e);
                }
                break;
            }

            // 检查是否超时
            let elapsed = self.runtime.elapsed_ms().await;
            let max_time_ms = self.runtime.config.max_execution_time * 1000;
            if elapsed > max_time_ms {
                let error = format!("执行超时: {}ms > {}ms", elapsed, max_time_ms);
                self.fail(error.clone()).await;
                if let Err(e) = self.logger.log_task_failed(&error, step).await {
                    warn!("记录任务失败失败: {}", e);
                }
                break;
            }

            // 更新状态为分析中
            *self.runtime.state.write().await = AgentState::Analyzing { step };

            // 截取屏幕
            debug!("步骤 {}: 截取屏幕", step);
            let screenshot_start = std::time::Instant::now();
            let screenshot = match self.device.screenshot().await {
                Ok(s) => s,
                Err(e) => {
                    let error = format!("截图失败: {}", e);
                    self.fail(error.clone()).await;
                    if let Err(e) = self.logger.log_task_failed(&error, step).await {
                        warn!("记录任务失败失败: {}", e);
                    }
                    break;
                }
            };
            let screenshot_duration = screenshot_start.elapsed();

            // 获取当前消息列表
            let current_messages = self.messages.read().await.clone();
            let messages_count = current_messages.len();

            // 克隆消息用于日志记录（在移动之前）
            let messages_for_log = current_messages.clone();

            // 使用消息列表查询 LLM
            debug!("步骤 {}: 查询 LLM (消息数: {})", step, messages_count);
            let query_start = std::time::Instant::now();
            let model_response = match self.model_client.query_with_messages(current_messages, Some(&screenshot)).await {
                Ok(r) => r,
                Err(e) => {
                    let error = format!("LLM 查询失败: {}", e);
                    self.fail(error.clone()).await;
                    if let Err(e) = self.logger.log_task_failed(&error, step).await {
                        warn!("记录任务失败失败: {}", e);
                    }
                    break;
                }
            };
            let query_duration = query_start.elapsed();

            // 检查是否有操作
            let parsed_actions = model_response.actions;

            // 检查是否为空
            if parsed_actions.is_empty() {
                // 没有解析到有效操作，添加反馈消息让 LLM 重新回复
                info!("没有解析到有效操作，添加反馈消息让 LLM 重新回复");
                let feedback_msg = format!(
                    "你的回复中没有包含 do(action=...) 格式的执行动作，我无法解析。\n\n请严格按照 do(action=ActionType, ...) 格式回复，例如：\n- do(action=\"Tap\", element=[x,y])\n- do(action=\"Type\", text=\"xxx\")\n- do(action=\"Swipe\", start=[x1,y1], end=[x2,y2])\n- do(action=\"Launch\", app=\"xxx\")\n- do(action=\"Back\")\n\n请重新分析屏幕并告诉我下一步操作。"
                );
                self.add_assistant_message(model_response.content).await;
                self.add_user_message(feedback_msg).await;
                no_action_count += 1;
                step = self.runtime.increment_step().await;
                continue;
            }

            // 重置无操作计数
            no_action_count = 0;

            // 检查是否有 finish 操作（最高优先级）
            if let Some(finish_action) = parsed_actions.iter().find(|a| a.action_type() == "finish") {
                // 添加助手完成消息
                let reasoning = model_response.reasoning.clone().unwrap_or_default();
                let completion_msg = format!(
                    "任务完成。{}\n思考过程: {}",
                    model_response.content,
                    reasoning
                );
                self.add_assistant_message(completion_msg).await;

                let total_duration = loop_start_time.elapsed().as_millis() as u64;
                let result_content = model_response.content.clone();
                self.complete(step, result_content.clone()).await;

                // 记录任务完成
                if let Err(e) = self.logger.log_task_complete(&result_content, step, total_duration).await {
                    warn!("记录任务完成失败: {}", e);
                }
                break;
            }

            // 执行所有操作（串行）
            info!("开始执行 {} 个操作", parsed_actions.len());
            let action_results = self.action_handler.execute_multiple_actions(&parsed_actions).await;

            // 记录每个操作的步骤
            let reasoning_text = model_response.reasoning.clone().unwrap_or_default();

            for (idx, (action, result)) in parsed_actions.iter().zip(action_results.iter()).enumerate() {
                // 更新状态为执行中
                *self.runtime.state.write().await = AgentState::Executing {
                    step,
                    action: action.action_type(),
                };

                debug!("步骤 {}: 记录操作 {}/{}", step, idx + 1, parsed_actions.len());

                // 记录步骤
                let execution_step = ExecutionStep {
                    step_number: step,
                    action_type: action.action_type(),
                    action_description: action.description(),
                    result: result.clone(),
                    timestamp: chrono::Utc::now(),
                    screenshot: screenshot.clone(),
                    reasoning: reasoning_text.clone(),
                };

                self.runtime.add_step(execution_step).await;

                // 添加到对话上下文（包含执行结果）
                let status = if result.success { "成功" } else { "失败" };
                self.conversation.add_message(
                    crate::agent::context::MessageRole::Assistant,
                    format!("执行操作: {} - {} ({})",
                        action.action_type(),
                        status,
                        result.message
                    ),
                    Some(screenshot.clone()),
                ).await;
            }

            // 将助手响应添加到消息列表
            let actions_summary: Vec<String> = parsed_actions.iter()
                .map(|a| format!("{} ({})", a.description(), a.action_type()))
                .collect();
            let assistant_response = format!(
                "我决定执行 {} 个操作:\n{}\n思考: {}",
                parsed_actions.len(),
                actions_summary.join("\n"),
                reasoning_text
            );
            self.add_assistant_message(assistant_response).await;

            // 将操作结果格式化并添加为用户消息
            let mut result_summary_parts = Vec::new();
            for (idx, (action, result)) in parsed_actions.iter().zip(action_results.iter()).enumerate() {
                let status = if result.success { "成功" } else { "失败" };
                let detail = if result.success {
                    format!("详情: {}", result.message)
                } else {
                    format!("错误: {}", result.message)
                };
                result_summary_parts.push(format!(
                    "- 操作 #{}: {} ({})\n  状态: {}\n  {}\n  耗时: {}ms",
                    idx + 1,
                    action.action_type(),
                    action.description(),
                    status,
                    detail,
                    result.duration_ms
                ));
            }

            let result_summary = format!(
                "操作结果（步骤 {}）:\n{}\n\n请分析当前屏幕并决定下一步操作。",
                step,
                result_summary_parts.join("\n")
            );
            self.add_user_message(result_summary).await;

            // 增加步数
            step = self.runtime.increment_step().await;

            // 记录操作日志（只记录第一个操作的日志，保持兼容性）
            use crate::agent::logger::{ActionResultLog, LogMessage};
            if let (Some(first_action), Some(first_result)) = (parsed_actions.first(), action_results.first()) {
                let total_step_duration = query_duration + screenshot_duration + std::time::Duration::from_millis(first_result.duration_ms as u64);

                // 将消息转换为日志格式
                let log_messages: Vec<LogMessage> = messages_for_log.iter().map(|msg| {
                    LogMessage {
                        role: format!("{:?}", msg.role).to_lowercase(),
                        content: msg.content.clone(),
                    }
                }).collect();

                if let Err(e) = self.logger.log_action(
                    step - 1, // 使用之前的 step（increment 之前）
                    log_messages,
                    Some(screenshot.clone()),
                    model_response.content.clone(),
                    model_response.reasoning.clone(),
                    first_action.action_type(),
                    serde_json::json!({}), // ActionEnum 没有 parameters 字段，使用空对象
                    Some(ActionResultLog {
                        success: first_result.success,
                        message: first_result.message.clone(),
                        duration_ms: first_result.duration_ms,
                    }),
                    model_response.tokens_used,
                    total_step_duration.as_millis() as u64,
                ).await {
                    warn!("记录操作日志失败: {}", e);
                }
            }

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
        let should_reset = matches!(*state, AgentState::Completed { .. } | AgentState::Failed { .. });
        let can_start = should_reset || matches!(*state, AgentState::Idle);
        drop(state);

        if !can_start {
            return Err(AppError::AgentError(
                crate::agent::core::traits::AgentError::AlreadyRunning,
            ));
        }

        // 如果已完成或失败，重置状态
        if should_reset {
            info!("Agent {} 已完成/失败，重置状态以重新启动", self.id);
            self.runtime.reset().await;
        }

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
            messages: Arc::clone(&self.messages),
            logger: Arc::clone(&self.logger),
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
