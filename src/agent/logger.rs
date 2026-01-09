use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use base64::Engine;

/// Agent 操作日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLogEntry {
    pub timestamp: DateTime<Utc>,
    pub agent_id: String,
    pub task_id: Option<String>,
    pub step_number: usize,

    // 请求信息
    pub request: LogRequest,

    // 响应信息
    pub response: LogResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRequest {
    pub messages: Vec<LogMessage>,
    pub screenshot_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogResponse {
    pub model_response: String,
    pub thinking: Option<String>,
    pub action_type: String,
    pub action_parameters: serde_json::Value,
    pub action_result: Option<ActionResultLog>,
    pub tokens_used: u32,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResultLog {
    pub success: bool,
    pub message: String,
    pub duration_ms: u32,
}

/// Agent 日志记录器
pub struct AgentLogger {
    agent_id: String,
    log_dir: String,
    log_file: Arc<Mutex<std::fs::File>>,
    current_task_id: Arc<Mutex<Option<String>>>,
}

impl AgentLogger {
    /// 创建新的日志记录器
    pub fn new(agent_id: &str, log_dir: &str) -> Result<Self, std::io::Error> {
        // 确保日志目录存在
        std::fs::create_dir_all(log_dir)?;

        // 创建日志文件，文件名包含 agent_id 和日期
        let date = Utc::now().format("%Y-%m-%d").to_string();
        let filename = format!("{}/agent_{}_{}.jsonl", log_dir, agent_id, date);

        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&filename)?;

        Ok(Self {
            agent_id: agent_id.to_string(),
            log_dir: log_dir.to_string(),
            log_file: Arc::new(Mutex::new(log_file)),
            current_task_id: Arc::new(Mutex::new(None)),
        })
    }

    /// 设置当前任务 ID
    pub async fn set_task_id(&self, task_id: String) {
        *self.current_task_id.lock().await = Some(task_id);
    }

    /// 保存截图到文件
    pub fn save_screenshot(&self, step_number: usize, screenshot_base64: &str) -> Result<String, std::io::Error> {
        // 创建 screenshots 子目录
        let screenshots_dir = format!("{}/screenshots", self.log_dir);
        fs::create_dir_all(&screenshots_dir)?;

        // 生成文件名：agent_id_step_timestamp.png
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S_%3f").to_string();
        let filename = format!("{}/{}_step_{}.png", screenshots_dir, self.agent_id, timestamp);

        // 解码 base64
        let base64_data = screenshot_base64.trim_start_matches("data:image/png;base64,");
        let image_bytes = base64::engine::general_purpose::STANDARD.decode(base64_data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Base64 解码失败: {}", e)))?;

        // 写入文件
        fs::write(&filename, image_bytes)?;

        Ok(filename)
    }

    /// 记录操作
    pub async fn log_action(
        &self,
        step_number: usize,
        messages: Vec<LogMessage>,
        screenshot_base64: Option<String>,
        model_response: String,
        thinking: Option<String>,
        action_type: String,
        action_parameters: serde_json::Value,
        action_result: Option<ActionResultLog>,
        tokens_used: u32,
        duration_ms: u64,
    ) -> Result<(), std::io::Error> {
        // 如果有截图，保存到文件并记录路径
        let screenshot_path = if let Some(ref base64) = screenshot_base64 {
            match self.save_screenshot(step_number, base64) {
                Ok(path) => Some(path),
                Err(e) => {
                    eprintln!("保存截图失败: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let entry = AgentLogEntry {
            timestamp: Utc::now(),
            agent_id: self.agent_id.clone(),
            task_id: self.current_task_id.lock().await.clone(),
            step_number,
            request: LogRequest {
                messages,
                screenshot_base64: screenshot_path,
            },
            response: LogResponse {
                model_response,
                thinking,
                action_type,
                action_parameters,
                action_result,
                tokens_used,
                duration_ms,
            },
        };

        // 序列化为 JSON
        let json_line = serde_json::to_string(&entry)?;
        let line_with_newline = format!("{}\n", json_line);

        // 写入文件
        let mut file = self.log_file.lock().await;
        file.write_all(line_with_newline.as_bytes())?;
        file.flush()?;

        Ok(())
    }

    /// 记录任务开始
    pub async fn log_task_start(&self, task: &str) -> Result<(), std::io::Error> {
        let task_id = format!("{}_{:?}", self.agent_id, Utc::now().timestamp());
        self.set_task_id(task_id.clone()).await;

        let entry = serde_json::json!({
            "timestamp": Utc::now().to_rfc3339(),
            "agent_id": self.agent_id,
            "task_id": task_id,
            "event": "task_start",
            "task": task,
        });

        let json_line = format!("{}\n", entry);
        let mut file = self.log_file.lock().await;
        file.write_all(json_line.as_bytes())?;
        file.flush()?;

        Ok(())
    }

    /// 记录任务完成
    pub async fn log_task_complete(&self, result: &str, steps: usize, duration_ms: u64) -> Result<(), std::io::Error> {
        let task_id = self.current_task_id.lock().await.clone();

        let entry = serde_json::json!({
            "timestamp": Utc::now().to_rfc3339(),
            "agent_id": self.agent_id,
            "task_id": task_id,
            "event": "task_complete",
            "result": result,
            "steps": steps,
            "duration_ms": duration_ms,
        });

        let json_line = format!("{}\n", entry);
        let mut file = self.log_file.lock().await;
        file.write_all(json_line.as_bytes())?;
        file.flush()?;

        // 清除任务 ID
        *self.current_task_id.lock().await = None;

        Ok(())
    }

    /// 记录任务失败
    pub async fn log_task_failed(&self, error: &str, step: usize) -> Result<(), std::io::Error> {
        let task_id = self.current_task_id.lock().await.clone();

        let entry = serde_json::json!({
            "timestamp": Utc::now().to_rfc3339(),
            "agent_id": self.agent_id,
            "task_id": task_id,
            "event": "task_failed",
            "error": error,
            "step": step,
        });

        let json_line = format!("{}\n", entry);
        let mut file = self.log_file.lock().await;
        file.write_all(json_line.as_bytes())?;
        file.flush()?;

        // 清除任务 ID
        *self.current_task_id.lock().await = None;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry_serialization() {
        let entry = AgentLogEntry {
            timestamp: Utc::now(),
            agent_id: "test_agent".to_string(),
            task_id: Some("task_123".to_string()),
            step_number: 1,
            request: LogRequest {
                messages: vec![
                    LogMessage {
                        role: "user".to_string(),
                        content: "Open WeChat".to_string(),
                    }
                ],
                screenshot_base64: None,
            },
            response: LogResponse {
                model_response: "I will open WeChat".to_string(),
                thinking: Some("Need to find WeChat icon".to_string()),
                action_type: "launch".to_string(),
                action_parameters: serde_json::json!({"app": "微信"}),
                action_result: Some(ActionResultLog {
                    success: true,
                    message: "Launched successfully".to_string(),
                    duration_ms: 1500,
                }),
                tokens_used: 100,
                duration_ms: 2000,
            },
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("Open WeChat"));
        assert!(json.contains("launch"));
    }
}
