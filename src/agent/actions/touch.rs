use serde::{Deserialize, Serialize};
use crate::agent::core::traits::{Action, Device, ActionResult, ActionError};
use crate::error::AppError;
use std::time::Instant;

/// 点击操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TapAction {
    pub x: u32,
    pub y: u32,
    pub description: Option<String>,
}

impl Action for TapAction {
    fn action_type(&self) -> String {
        "tap".to_string()
    }

    async fn execute(&self, device: &dyn Device) -> Result<ActionResult, AppError> {
        use tracing::{info, debug};

        info!("👆 TapAction: 执行点击");
        info!("   坐标: ({}, {})", self.x, self.y);
        info!("   描述: {:?}", self.description);

        let start = Instant::now();

        debug!("   调用 device.tap...");
        device.tap(self.x, self.y).await?;

        let elapsed = start.elapsed();
        info!("   ✅ 点击完成 (耗时: {}ms)", elapsed.as_millis());

        Ok(ActionResult::success(
            self.description.clone().unwrap_or_else(|| format!("点击 ({}, {})", self.x, self.y)),
            elapsed.as_millis() as u32,
        ))
    }

    fn validate(&self) -> Result<(), ActionError> {
        use tracing::debug;

        debug!("🔍 TapAction: 验证参数");
        debug!("   坐标: ({}, {})", self.x, self.y);

        if self.x > 10000 || self.y > 10000 {
            return Err(ActionError::OutOfBounds { x: self.x, y: self.y });
        }

        debug!("   ✅ 验证通过");
        Ok(())
    }

    fn description(&self) -> String {
        self.description.clone().unwrap_or_else(|| format!("点击 ({}, {})", self.x, self.y))
    }
}

/// 长按操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongPressAction {
    pub x: u32,
    pub y: u32,
    pub duration_ms: u32,
    pub description: Option<String>,
}

impl Action for LongPressAction {
    fn action_type(&self) -> String {
        "long_press".to_string()
    }

    async fn execute(&self, device: &dyn Device) -> Result<ActionResult, AppError> {
        let start = Instant::now();
        device.long_press(self.x, self.y, self.duration_ms).await?;
        Ok(ActionResult::success(
            self.description
                .clone()
                .unwrap_or_else(|| format!("长按 ({}, {}) {}ms", self.x, self.y, self.duration_ms)),
            start.elapsed().as_millis() as u32,
        ))
    }

    fn validate(&self) -> Result<(), ActionError> {
        if self.x > 10000 || self.y > 10000 {
            return Err(ActionError::OutOfBounds { x: self.x, y: self.y });
        }
        if self.duration_ms < 100 {
            return Err(ActionError::DurationTooShort(self.duration_ms));
        }
        if self.duration_ms > 10000 {
            return Err(ActionError::DurationTooLong(self.duration_ms));
        }
        Ok(())
    }

    fn description(&self) -> String {
        self.description
            .clone()
            .unwrap_or_else(|| format!("长按 ({}, {}) {}ms", self.x, self.y, self.duration_ms))
    }
}

/// 双击操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoubleTapAction {
    pub x: u32,
    pub y: u32,
    pub description: Option<String>,
}

impl Action for DoubleTapAction {
    fn action_type(&self) -> String {
        "double_tap".to_string()
    }

    async fn execute(&self, device: &dyn Device) -> Result<ActionResult, AppError> {
        let start = Instant::now();
        device.double_tap(self.x, self.y).await?;
        Ok(ActionResult::success(
            self.description
                .clone()
                .unwrap_or_else(|| format!("双击 ({}, {})", self.x, self.y)),
            start.elapsed().as_millis() as u32,
        ))
    }

    fn validate(&self) -> Result<(), ActionError> {
        if self.x > 10000 || self.y > 10000 {
            return Err(ActionError::OutOfBounds { x: self.x, y: self.y });
        }
        Ok(())
    }

    fn description(&self) -> String {
        self.description
            .clone()
            .unwrap_or_else(|| format!("双击 ({}, {})", self.x, self.y))
    }
}
