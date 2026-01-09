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
use super::input::KeyCode;
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

impl ActionEnum {
    /// 解析 LLM 响应中的操作
    /// 支持两种格式：
    /// 1. `finish(...)` - 任务完成，括号内是消息（最高优先级，单个）
    /// 2. `do(...)` - 执行操作，括号内是 `action="...", key=value` 格式（支持多个）
    ///
    /// 返回格式：
    /// - 如果有 finish(...)，返回 (Some(thinking), vec![finish_action])
    /// - 如果有多个 do(...)，返回 (Some(thinking), vec![action1, action2, ...])
    /// - 如果都没有，返回 (Some(thinking), vec![])
    pub fn parse_from_response(content: &str) -> (Option<String>, Vec<Self>) {
        use regex::Regex;
        use tracing::{debug, info, warn};

        // 提取 <thinking> 标签内容
        let thinking_re = Regex::new(r"<thinking>([^<]*)</thinking>").unwrap();
        let thinking = thinking_re.captures(content)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().trim().to_string());

        if let Some(ref t) = thinking {
            debug!("💭 thinking 部分: {}", t);
        } else {
            debug!("💭 未找到 <thinking> 标签");
        }

        // 规则 1: 检查 finish(...)
        // 手动查找匹配的括号，支持多行内容
        debug!("🔍 检查 finish(...) 模式");
        if let Some(start_pos) = content.find("finish(") {
            let mut bracket_count = 0;
            let mut in_brackets = false;
            let mut end_pos = start_pos + 6; // 跳过 "finish"

            for (i, c) in content[start_pos + 6..].char_indices() {
                let actual_i = start_pos + 6 + i;
                if c == '(' {
                    bracket_count += 1;
                    in_brackets = true;
                } else if c == ')' {
                    bracket_count -= 1;
                    if bracket_count == 0 && in_brackets {
                        end_pos = actual_i;
                        break;
                    }
                }
            }

            if end_pos > start_pos + 6 {
                let message = content[start_pos + 7..end_pos].trim();
                debug!("✅ 匹配到 finish(...) 模式");
                debug!("💬 message 部分: {}", message);

                // 移除可能的 message= 前缀和引号
                let message = message
                    .strip_prefix("message=")
                    .unwrap_or(message)
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();

                info!("✅ 解析成功: finish action with message='{}'", message);
                return (thinking, vec![ActionEnum::Finish(FinishAction {
                    result: message,
                    success: true,
                })]);
            }
        }

        // 规则 2: 检查多个 do(...)
        // 查找所有 do(...) 模式
        debug!("🔍 检查 do(...) 模式（支持多个）");
        let mut actions = Vec::new();
        let mut search_start = 0;

        while let Some(start_pos) = content[search_start..].find("do(") {
            let actual_start = search_start + start_pos;

            // 手动查找匹配的括号
            let mut bracket_count = 0;
            let mut in_brackets = false;
            let mut end_pos = actual_start + 2; // 跳过 "do"

            for (i, c) in content[actual_start + 2..].char_indices() {
                let actual_i = actual_start + 2 + i;
                if c == '(' {
                    bracket_count += 1;
                    in_brackets = true;
                } else if c == ')' {
                    bracket_count -= 1;
                    if bracket_count == 0 && in_brackets {
                        end_pos = actual_i;
                        break;
                    }
                }
            }

            if end_pos > actual_start + 2 {
                let params_str = content[actual_start + 3..end_pos].trim();
                debug!("✅ 匹配到 do(...) 模式 #{}", actions.len() + 1);
                debug!("🔧 参数字符串: {}", params_str);

                // 解析参数
                match Self::parse_do_params(params_str) {
                    Some(action) => {
                        info!("✅ 解析成功 #{}: {} action", actions.len() + 1, action.action_type());
                        actions.push(action);
                    }
                    None => {
                        warn!("⚠️  do(...) #{} 参数解析失败: {}", actions.len() + 1, params_str);
                    }
                }

                // 移动到下一个位置继续搜索
                search_start = end_pos + 1;
            } else {
                // 没有找到匹配的括号，停止搜索
                break;
            }
        }

        if !actions.is_empty() {
            info!("✅ 总共解析到 {} 个 do(...) 操作", actions.len());
            return (thinking, actions);
        }

