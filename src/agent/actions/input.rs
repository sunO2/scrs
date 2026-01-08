use serde::{Deserialize, Serialize};
use crate::agent::core::traits::{Action, Device, ActionResult, ActionError};
use crate::error::AppError;
use std::time::Instant;

/// 输入文本操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeAction {
    pub text: String,
    pub description: Option<String>,
}

impl Action for TypeAction {
    async fn execute(&self, device: &dyn Device) -> Result<ActionResult, AppError> {
        let start = Instant::now();
        device.input_text(&self.text).await?;
        Ok(ActionResult::success(
            self.description
                .clone()
                .unwrap_or_else(|| format!("输入文本: {}", self.text)),
            start.elapsed().as_millis() as u32,
        ))
    }

    fn validate(&self) -> Result<(), ActionError> {
        if self.text.is_empty() {
            return Err(ActionError::InvalidParameters("文本不能为空".to_string()));
        }
        if self.text.len() > 10000 {
            return Err(ActionError::InvalidParameters("文本过长".to_string()));
        }
        Ok(())
    }

    fn description(&self) -> String {
        self.description
            .clone()
            .unwrap_or_else(|| format!("输入文本: {}", self.text))
    }
}

/// 按键码
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KeyCode {
    Enter,
    Escape,
    Delete,
    Backspace,
    Tab,
    Home,
    Back,
    VolumeUp,
    VolumeDown,
    Power,
    Camera,
}

impl KeyCode {
    /// 转换为 Android keycode
    pub fn to_android_keycode(&self) -> u32 {
        match self {
            KeyCode::Enter => 66,
            KeyCode::Escape => 111,
            KeyCode::Delete => 67,
            KeyCode::Backspace => 67,
            KeyCode::Tab => 61,
            KeyCode::Home => 3,
            KeyCode::Back => 4,
            KeyCode::VolumeUp => 24,
            KeyCode::VolumeDown => 25,
            KeyCode::Power => 26,
            KeyCode::Camera => 27,
        }
    }
}

/// 按键操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PressKeyAction {
    pub keycode: KeyCode,
    pub description: Option<String>,
}

impl Action for PressKeyAction {
    async fn execute(&self, device: &dyn Device) -> Result<ActionResult, AppError> {
        let start = Instant::now();
        device.press_key(self.keycode.to_android_keycode()).await?;
        Ok(ActionResult::success(
            self.description
                .clone()
                .unwrap_or_else(|| format!("按键: {:?}", self.keycode)),
            start.elapsed().as_millis() as u32,
        ))
    }

    fn validate(&self) -> Result<(), ActionError> {
        Ok(())
    }

    fn description(&self) -> String {
        self.description
            .clone()
            .unwrap_or_else(|| format!("按键: {:?}", self.keycode))
    }
}
