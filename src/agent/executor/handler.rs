use std::sync::Arc;
use crate::agent::core::traits::{Device, Action, ActionResult};
use crate::agent::actions::ActionEnum;
use crate::agent::core::traits::ParsedAction;
use crate::error::AppError;
use tracing::{debug, info, warn, error};

/// 操作处理器，负责执行和调度操作
pub struct ActionHandler {
    device: Arc<dyn Device>,
    max_retries: u32,
    retry_delay_ms: u64,
}

impl ActionHandler {
    /// 创建新的操作处理器
    pub fn new(device: Arc<dyn Device>) -> Self {
        Self {
            device,
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

            match action.execute(self.device.as_ref()).await {
                Ok(result) => {
                    if result.success {
                        debug!("操作成功: {}", action.description());
                        return Ok(result);
                    } else {
                        warn!("操作失败: {}", result.message);
                        last_error = Some(AppError::Unknown(result.message.clone()));
                    }
                }
                Err(e) => {
                    warn!("操作执行出错: {}", e);
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
        parsed: &ParsedAction,
    ) -> Result<ActionResult, AppError> {
        debug!("执行解析的操作: {} {:?}", parsed.action_type, parsed.parameters);

        // 特殊处理 launch 操作，将 app_name 转换为 package
        let params = if parsed.action_type == "launch" {
            self.convert_launch_params(parsed.parameters.clone())
                .map_err(|e| AppError::Unknown(format!("转换 launch 参数失败: {}", e)))?
        } else {
            parsed.parameters.clone()
        };

        // 从 JSON 创建 ActionEnum
        let action = ActionEnum::from_json(&parsed.action_type, params)
            .map_err(|e| AppError::Unknown(format!("创建操作失败: {}", e)))?;

        // 验证操作
        action.validate().map_err(|e| {
            AppError::Unknown(format!("操作验证失败: {}", e))
        })?;

        // 执行操作
        self.execute_with_retry(&action).await
    }

    /// 转换 launch 操作的参数，将 app_name 转换为 package
    fn convert_launch_params(
        &self,
        mut params: serde_json::Value,
    ) -> Result<serde_json::Value, AppError> {
        // 如果有 app_name 字段，转换为 package
        if let Some(app_name) = params.get("app_name").and_then(|v| v.as_str()) {
            // 使用 system.rs 中的函数转换
            let package = crate::agent::actions::system::app_name_to_package(app_name)
                .ok_or_else(|| AppError::Unknown(format!("无法识别的应用名称: {}", app_name)))?;

            // 添加或替换 package 字段
            if let Some(obj) = params.as_object_mut() {
                obj.insert("package".to_string(), serde_json::json!(package));
            }
        }

        Ok(params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::actions::TapAction;

    // 注意：这些测试需要 mock device 实现

    #[tokio::test]
    async fn test_action_handler_creation() {
        // 这是一个占位测试，实际需要 mock device
        // let handler = ActionHandler::new(device);
        // assert_eq!(handler.max_retries, 3);
    }
}
