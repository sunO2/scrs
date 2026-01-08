//! 设备池实现
//!
//! 统一管理设备连接、Agent 创建和生命周期

use super::types::{
    DeviceStatus, DevicePoolConfig, DevicePoolEvent,
};
use super::device_entry::DeviceEntry;
use crate::agent::core::agent::PhoneAgent;
use crate::agent::core::traits::Agent;
use crate::agent::core::state::AgentConfig;
use crate::agent::executor::ScrcpyDeviceWrapper;
use crate::agent::llm::{create_model_client, ModelConfig};
use crate::error::AppError;
use adb_client::server::ADBServer;
use adb_client::server_device::ADBServerDevice;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info};
use uuid::Uuid;

/// 设备池
pub struct DevicePool {
    /// 设备映射表
    devices: Arc<RwLock<HashMap<String, DeviceEntry>>>,

    /// 配置
    config: DevicePoolConfig,

    /// 事件发送器
    event_tx: broadcast::Sender<DevicePoolEvent>,

    /// ADB 服务器引用
    adb_server: Arc<RwLock<ADBServer>>,

    /// LLM 客户端配置
    model_config: ModelConfig,

    /// Agent 配置
    agent_config: AgentConfig,
}

impl DevicePool {
    /// 创建新的设备池
    pub fn new(
        config: DevicePoolConfig,
        adb_server: Arc<RwLock<ADBServer>>,
        model_config: ModelConfig,
        agent_config: AgentConfig,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(100);

        Self {
            devices: Arc::new(RwLock::new(HashMap::new())),
            config,
            event_tx,
            adb_server,
            model_config,
            agent_config,
        }
    }

    /// 订阅事件
    pub fn subscribe_events(&self) -> broadcast::Receiver<DevicePoolEvent> {
        self.event_tx.subscribe()
    }

    /// 注册设备
    pub async fn register_device(
        &self,
        serial: String,
        name: Option<String>,
    ) -> Result<(), AppError> {
        // 检查连接数限制
        {
            let devices = self.devices.read().await;
            if devices.len() >= self.config.max_connections {
                return Err(AppError::AgentError(
                    crate::agent::core::traits::AgentError::ValidationError(format!(
                        "已达最大连接数限制: {}",
                        self.config.max_connections
                    )),
                ));
            }
        }

        let mut devices = self.devices.write().await;

        if devices.contains_key(&serial) {
            return Err(AppError::AgentError(
                crate::agent::core::traits::AgentError::ValidationError(format!(
                    "设备已注册: {}",
                    serial
                )),
            ));
        }

        let entry = DeviceEntry::new(serial.clone(), name);
        devices.insert(serial.clone(), entry);

        let _ = self.event_tx.send(DevicePoolEvent::DeviceRegistered {
            serial: serial.clone(),
        });

        info!("设备已注册: {}", serial);
        Ok(())
    }

    /// 注销设备
    pub async fn unregister_device(&self, serial: &str) -> Result<(), AppError> {
        let mut devices = self.devices.write().await;

        if let Some(mut entry) = devices.remove(serial) {
            // 清理资源
            if let Some(agent) = entry.agent.take() {
                let _ = agent.stop().await;
            }
            // ScrcpyConnect 会在 Drop 时自动清理

            let _ = self
                .event_tx
                .send(DevicePoolEvent::DeviceDisconnected {
                    serial: serial.to_string(),
                });

            info!("设备已注销: {}", serial);
            Ok(())
        } else {
            Err(AppError::AgentError(
                crate::agent::core::traits::AgentError::DeviceNotFound(
                    serial.to_string(),
                ),
            ))
        }
    }

    /// 连接设备（懒加载 ScrcpyConnect）
    pub async fn connect_device(&self, serial: &str) -> Result<(), AppError> {
        let mut devices = self.devices.write().await;

        let entry = devices
            .get_mut(serial)
            .ok_or_else(|| AppError::AgentError(
                crate::agent::core::traits::AgentError::DeviceNotFound(
                    serial.to_string(),
                ),
            ))?;

        // 如果已经连接，直接返回
        if entry.scrcpy.is_some() {
            debug!("设备已连接: {}", serial);
            return Ok(());
        }

        // 更新状态
        entry.set_status(DeviceStatus::Connecting);

        // 创建 ScrcpyConnect（默认端口 27183）
        let scrcpy_connect = crate::scrcpy::scrcpy::ScrcpyConnect::new(27183);

        entry.scrcpy = Some(Arc::new(scrcpy_connect));
        entry.set_status(DeviceStatus::Connected);

        let _ = self
            .event_tx
            .send(DevicePoolEvent::DeviceConnected {
                serial: serial.to_string(),
            });

        info!("设备已连接: {}", serial);
        Ok(())
    }

