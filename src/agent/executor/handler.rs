use std::sync::Arc;
use crate::agent::core::traits::{Device, Action, ActionResult};
use crate::agent::actions::ActionEnum;
use crate::agent::core::traits::ParsedAction;
use crate::error::AppError;
use tracing::{debug, info, warn, error};

/// 操作处理器，负责执行和调度操作
pub struct ActionHandler {
    device: Option<Arc<dyn Device>>,
    max_retries: u32,
    retry_delay_ms: u64,
}

impl ActionHandler {
    /// 创建新的操作处理器
    pub fn new(device: Arc<dyn Device>) -> Self {
        Self {
            device: Some(device),
            max_retries: 3,
            retry_delay_ms: 1000,
        }
    }

    /// 设置最大重试次数
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// 设置重试延迟
    pub fn with_retry_delay(mut self, delay_ms: u64) -> Self {
        self.retry_delay_ms = delay_ms;
        self
    }

    /// 执行操作（带重试）
    pub async fn execute_with_retry(
        &self,
        action: &ActionEnum,
    ) -> Result<ActionResult, AppError> {
        let device = self.device.as_ref()
            .ok_or_else(|| AppError::Unknown("Device 未初始化".to_string()))?;

        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                debug!(
                    "重试操作，第 {} 次，操作: {}",
                    attempt,
                    action.description()
                );
                tokio::time::sleep(tokio::time::Duration::from_millis(
                    self.retry_delay_ms * attempt as u64,
                ))
                .await;
            }

            // 执行前输出 action 详情
            info!("📋 ActionHandler: 准备执行 action");
            info!("   Action 类型: {}", action.action_type());
            info!("   Action 描述: {}", action.description());
            info!("   Action 详情: {:?}", action);
            info!("   重试次数: {}/{}", attempt, self.max_retries);

