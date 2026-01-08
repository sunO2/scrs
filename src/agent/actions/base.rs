use serde::{Deserialize, Serialize};
use crate::agent::core::traits::{Action, Device, ActionResult, ActionError};
use crate::error::AppError;

// 导入具体的 Action 类型
use super::touch::TapAction;
use super::touch::LongPressAction;
use super::touch::DoubleTapAction;
use super::swipe::SwipeAction;
use super::swipe::ScrollAction;
use super::input::TypeAction;
use super::input::PressKeyAction;
use super::navigation::BackAction;
use super::navigation::HomeAction;
use super::navigation::RecentAction;
use super::navigation::NotificationAction;
use super::system::LaunchAction;
use super::system::WaitAction;
use super::system::ScreenshotAction;
use super::system::FinishAction;

/// 所有支持的操作类型（枚举形式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionEnum {
    Tap(TapAction),
    LongPress(LongPressAction),
    DoubleTap(DoubleTapAction),
    Swipe(SwipeAction),
    Scroll(ScrollAction),
    Type(TypeAction),
    PressKey(PressKeyAction),
    Back(BackAction),
    Home(HomeAction),
    Recent(RecentAction),
    Notification(NotificationAction),
    Launch(LaunchAction),
    Wait(WaitAction),
    Screenshot(ScreenshotAction),
    Finish(FinishAction),
}

impl Action for ActionEnum {
    async fn execute(&self, device: &dyn Device) -> Result<ActionResult, AppError> {
        match self {
            ActionEnum::Tap(a) => a.execute(device).await,
            ActionEnum::LongPress(a) => a.execute(device).await,
            ActionEnum::DoubleTap(a) => a.execute(device).await,
            ActionEnum::Swipe(a) => a.execute(device).await,
            ActionEnum::Scroll(a) => a.execute(device).await,
            ActionEnum::Type(a) => a.execute(device).await,
            ActionEnum::PressKey(a) => a.execute(device).await,
            ActionEnum::Back(a) => a.execute(device).await,
            ActionEnum::Home(a) => a.execute(device).await,
            ActionEnum::Recent(a) => a.execute(device).await,
            ActionEnum::Notification(a) => a.execute(device).await,
            ActionEnum::Launch(a) => a.execute(device).await,
            ActionEnum::Wait(a) => a.execute(device).await,
            ActionEnum::Screenshot(a) => a.execute(device).await,
            ActionEnum::Finish(a) => a.execute(device).await,
        }
    }

    fn validate(&self) -> Result<(), ActionError> {
        match self {
            ActionEnum::Tap(a) => a.validate(),
            ActionEnum::LongPress(a) => a.validate(),
            ActionEnum::DoubleTap(a) => a.validate(),
            ActionEnum::Swipe(a) => a.validate(),
            ActionEnum::Scroll(a) => a.validate(),
            ActionEnum::Type(a) => a.validate(),
            ActionEnum::PressKey(a) => a.validate(),
            ActionEnum::Back(a) => a.validate(),
            ActionEnum::Home(a) => a.validate(),
            ActionEnum::Recent(a) => a.validate(),
            ActionEnum::Notification(a) => a.validate(),
            ActionEnum::Launch(a) => a.validate(),
            ActionEnum::Wait(a) => a.validate(),
            ActionEnum::Screenshot(a) => a.validate(),
            ActionEnum::Finish(a) => a.validate(),
        }
    }

    fn description(&self) -> String {
        match self {
            ActionEnum::Tap(a) => a.description(),
            ActionEnum::LongPress(a) => a.description(),
            ActionEnum::DoubleTap(a) => a.description(),
            ActionEnum::Swipe(a) => a.description(),
            ActionEnum::Scroll(a) => a.description(),
            ActionEnum::Type(a) => a.description(),
            ActionEnum::PressKey(a) => a.description(),
            ActionEnum::Back(a) => a.description(),
            ActionEnum::Home(a) => a.description(),
            ActionEnum::Recent(a) => a.description(),
            ActionEnum::Notification(a) => a.description(),
            ActionEnum::Launch(a) => a.description(),
            ActionEnum::Wait(a) => a.description(),
            ActionEnum::Screenshot(a) => a.description(),
            ActionEnum::Finish(a) => a.description(),
        }
    }

