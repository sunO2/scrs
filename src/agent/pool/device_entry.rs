//! 设备条目实现
//!
//! 表示池中的单个设备及其状态

use crate::agent::core::agent::PhoneAgent;
use crate::agent::pool::types::DeviceStatus;
use crate::scrcpy::scrcpy::ScrcpyConnect;
use chrono::{DateTime, Utc};
use std::sync::Arc;

/// 设备条目
pub struct DeviceEntry {
    /// 设备序列号
    pub serial: String,

    /// 设备名称（可选）
    pub name: Option<String>,

    /// Scrcpy 连接（懒加载）
    pub scrcpy: Option<Arc<ScrcpyConnect>>,

    /// Agent 实例（按需创建）
    pub agent: Option<Arc<PhoneAgent>>,

    /// 当前状态
    pub status: DeviceStatus,

    /// 最后使用时间
    pub last_used: DateTime<Utc>,

    /// 创建时间
    pub created_at: DateTime<Utc>,

    /// 当前任务 ID（如果有）
    pub current_task_id: Option<String>,

    /// 当前任务描述（如果有）
    pub current_task: Option<String>,
}

impl DeviceEntry {
    /// 创建新的设备条目
    pub fn new(serial: String, name: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            serial,
            name,
            scrcpy: None,
            agent: None,
            status: DeviceStatus::Registered,
            last_used: now,
            created_at: now,
            current_task_id: None,
            current_task: None,
        }
    }

    /// 更新最后使用时间
    pub fn touch(&mut self) {
        self.last_used = Utc::now();
    }

    /// 获取空闲时长（秒）
    pub fn idle_seconds(&self) -> i64 {
        Utc::now()
            .signed_duration_since(self.last_used)
            .num_seconds()
    }

    /// 是否空闲
    pub fn is_idle(&self, threshold_seconds: i64) -> bool {
        self.idle_seconds() > threshold_seconds
            && self.agent.is_none()
            && self.current_task_id.is_none()
    }

    /// 是否有活跃连接
    pub fn is_connected(&self) -> bool {
        self.scrcpy.is_some()
            && (self.status == DeviceStatus::Connected
                || self.status == DeviceStatus::Busy)
    }

    /// 是否忙碌（有 Agent 在运行）
    pub fn is_busy(&self) -> bool {
        self.agent.is_some() || self.status == DeviceStatus::Busy
    }

    /// 获取设备信息
    pub fn to_info(&self) -> crate::agent::pool::types::DeviceInfo {
        crate::agent::pool::types::DeviceInfo {
            serial: self.serial.clone(),
            name: self.name.clone(),
            status: self.status.clone(),
            has_agent: self.agent.is_some(),
            last_used: self.last_used.timestamp(),
            idle_seconds: self.idle_seconds(),
        }
    }

    /// 设置状态
    pub fn set_status(&mut self, status: DeviceStatus) {
        self.status = status;
        self.touch();
    }

    /// 开始任务
    pub fn start_task(&mut self, task_id: String, task: String) {
        self.current_task_id = Some(task_id);
        self.current_task = Some(task);
        self.status = DeviceStatus::Busy;
        self.touch();
    }

    /// 完成任务
    pub fn complete_task(&mut self) {
        self.current_task_id = None;
        self.current_task = None;
        self.status = DeviceStatus::Connected;
        self.touch();
    }
}