            match action.execute(device.as_ref()).await {
                Ok(result) => {
                    info!("📊 ActionHandler: 执行结果");
                    info!("   成功: {}", result.success);
                    info!("   消息: {}", result.message);
                    info!("   耗时: {}ms", result.duration_ms);

                    if result.success {
                        info!("✅ 操作执行成功");
                        return Ok(result);
                    } else {
                        warn!("❌ 操作执行失败: {}", result.message);
                        last_error = Some(AppError::Unknown(result.message.clone()));
                    }
                }
                Err(e) => {
                    warn!("❌ 操作执行出错: {}", e);
                    warn!("   错误类型: {:?}", std::any::type_name::<AppError>());
                    warn!("   错误详情: {:?}", e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            AppError::Unknown("操作失败，已达到最大重试次数".to_string())
        }))
    }

    /// 从解析的操作执行
    pub async fn execute_parsed_action(
        &self,
        action: &ActionEnum,
    ) -> Result<ActionResult, AppError> {
        debug!("执行解析的操作: {}", action.action_type());

        // 验证操作
        action.validate().map_err(|e| {
            AppError::Unknown(format!("操作验证失败: {}", e))
        })?;

        // 执行操作
        self.execute_with_retry(action).await
    }

    /// 串行执行多个操作
    /// 返回所有操作的执行结果列表
    /// 即使某个操作失败，也会继续执行后续操作
    pub async fn execute_multiple_actions(
        &self,
        actions: &[ActionEnum],
    ) -> Vec<ActionResult> {
        info!("开始执行 {} 个操作", actions.len());

        let mut results = Vec::with_capacity(actions.len());

        for (idx, action) in actions.iter().enumerate() {
            info!("执行操作 {}/{}", idx + 1, actions.len());
            info!("  操作类型: {}", action.action_type());
            info!("  操作描述: {}", action.description());

            // 验证操作
            let validation_result = action.validate();
            if let Err(e) = validation_result {
                warn!("操作 #{} 验证失败: {}", idx + 1, e);
                results.push(ActionResult::failure(
                    format!("操作验证失败: {}", e),
                    0
                ));
                continue;
            }

            // 执行操作
            match self.execute_with_retry(action).await {
                Ok(result) => {
                    info!("操作 #{} 执行成功: {}", idx + 1, result.message);
                    results.push(result);
                }
                Err(e) => {
                    warn!("操作 #{} 执行失败: {}", idx + 1, e);
                    results.push(ActionResult::failure(
                        format!("{}", e),
                        0
                    ));
                }
            }
        }

        let success_count = results.iter().filter(|r| r.success).count();
        info!("执行完成: {}/{} 成功", success_count, actions.len());

        results
    }

    /// 转换 Action 参数格式
    /// 将提示词中的参数格式转换为 Action 结构体需要的格式
    fn convert_action_params(
        &self,
        action_type: &str,
        mut params: serde_json::Value,
    ) -> Result<serde_json::Value, AppError> {
        let obj = params.as_object_mut()
            .ok_or_else(|| AppError::Unknown("参数不是对象".to_string()))?;

        match action_type {
            // Tap/Long Press: element=[x,y] -> x, y
            "tap" | "long_press" | "double_tap" => {
                if let Some(element) = obj.remove("element") {
                    if let Some(arr) = element.as_array() {
                        if arr.len() >= 2 {
                            if let (Some(x), Some(y)) = (arr[0].as_u64(), arr[1].as_u64()) {
                                obj.insert("x".to_string(), serde_json::json!(x));
                                obj.insert("y".to_string(), serde_json::json!(y));
                            }
                        }
                    }
                }
            }

            // Swipe: start=[x1,y1], end=[x2,y2] -> start_x, start_y, end_x, end_y
            "swipe" => {
                if let Some(start) = obj.remove("start") {
                    if let Some(arr) = start.as_array() {
                        if arr.len() >= 2 {
                            if let (Some(x), Some(y)) = (arr[0].as_u64(), arr[1].as_u64()) {
                                obj.insert("start_x".to_string(), serde_json::json!(x));
                                obj.insert("start_y".to_string(), serde_json::json!(y));
                            }
                        }
                    }
                }
                if let Some(end) = obj.remove("end") {
                    if let Some(arr) = end.as_array() {
                        if arr.len() >= 2 {
                            if let (Some(x), Some(y)) = (arr[0].as_u64(), arr[1].as_u64()) {
                                obj.insert("end_x".to_string(), serde_json::json!(x));
                                obj.insert("end_y".to_string(), serde_json::json!(y));
                            }
                        }
                    }
                }
                // 如果没有 duration_ms，设置默认值
                if !obj.contains_key("duration_ms") {
                    obj.insert("duration_ms".to_string(), serde_json::json!(500));
                }
            }

            // Launch: app="..." -> package="..."
            "launch" => {
                if let Some(app) = obj.remove("app") {
                    if let Some(app_name) = app.as_str() {
                        let package = crate::agent::actions::system::app_name_to_package(app_name)
                            .ok_or_else(|| AppError::Unknown(format!("无法识别的应用名称: {}", app_name)))?;
                        obj.insert("package".to_string(), serde_json::json!(package));
                    }
                }
            }

            // Wait: duration=1 -> duration_ms=1000, message="..." -> reason="..."
            "wait" => {
                if let Some(duration) = obj.remove("duration") {
                    // duration 可能是数字（秒）或字符串
                    let duration_ms = if let Some(seconds) = duration.as_u64() {
                        seconds * 1000
                    } else if let Some(seconds_str) = duration.as_str() {
                        // 尝试解析字符串
                        seconds_str.parse::<u64>()
                            .map(|s| s * 1000)
                            .unwrap_or(1000)
                    } else {
                        1000 // 默认 1 秒
                    };
                    obj.insert("duration_ms".to_string(), serde_json::json!(duration_ms));
                }
                // 如果没有 duration_ms，设置默认值
                if !obj.contains_key("duration_ms") {
                    obj.insert("duration_ms".to_string(), serde_json::json!(1000));
                }
                // message -> reason
                if let Some(message) = obj.remove("message") {
                    obj.insert("reason".to_string(), message);
                }
            }

            _ => {}
        }

        Ok(serde_json::Value::Object(obj.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::actions::TapAction;
    use crate::agent::core::traits::ParsedAction;

    // 注意：这些测试需要 mock device 实现

    #[tokio::test]
    async fn test_action_handler_creation() {
        // 这是一个占位测试，实际需要 mock device
        // let handler = ActionHandler::new(device);
        // assert_eq!(handler.max_retries, 3);
    }

    #[test]
    fn test_convert_tap_params() {
        let handler = ActionHandler::new_uninitialized();
        let params = serde_json::json!({
            "element": [500, 800]
        });

        let result = handler.convert_action_params("tap", params).unwrap();
        assert_eq!(result.get("x").unwrap().as_u64().unwrap(), 500);
        assert_eq!(result.get("y").unwrap().as_u64().unwrap(), 800);
        assert!(result.get("element").is_none());
    }

    #[test]
    fn test_convert_swipe_params() {
        let handler = ActionHandler::new_uninitialized();
        let params = serde_json::json!({
            "start": [100, 200],
            "end": [300, 400]
        });

        let result = handler.convert_action_params("swipe", params).unwrap();
        assert_eq!(result.get("start_x").unwrap().as_u64().unwrap(), 100);
        assert_eq!(result.get("start_y").unwrap().as_u64().unwrap(), 200);
        assert_eq!(result.get("end_x").unwrap().as_u64().unwrap(), 300);
        assert_eq!(result.get("end_y").unwrap().as_u64().unwrap(), 400);
        assert_eq!(result.get("duration_ms").unwrap().as_u64().unwrap(), 500); // 默认值
    }

    #[test]
    fn test_convert_launch_params() {
        let handler = ActionHandler::new_uninitialized();
        let params = serde_json::json!({
            "app": "微信"
        });

        let result = handler.convert_action_params("launch", params).unwrap();
        assert_eq!(result.get("package").unwrap().as_str().unwrap(), "com.tencent.mm");
        assert!(result.get("app").is_none());
    }

    #[test]
    fn test_convert_type_params() {
        let handler = ActionHandler::new_uninitialized();
        let params = serde_json::json!({
            "text": "Hello World"
        });

        let result = handler.convert_action_params("type", params).unwrap();
        assert_eq!(result.get("text").unwrap().as_str().unwrap(), "Hello World");
    }

    #[test]
    fn test_convert_wait_params() {
        let handler = ActionHandler::new_uninitialized();
        let params = serde_json::json!({
            "duration": 1,
            "message": "应用正在加载中，请稍等。"
        });

        let result = handler.convert_action_params("wait", params).unwrap();
        assert_eq!(result.get("duration_ms").unwrap().as_u64().unwrap(), 1000);
        assert_eq!(result.get("reason").unwrap().as_str().unwrap(), "应用正在加载中，请稍等。");
        assert!(result.get("duration").is_none());
        assert!(result.get("message").is_none());
    }

    #[test]
    fn test_convert_wait_params_with_string_duration() {
        let handler = ActionHandler::new_uninitialized();
        let params = serde_json::json!({
            "duration": "2"
        });

        let result = handler.convert_action_params("wait", params).unwrap();
        assert_eq!(result.get("duration_ms").unwrap().as_u64().unwrap(), 2000);
    }
}

impl ActionHandler {
    /// 用于测试的构造函数（不需要 device）
    fn new_uninitialized() -> Self {
        // 这是一个用于测试的辅助方法
        Self {
            device: None,
            max_retries: 3,
            retry_delay_ms: 1000,
        }
    }
}