    /// 断开设备
    pub async fn disconnect_device(&self, serial: &str) -> Result<(), AppError> {
        let mut devices = self.devices.write().await;

        let entry = devices
            .get_mut(serial)
            .ok_or_else(|| AppError::AgentError(
                crate::agent::core::traits::AgentError::DeviceNotFound(
                    serial.to_string(),
                ),
            ))?;

        // 停止 Agent
        if let Some(agent) = entry.agent.take() {
            info!("停止 Agent: {} (设备: {})", agent.id(), serial);
            let _ = agent.stop().await;
        }

        // 清理连接
        entry.scrcpy = None;
        entry.set_status(DeviceStatus::Disconnected);

        let _ = self
            .event_tx
            .send(DevicePoolEvent::DeviceDisconnected {
                serial: serial.to_string(),
            });

        info!("设备已断开: {}", serial);
        Ok(())
    }

    /// 获取设备的 Agent（按需创建）
    pub async fn get_agent(&self, serial: &str) -> Result<Arc<PhoneAgent>, AppError> {
        // 确保设备已连接
        self.connect_device(serial).await?;

        let mut devices = self.devices.write().await;
        let entry = devices
            .get_mut(serial)
            .ok_or_else(|| AppError::AgentError(
                crate::agent::core::traits::AgentError::DeviceNotFound(
                    serial.to_string(),
                ),
            ))?;

        // 如果 Agent 已存在，直接返回
        if let Some(agent) = &entry.agent {
            debug!("复用现有 Agent: {} (设备: {})", agent.id(), serial);
            let agent_arc = Arc::clone(agent);
            entry.touch();
            return Ok(agent_arc);
        }

        // 提取需要的数据以避免借用问题
        let scrcpy_opt = entry.scrcpy.clone();
        let name_opt = entry.name.clone();

        // 创建新的 Agent
        let scrcpy = scrcpy_opt
            .as_ref()
            .ok_or_else(|| AppError::AgentError(
                crate::agent::core::traits::AgentError::ConnectionError(
                    "设备未连接".to_string(),
                ),
            ))?;

        // 创建 ADB device (需要在释放 devices 锁之前)
        drop(devices); // 先释放写锁
        let mut adb_server = self.adb_server.write().await;
        let adb_device = adb_server.get_device_by_name(&serial)
            .map_err(|_| AppError::AgentError(
                crate::agent::core::traits::AgentError::DeviceNotFound(serial.to_string())
            ))?;

        let device = Arc::new(ScrcpyDeviceWrapper::new(
            serial.to_string(),
            name_opt.unwrap_or_else(|| serial.to_string()),
            Arc::clone(scrcpy),
            Arc::new(adb_device),
        ));
        let model_client = create_model_client(&self.model_config)?;

        let agent_id = Uuid::new_v4().to_string();
        let agent = PhoneAgent::new(
            agent_id.clone(),
            device,
            model_client,
            self.agent_config.clone(),
        )?;

        let agent_arc = Arc::new(agent);

        // 重新获取写锁来设置 agent
        let mut devices = self.devices.write().await;
        let entry = devices.get_mut(serial).unwrap();
        entry.agent = Some(Arc::clone(&agent_arc));
        entry.set_status(DeviceStatus::Busy);

        let _ = self.event_tx.send(DevicePoolEvent::AgentCreated {
            serial: serial.to_string(),
            agent_id: agent_id.clone(),
        });

        info!("Agent 已创建: {} (设备: {})", agent_id, serial);
        Ok(agent_arc)
    }

    /// 释放设备的 Agent
    pub async fn release_agent(&self, serial: &str) -> Result<(), AppError> {
        let mut devices = self.devices.write().await;

        let entry = devices
            .get_mut(serial)
            .ok_or_else(|| AppError::AgentError(
                crate::agent::core::traits::AgentError::DeviceNotFound(
                    serial.to_string(),
                ),
            ))?;

        if let Some(agent) = entry.agent.take() {
            let agent_id = agent.id().to_string();
            info!("停止 Agent: {} (设备: {})", agent_id, serial);
            let _ = agent.stop().await;

            entry.set_status(DeviceStatus::Connected);
            entry.complete_task();

            let _ = self.event_tx.send(DevicePoolEvent::AgentDestroyed {
                serial: serial.to_string(),
                agent_id,
            });

            info!("Agent 已释放: {}", serial);
        }

        Ok(())
    }

