use serde::{Deserialize, Serialize};
use crate::agent::core::traits::{Action, Device, ActionResult, ActionError};
use crate::error::AppError;
use std::time::Instant;

/// 返回键操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackAction {
    pub description: Option<String>,
}

impl Action for BackAction {
    async fn execute(&self, device: &dyn Device) -> Result<ActionResult, AppError> {
        let start = Instant::now();
        device.back().await?;
        Ok(ActionResult::success(
            self.description.clone().unwrap_or_else(|| "按下返回键".to_string()),
            start.elapsed().as_millis() as u32,
        ))
    }

    fn validate(&self) -> Result<(), ActionError> {
        Ok(())
    }

    fn description(&self) -> String {
        self.description.clone().unwrap_or_else(|| "按下返回键".to_string())
    }
}

/// Home 键操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeAction {
    pub description: Option<String>,
}

impl Action for HomeAction {
    async fn execute(&self, device: &dyn Device) -> Result<ActionResult, AppError> {
        let start = Instant::now();
        device.home().await?;
        Ok(ActionResult::success(
            self.description.clone().unwrap_or_else(|| "按下 Home 键".to_string()),
            start.elapsed().as_millis() as u32,
        ))
    }

    fn validate(&self) -> Result<(), ActionError> {
        Ok(())
    }

    fn description(&self) -> String {
        self.description.clone().unwrap_or_else(|| "按下 Home 键".to_string())
    }
}

/// 最近任务操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentAction {
    pub description: Option<String>,
}

impl Action for RecentAction {
    async fn execute(&self, device: &dyn Device) -> Result<ActionResult, AppError> {
        let start = Instant::now();
        device.recent().await?;
        Ok(ActionResult::success(
            self.description
                .clone()
                .unwrap_or_else(|| "打开最近任务".to_string()),
            start.elapsed().as_millis() as u32,
        ))
    }

    fn validate(&self) -> Result<(), ActionError> {
        Ok(())
    }

    fn description(&self) -> String {
        self.description
            .clone()
            .unwrap_or_else(|| "打开最近任务".to_string())
    }
}

/// 通知栏操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationAction {
    pub description: Option<String>,
}

impl Action for NotificationAction {
    async fn execute(&self, device: &dyn Device) -> Result<ActionResult, AppError> {
        let start = Instant::now();
        device.notification().await?;
        Ok(ActionResult::success(
            self.description
                .clone()
                .unwrap_or_else(|| "打开通知栏".to_string()),
            start.elapsed().as_millis() as u32,
        ))
    }

    fn validate(&self) -> Result<(), ActionError> {
        Ok(())
    }

    fn description(&self) -> String {
        self.description
            .clone()
            .unwrap_or_else(|| "打开通知栏".to_string())
    }
}
