use serde::{Deserialize, Serialize};
use crate::agent::core::traits::{Action, Device, ActionResult, ActionError};
use crate::error::AppError;
use std::time::Instant;

/// 滑动操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwipeAction {
    pub start_x: u32,
    pub start_y: u32,
    pub end_x: u32,
    pub end_y: u32,
    pub duration_ms: u32,
    pub description: Option<String>,
}

impl Action for SwipeAction {
    fn action_type(&self) -> String {
        "swipe".to_string()
    }

    async fn execute(&self, device: &dyn Device) -> Result<ActionResult, AppError> {
        use tracing::{info, debug};

        info!("👆 SwipeAction: 执行滑动");
        info!("   起点: ({}, {})", self.start_x, self.start_y);
        info!("   终点: ({}, {})", self.end_x, self.end_y);
        info!("   持续时间: {}ms", self.duration_ms);
        info!("   描述: {:?}", self.description);

        let start = Instant::now();

        debug!("   调用 device.swipe...");
        device
            .swipe(self.start_x, self.start_y, self.end_x, self.end_y, self.duration_ms)
            .await?;

        let elapsed = start.elapsed();
        info!("   ✅ 滑动完成 (耗时: {}ms)", elapsed.as_millis());

        Ok(ActionResult::success(
            self.description.clone().unwrap_or_else(|| {
                format!(
                    "滑动 from ({},{}) to ({},{}) {}ms",
                    self.start_x, self.start_y, self.end_x, self.end_y, self.duration_ms
                )
            }),
            elapsed.as_millis() as u32,
        ))
    }

    fn validate(&self) -> Result<(), ActionError> {
        use tracing::debug;

        debug!("🔍 SwipeAction: 验证参数");
        debug!("   起点: ({}, {})", self.start_x, self.start_y);
        debug!("   终点: ({}, {})", self.end_x, self.end_y);
        debug!("   持续时间: {}ms", self.duration_ms);

        if self.start_x > 10000 || self.start_y > 10000 || self.end_x > 10000 || self.end_y > 10000 {
            return Err(ActionError::OutOfBounds {
                x: self.start_x.max(self.end_x),
                y: self.start_y.max(self.end_y),
            });
        }
        if self.duration_ms < 50 {
            return Err(ActionError::DurationTooShort(self.duration_ms));
        }
        if self.duration_ms > 5000 {
            return Err(ActionError::DurationTooLong(self.duration_ms));
        }

        debug!("   ✅ 验证通过");
        Ok(())
    }

    fn description(&self) -> String {
        self.description.clone().unwrap_or_else(|| {
            format!(
                "滑动 from ({},{}) to ({},{}) {}ms",
                self.start_x, self.start_y, self.end_x, self.end_y, self.duration_ms
            )
        })
    }
}

/// 滚动方向
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

/// 滚动操作（特殊的滑动）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrollAction {
    pub direction: ScrollDirection,
    pub distance_pct: u32, // 屏幕高度的百分比
    pub duration_ms: u32,
    pub description: Option<String>,
}

impl Action for ScrollAction {
    fn action_type(&self) -> String {
        "scroll".to_string()
    }

    async fn execute(&self, device: &dyn Device) -> Result<ActionResult, AppError> {
        let start = Instant::now();

        // 获取屏幕尺寸
        let (width, height) = device.screen_size().await?;

        // 计算滚动距离
        let distance_y = (height * self.distance_pct / 100) as u32;
        let distance_x = (width * self.distance_pct / 100) as u32;

        // 计算起点和终点
        let (start_x, start_y, end_x, end_y) = match self.direction {
            ScrollDirection::Up => (width / 2, height * 3 / 4, width / 2, height / 4),
            ScrollDirection::Down => (width / 2, height / 4, width / 2, height * 3 / 4),
            ScrollDirection::Left => (width * 3 / 4, height / 2, width / 4, height / 2),
            ScrollDirection::Right => (width / 4, height / 2, width * 3 / 4, height / 2),
        };

        device
            .swipe(start_x, start_y, end_x, end_y, self.duration_ms)
            .await?;

        Ok(ActionResult::success(
            self.description.clone().unwrap_or_else(|| {
                format!(
                    "滚动 {:?} {}% {}ms",
                    self.direction, self.distance_pct, self.duration_ms
                )
            }),
            start.elapsed().as_millis() as u32,
        ))
    }

    fn validate(&self) -> Result<(), ActionError> {
        if self.distance_pct > 100 {
            return Err(ActionError::InvalidParameters(
                "距离百分比不能超过100".to_string(),
            ));
        }
        if self.distance_pct < 1 {
            return Err(ActionError::InvalidParameters(
                "距离百分比不能小于1".to_string(),
            ));
        }
        if self.duration_ms < 50 {
            return Err(ActionError::DurationTooShort(self.duration_ms));
        }
        if self.duration_ms > 2000 {
            return Err(ActionError::DurationTooLong(self.duration_ms));
        }
        Ok(())
    }

    fn description(&self) -> String {
        self.description.clone().unwrap_or_else(|| {
            format!(
                "滚动 {:?} {}% {}ms",
                self.direction, self.distance_pct, self.duration_ms
            )
        })
    }
}