        warn!("❌ 无法解析响应内容，没有匹配到 finish() 或 do() 模式");
        // 如果没有找到匹配，返回空 Vec
        (thinking, vec![])
    }

    /// 解析 do() 括号内的参数
    /// 支持格式：
    /// - action="Tap", element=[x,y]
    /// - action="Type", text="hello"
    /// - action="Back"
    fn parse_do_params(params_str: &str) -> Option<Self> {
        use regex::Regex;
        use tracing::{debug, info};

        debug!("🔧 开始解析 do() 参数: {}", params_str);

        // 提取 action 类型
        let action_re = Regex::new(r#"action\s*=\s*"([^"]+)""#).unwrap();
        let action_type = if let Some(cap) = action_re.captures(params_str) {
            let action = cap.get(1).unwrap().as_str();
            debug!("✅ 提取 action 类型: {}", action);
            action
        } else {
            debug!("❌ 未找到 action 类型");
            // 如果没有 action=，直接返回 None
            return None;
        };

        // 构建参数 JSON
        let mut params = serde_json::Map::new();

        // 匹配 key="value" 格式
        let kv_re = Regex::new(r#"(\w+)\s*=\s*"([^"]*)""#).unwrap();
        for cap in kv_re.captures_iter(params_str) {
            let key = cap.get(1).unwrap().as_str();
            let value = cap.get(2).unwrap().as_str();
            // 跳过 action 字段
            if key != "action" {
                debug!("  📌 参数: {} = {}", key, value);
                params.insert(key.to_string(), serde_json::json!(value));
            }
        }

        // 匹配 key=[...] 格式（数组）
        let array_re = Regex::new(r#"(\w+)\s*=\s*\[([^\]]+)\]"#).unwrap();
        for cap in array_re.captures_iter(params_str) {
            let key = cap.get(1).unwrap().as_str();
            let values_str = cap.get(2).unwrap().as_str();
            let values: Vec<u32> = values_str
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if !values.is_empty() && key != "action" {
                debug!("  📌 参数: {} = {:?}", key, values);
                params.insert(key.to_string(), serde_json::json!(values));
            }
        }

        // 匹配 key=value 格式（无引号，用于数字）
        let num_re = Regex::new(r#"(\w+)\s*=\s*(\d+)"#).unwrap();
        for cap in num_re.captures_iter(params_str) {
            let key = cap.get(1).unwrap().as_str();
            let value = cap.get(2).unwrap().as_str();
            if key != "action" && !params.contains_key(key) {
                debug!("  📌 参数: {} = {} (数字)", key, value);
                params.insert(key.to_string(), serde_json::json!(value));
            }
        }

        debug!("📊 解析后的参数: {:?}", params);

        // 使用 ParsedAction 转换
        let parsed = crate::agent::core::traits::ParsedAction {
            action_type: action_type.to_string(),
            parameters: serde_json::Value::Object(params),
            reasoning: params_str.to_string(),
        };

        info!("🔄 转换 ParsedAction: action_type={}", parsed.action_type);
        let result = Self::from_parsed(parsed);

        if result.is_some() {
            info!("✅ 成功创建 ActionEnum");
        } else {
            info!("❌ 无法创建 ActionEnum (from_parsed 返回 None)");
        }

        result
    }

    /// 从 ParsedAction 创建 ActionEnum
    fn from_parsed(parsed: crate::agent::core::traits::ParsedAction) -> Option<Self> {
        use tracing::debug;

        debug!("🎯 from_parsed: 处理 action_type='{}'", parsed.action_type);
        debug!("   参数: {:?}", parsed.parameters);

        match parsed.action_type.to_lowercase().as_str() {
            "tap" => {
                // 尝试从 element 或 x,y 获取坐标
                if let Some(element) = parsed.parameters.get("element") {
                    if let Some(coords) = element.as_array() {
                        if coords.len() >= 2 {
                            let x = coords[0].as_u64()? as u32;
                            let y = coords[1].as_u64()? as u32;
                            return Some(ActionEnum::Tap(TapAction { x, y, description: None }));
                        }
                    }
                }
                // 尝试从 x, y 字段获取
                if let (Some(x), Some(y)) = (
                    parsed.parameters.get("x").and_then(|v| v.as_u64()).map(|v| v as u32),
                    parsed.parameters.get("y").and_then(|v| v.as_u64()).map(|v| v as u32),
                ) {
                    return Some(ActionEnum::Tap(TapAction { x, y, description: None }));
                }
                None
            }
            "long_press" => {
                if let Some(element) = parsed.parameters.get("element") {
                    if let Some(coords) = element.as_array() {
                        if coords.len() >= 2 {
                            let x = coords[0].as_u64()? as u32;
                            let y = coords[1].as_u64()? as u32;
                            let duration_ms = parsed.parameters.get("duration_ms")
                                .and_then(|v| v.as_u64()).map(|v| v as u32)
                                .unwrap_or(1000);
                            return Some(ActionEnum::LongPress(LongPressAction { x, y, duration_ms, description: None }));
                        }
                    }
                }
                None
            }
            "double_tap" => {
                if let Some(element) = parsed.parameters.get("element") {
                    if let Some(coords) = element.as_array() {
                        if coords.len() >= 2 {
                            let x = coords[0].as_u64()? as u32;
                            let y = coords[1].as_u64()? as u32;
                            return Some(ActionEnum::DoubleTap(DoubleTapAction { x, y, description: None }));
                        }
                    }
                }
                None
            }
            "swipe" => {
                if let (Some(start), Some(end)) = (
                    parsed.parameters.get("start").and_then(|v| v.as_array()),
                    parsed.parameters.get("end").and_then(|v| v.as_array()),
                ) {
                    if start.len() >= 2 && end.len() >= 2 {
                        let start_x = start[0].as_u64()? as u32;
                        let start_y = start[1].as_u64()? as u32;
                        let end_x = end[0].as_u64()? as u32;
                        let end_y = end[1].as_u64()? as u32;
                        let duration_ms = parsed.parameters.get("duration_ms")
                            .and_then(|v| v.as_u64()).map(|v| v as u32)
                            .unwrap_or(500);
                        return Some(ActionEnum::Swipe(SwipeAction { start_x, start_y, end_x, end_y, duration_ms, description: None }));
                    }
                }
                None
            }
            "type" => {
                if let Some(text) = parsed.parameters.get("text").and_then(|v| v.as_str()) {
                    return Some(ActionEnum::Type(TypeAction { text: text.to_string(), description: None }));
                }
                None
            }
            "press_key" => {
                if let Some(keycode) = parsed.parameters.get("keycode").and_then(|v| v.as_u64()) {
                    let key_code = match keycode as u32 {
                        3 => KeyCode::Home,
                        4 => KeyCode::Back,
                        66 => KeyCode::Enter,
                        111 => KeyCode::Escape,
                        67 => KeyCode::Delete,
                        61 => KeyCode::Tab,
                        24 => KeyCode::VolumeUp,
                        25 => KeyCode::VolumeDown,
                        26 => KeyCode::Power,
                        27 => KeyCode::Camera,
                        _ => KeyCode::Back,
                    };
                    return Some(ActionEnum::PressKey(PressKeyAction { keycode: key_code, description: None }));
                }
                None
            }
            "back" => Some(ActionEnum::Back(BackAction { description: None })),
            "home" => Some(ActionEnum::Home(HomeAction { description: None })),
            "recent" => Some(ActionEnum::Recent(RecentAction { description: None })),
            "notification" => Some(ActionEnum::Notification(NotificationAction { description: None })),
            "launch" => {
                if let Some(app) = parsed.parameters.get("app").and_then(|v| v.as_str())
                    .or_else(|| parsed.parameters.get("app_name").and_then(|v| v.as_str())) {
                    return Some(ActionEnum::Launch(LaunchAction {
                        package: app.to_string(),
                        activity: None,
                        description: None,
                    }));
                }
                None
            }
            "wait" => {
                let duration_ms = parsed.parameters.get("duration_ms")
                    .and_then(|v| v.as_u64()).map(|v| v as u32)
                    .or_else(|| parsed.parameters.get("duration").and_then(|v| v.as_u64()).map(|v| v as u32 * 1000))
                    .unwrap_or(1000);
                let message = parsed.parameters.get("message").and_then(|v| v.as_str()).map(|s| s.to_string());
                return Some(ActionEnum::Wait(WaitAction { duration_ms, reason: message }));
            }
            "screenshot" => Some(ActionEnum::Screenshot(ScreenshotAction { description: None })),
            "finish" => {
                let result = parsed.parameters.get("result")
                    .and_then(|v| v.as_str())
                    .or_else(|| parsed.parameters.get("message").and_then(|v| v.as_str()))
                    .unwrap_or("任务完成");
                let success = parsed.parameters.get("success").and_then(|v| v.as_bool()).unwrap_or(true);
                return Some(ActionEnum::Finish(FinishAction {
                    result: result.to_string(),
                    success,
                }));
            }
            _ => None,
        }
    }
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