    fn action_type(&self) -> String {
        match self {
            ActionEnum::Tap(_) => "tap".to_string(),
            ActionEnum::LongPress(_) => "long_press".to_string(),
            ActionEnum::DoubleTap(_) => "double_tap".to_string(),
            ActionEnum::Swipe(_) => "swipe".to_string(),
            ActionEnum::Scroll(_) => "scroll".to_string(),
            ActionEnum::Type(_) => "type".to_string(),
            ActionEnum::PressKey(_) => "press_key".to_string(),
            ActionEnum::Back(_) => "back".to_string(),
            ActionEnum::Home(_) => "home".to_string(),
            ActionEnum::Recent(_) => "recent".to_string(),
            ActionEnum::Notification(_) => "notification".to_string(),
            ActionEnum::Launch(_) => "launch".to_string(),
            ActionEnum::Wait(_) => "wait".to_string(),
            ActionEnum::Screenshot(_) => "screenshot".to_string(),
            ActionEnum::Finish(_) => "finish".to_string(),
        }
    }

    fn estimated_duration(&self) -> u32 {
        match self {
            ActionEnum::Tap(_) => 100,
            ActionEnum::LongPress(a) => a.duration_ms + 100,
            ActionEnum::DoubleTap(_) => 300,
            ActionEnum::Swipe(a) => a.duration_ms + 100,
            ActionEnum::Scroll(a) => a.duration_ms + 100,
            ActionEnum::Type(_) => 200,
            ActionEnum::PressKey(_) => 100,
            ActionEnum::Back(_) => 100,
            ActionEnum::Home(_) => 100,
            ActionEnum::Recent(_) => 100,
            ActionEnum::Notification(_) => 300,
            ActionEnum::Launch(_) => 2000,
            ActionEnum::Wait(a) => a.duration_ms,
            ActionEnum::Screenshot(_) => 500,
            ActionEnum::Finish(_) => 0,
        }
    }
}

impl ActionEnum {
    /// 从 JSON 创建 ActionEnum
    pub fn from_json(action_type: &str, params: serde_json::Value) -> Result<Self, serde_json::Error> {
        Ok(match action_type {
            "tap" => ActionEnum::Tap(serde_json::from_value(params)?),
            "long_press" => ActionEnum::LongPress(serde_json::from_value(params)?),
            "double_tap" => ActionEnum::DoubleTap(serde_json::from_value(params)?),
            "swipe" => ActionEnum::Swipe(serde_json::from_value(params)?),
            "scroll" => ActionEnum::Scroll(serde_json::from_value(params)?),
            "type" => ActionEnum::Type(serde_json::from_value(params)?),
            "press_key" => ActionEnum::PressKey(serde_json::from_value(params)?),
            "back" => ActionEnum::Back(serde_json::from_value(params)?),
            "home" => ActionEnum::Home(serde_json::from_value(params)?),
            "recent" => ActionEnum::Recent(serde_json::from_value(params)?),
            "notification" => ActionEnum::Notification(serde_json::from_value(params)?),
            "launch" => ActionEnum::Launch(serde_json::from_value(params)?),
            "wait" => ActionEnum::Wait(serde_json::from_value(params)?),
            "screenshot" => ActionEnum::Screenshot(serde_json::from_value(params)?),
            "finish" => ActionEnum::Finish(serde_json::from_value(params)?),
            _ => {
                return Err(serde_json::Error::io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("未知的操作类型: {}", action_type),
                )))
           }
        })
    }
}
