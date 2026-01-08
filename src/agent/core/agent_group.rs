use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::{RwLock, broadcast};
use tracing::{debug, info, warn};
use crate::agent::core::traits::{Agent, Device, ModelClient};
use crate::agent::core::agent::PhoneAgent;
use crate::agent::core::state::AgentConfig;
use crate::agent::llm::create_model_client;
use crate::agent::llm::types::ModelConfig;
use crate::error::AppError;
use uuid::Uuid;

/// Agent 组事件
#[derive(Debug, Clone)]
pub enum AgentGroupEvent {
    /// Agent 创建
    AgentCreated { agent_id: String, device_serial: String },

    /// Agent 启动
    AgentStarted { agent_id: String, task: String },

    /// Agent 完成
    AgentCompleted { agent_id: String, result: String },

    /// Agent 失败
    AgentFailed { agent_id: String, error: String },

    /// Agent 停止
    AgentStopped { agent_id: String },

    /// 自定义事件
    Custom { agent_id: String, event_type: String, data: String },
}

/// Agent 组配置
#[derive(Debug, Clone)]
pub struct AgentGroupConfig {
    /// 最大并发 Agent 数量
    pub max_concurrent_agents: usize,

    /// 任务队列大小
    pub task_queue_size: usize,

    /// 是否启用事件广播
    pub enable_event_broadcast: bool,
}

impl Default for AgentGroupConfig {
    fn default() -> Self {
        Self {
            max_concurrent_agents: 5,
            task_queue_size: 100,
            enable_event_broadcast: true,
        }
    }
}

/// Agent 组，管理多个 Agent
pub struct AgentGroup {
    id: String,
    agents: RwLock<HashMap<String, Arc<PhoneAgent>>>,
    devices: RwLock<HashMap<String, Arc<dyn Device>>>,
    event_tx: broadcast::Sender<AgentGroupEvent>,
    config: AgentGroupConfig,
    model_config: ModelConfig,
}

impl AgentGroup {
    /// 创建新的 Agent 组
    pub fn new(config: AgentGroupConfig, model_config: ModelConfig) -> Self {
        let (event_tx, _event_rx) = broadcast::channel(100);

        Self {
            id: Uuid::new_v4().to_string(),
            agents: RwLock::new(HashMap::new()),
            devices: RwLock::new(HashMap::new()),
            event_tx,
            config,
            model_config,
        }
    }

    /// 获取 Agent 组 ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 注册设备
    pub async fn register_device(&self, device: Arc<dyn Device>) {
        let serial = device.serial().to_string();
        self.devices.write().await.insert(serial.clone(), device);
        info!("注册设备: {}", serial);
    }

    /// 取消注册设备
    pub async fn unregister_device(&self, serial: &str) {
        self.devices.write().await.remove(serial);
        info!("取消注册设备: {}", serial);
    }

    /// 获取所有已注册设备
    pub async fn get_devices(&self) -> Vec<String> {
        self.devices.read().await.keys().cloned().collect()
    }

    /// 创建 Agent
    pub async fn create_agent(
        &self,
        device_serial: &str,
        config: AgentConfig,
    ) -> Result<String, AppError> {
        // 检查设备是否已注册
        let device = {
            let devices = self.devices.read().await;
            devices.get(device_serial)
                .ok_or_else(|| AppError::DeviceNotFound(device_serial.to_string()))?
                .clone()
        };

        // 创建 LLM 客户端
        let model_client = create_model_client(&self.model_config)?;

        // 创建 Agent
        let agent_id = Uuid::new_v4().to_string();
        let agent = Arc::new(PhoneAgent::new(
            agent_id.clone(),
            device,
            model_client,
            config,
        )?);

        // 添加到管理列表
        self.agents.write().await.insert(agent_id.clone(), agent.clone());

        // 发送事件
        let _ = self.event_tx.send(AgentGroupEvent::AgentCreated {
            agent_id: agent_id.clone(),
            device_serial: device_serial.to_string(),
        });

        info!("创建 Agent: {} for device: {}", agent_id, device_serial);

        Ok(agent_id)
    }

    /// 获取 Agent
    pub async fn get_agent(&self, agent_id: &str) -> Option<Arc<PhoneAgent>> {
        self.agents.read().await.get(agent_id).cloned()
    }

    /// 启动 Agent
    pub async fn start_agent(&self, agent_id: &str, task: String) -> Result<(), AppError> {
        let agent = self.get_agent(agent_id).await
            .ok_or_else(|| AppError::AgentError(crate::agent::core::traits::AgentError::NotFound(agent_id.to_string())))?;

        let task_for_event = task.clone();
        agent.start(task).await?;

        // 发送事件
        let _ = self.event_tx.send(AgentGroupEvent::AgentStarted {
            agent_id: agent_id.to_string(),
            task: task_for_event,
        });

        Ok(())
    }

    /// 停止 Agent
    pub async fn stop_agent(&self, agent_id: &str) -> Result<(), AppError> {
        let agent = self.get_agent(agent_id).await
            .ok_or_else(|| AppError::AgentError(crate::agent::core::traits::AgentError::NotFound(agent_id.to_string())))?;

        agent.stop().await?;

        // 发送事件
        let _ = self.event_tx.send(AgentGroupEvent::AgentStopped {
            agent_id: agent_id.to_string(),
        });

        Ok(())
    }

    /// 移除 Agent
    pub async fn remove_agent(&self, agent_id: &str) -> Result<(), AppError> {
        // 先停止 Agent
        if let Some(agent) = self.get_agent(agent_id).await {
            let _ = agent.stop().await;
        }

        // 从管理列表中移除
        self.agents.write().await.remove(agent_id);

        info!("移除 Agent: {}", agent_id);

        Ok(())
    }

    /// 获取所有 Agent ID
    pub async fn list_agents(&self) -> Vec<String> {
        self.agents.read().await.keys().cloned().collect()
    }

    /// 获取运行中的 Agent 数量
    pub async fn active_agent_count(&self) -> usize {
        let agents = self.agents.read().await;
        let mut count = 0;

        for agent in agents.values() {
            if let crate::agent::core::traits::AgentStatus::Running { .. } = agent.status().await {
                count += 1;
            }
        }

        count
    }

    /// 广播任务到所有设备
    pub async fn broadcast_task(&self, task: String) -> Result<Vec<String>, AppError> {
        let device_serials = self.get_devices().await;
        let mut agent_ids = Vec::new();

        for serial in device_serials {
            match self.create_agent(&serial, AgentConfig::default()).await {
                Ok(agent_id) => {
                    if let Err(e) = self.start_agent(&agent_id, task.clone()).await {
                        warn!("启动 Agent 失败: {}", e);
                        let _ = self.remove_agent(&agent_id).await;
                    } else {
                        agent_ids.push(agent_id);
                    }
                }
                Err(e) => {
                    warn!("创建 Agent 失败: {}", e);
                }
            }
        }

        Ok(agent_ids)
    }

    /// 订阅事件
    pub fn subscribe_events(&self) -> broadcast::Receiver<AgentGroupEvent> {
        self.event_tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_agent_group_creation() {
        let config = AgentGroupConfig::default();
        let model_config = ModelConfig::default();
        let group = AgentGroup::new(config, model_config);

        assert!(!group.id().is_empty());
        assert_eq!(group.active_agent_count().await, 0);
    }
}