    /// 获取所有设备状态
    pub async fn get_all_devices_status(&self) -> Vec<(String, DeviceStatus)> {
        let devices = self.devices.read().await;
        devices
            .iter()
            .map(|(serial, entry)| (serial.clone(), entry.status.clone()))
            .collect()
    }

    /// 获取设备列表
    pub async fn list_devices(&self) -> Vec<String> {
        let devices = self.devices.read().await;
        devices.keys().cloned().collect()
    }

    /// 获取设备详细信息
    pub async fn get_device_info(&self, serial: &str) -> Option<crate::agent::pool::types::DeviceInfo> {
        let devices = self.devices.read().await;
        devices.get(serial).map(|entry| entry.to_info())
    }

    /// 获取所有设备详细信息
    pub async fn get_all_devices_info(&self) -> Vec<crate::agent::pool::types::DeviceInfo> {
        let devices = self.devices.read().await;
        devices.values().map(|entry| entry.to_info()).collect()
    }

    /// 清理空闲设备
    pub async fn cleanup_idle_devices(&self) -> Result<usize, AppError> {
        let mut devices = self.devices.write().await;
        let threshold = self.config.idle_cleanup_threshold as i64;

        let mut cleaned = 0;

        for (serial, entry) in devices.iter_mut() {
            if entry.is_idle(threshold) {
                // 清理 Agent
                if let Some(agent) = entry.agent.take() {
                    info!("清理空闲 Agent: {} (设备: {})", agent.id(), serial);
                    let _ = agent.stop().await;
                    cleaned += 1;
                }

                // 可选：断开连接（如果空闲时间超过阈值的两倍）
                if entry.idle_seconds() > threshold * 2 {
                    info!("断开空闲连接: {}", serial);
                    entry.scrcpy = None;
                    entry.set_status(DeviceStatus::Disconnected);
                }
            }
        }

        if cleaned > 0 {
            info!("清理了 {} 个空闲设备", cleaned);
        }

        Ok(cleaned)
    }

    /// 健康检查
    pub async fn health_check(&self) -> Result<HashMap<String, bool>, AppError> {
        let devices = self.devices.read().await;
        let mut results = HashMap::new();

        for (serial, entry) in devices.iter() {
            // 检查设备是否健康（基于连接和状态）
            let is_healthy = entry.scrcpy.is_some()
                && matches!(entry.status, DeviceStatus::Connected | DeviceStatus::Busy);

            results.insert(serial.clone(), is_healthy);
        }

        Ok(results)
    }

    /// 更新设备任务状态
    pub async fn update_task_status(
        &self,
        serial: &str,
        task_id: String,
        task: String,
    ) -> Result<(), AppError> {
        let mut devices = self.devices.write().await;

        let entry = devices
            .get_mut(serial)
            .ok_or_else(|| AppError::AgentError(
                crate::agent::core::traits::AgentError::DeviceNotFound(
                    serial.to_string(),
                ),
            ))?;

        // 克隆 task 用于事件发送
        let task_clone = task.clone();
        entry.start_task(task_id, task);

        let _ = self
            .event_tx
            .send(DevicePoolEvent::TaskStarted {
                serial: serial.to_string(),
                task: task_clone,
            });

        Ok(())
    }

    /// 标记任务完成
    pub async fn mark_task_completed(
        &self,
        serial: &str,
        result: String,
    ) -> Result<(), AppError> {
        let mut devices = self.devices.write().await;

        let entry = devices
            .get_mut(serial)
            .ok_or_else(|| AppError::AgentError(
                crate::agent::core::traits::AgentError::DeviceNotFound(
                    serial.to_string(),
                ),
            ))?;

        entry.complete_task();

        let _ = self
            .event_tx
            .send(DevicePoolEvent::TaskCompleted {
                serial: serial.to_string(),
                result,
            });

        Ok(())
    }

    /// 标记任务失败
    pub async fn mark_task_failed(
        &self,
        serial: &str,
        error: String,
    ) -> Result<(), AppError> {
        let mut devices = self.devices.write().await;

        let entry = devices
            .get_mut(serial)
            .ok_or_else(|| AppError::AgentError(
                crate::agent::core::traits::AgentError::DeviceNotFound(
                    serial.to_string(),
                ),
            ))?;

        entry.complete_task();

        let _ = self.event_tx.send(DevicePoolEvent::TaskFailed {
            serial: serial.to_string(),
            error,
        });

        Ok(())
    }
}
