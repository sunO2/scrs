use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// 对话上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub screenshot: Option<String>,
}

/// 消息角色
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// 线程安全的对话上下文管理
pub struct ConversationContext {
    messages: Arc<RwLock<Vec<ConversationMessage>>>,
    max_messages: usize,
}

impl ConversationContext {
    /// 创建新的对话上下文
    pub fn new(max_messages: usize) -> Self {
        Self {
            messages: Arc::new(RwLock::new(Vec::new())),
            max_messages,
        }
    }

    /// 添加消息
    pub async fn add_message(
        &self,
        role: MessageRole,
        content: String,
        screenshot: Option<String>,
    ) {
        let msg = ConversationMessage {
            role,
            content,
            timestamp: Utc::now(),
            screenshot,
        };

        let mut messages = self.messages.write().await;
        messages.push(msg);

        // 限制消息数量
        if messages.len() > self.max_messages {
            messages.remove(0);
        }
    }

    /// 获取所有消息
    pub async fn get_messages(&self) -> Vec<ConversationMessage> {
        self.messages.read().await.clone()
    }

    /// 获取最近的 N 条消息
    pub async fn get_recent_messages(&self, n: usize) -> Vec<ConversationMessage> {
        let messages = self.messages.read().await;
        let len = messages.len();
        if len <= n {
            messages.clone()
        } else {
            messages[len - n..].to_vec()
        }
    }

    /// 清空消息
    pub async fn clear(&self) {
        self.messages.write().await.clear();
    }

    /// 获取消息数量
    pub async fn len(&self) -> usize {
        self.messages.read().await.len()
    }

    /// 构建提示词（包含历史消息）
    pub async fn build_prompt(&self, task: &str) -> String {
        let messages = self.get_recent_messages(10).await;

        let mut prompt = format!("任务: {}\n\n", task);

        if !messages.is_empty() {
            prompt.push_str("历史对话:\n");
            for msg in messages {
                let role_str = match msg.role {
                    MessageRole::User => "用户",
                    MessageRole::Assistant => "助手",
                    MessageRole::System => "系统",
                };
                prompt.push_str(&format!("{}: {}\n", role_str, msg.content));
            }
            prompt.push('\n');
        }

        prompt
    }
}

impl Default for ConversationContext {
    fn default() -> Self {
        Self::new(50)
    }
}
