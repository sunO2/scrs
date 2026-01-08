use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// 短期记忆条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub key: String,
    pub value: String,
    pub timestamp: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl MemoryEntry {
    /// 检查是否过期
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }
}

/// 短期记忆管理
pub struct ShortTermMemory {
    entries: Arc<RwLock<HashMap<String, MemoryEntry>>>,
    default_ttl_seconds: u64,
}

impl ShortTermMemory {
    /// 创建新的短期记忆
    pub fn new(default_ttl_seconds: u64) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            default_ttl_seconds,
        }
    }

    /// 设置记忆
    pub async fn set(&self, key: String, value: String) {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(self.default_ttl_seconds as i64);

        let entry = MemoryEntry {
            key: key.clone(),
            value,
            timestamp: now,
            expires_at: Some(expires_at),
        };

        self.entries.write().await.insert(key, entry);
    }

    /// 设置记忆（带自定义 TTL）
    pub async fn set_with_ttl(&self, key: String, value: String, ttl_seconds: u64) {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(ttl_seconds as i64);

        let entry = MemoryEntry {
            key: key.clone(),
            value,
            timestamp: now,
            expires_at: Some(expires_at),
        };

        self.entries.write().await.insert(key, entry);
    }

    /// 获取记忆
    pub async fn get(&self, key: &str) -> Option<String> {
        let entries = self.entries.read().await;

        if let Some(entry) = entries.get(key) {
            if !entry.is_expired() {
                return Some(entry.value.clone());
            }
        }

        None
    }

    /// 删除记忆
    pub async fn remove(&self, key: &str) {
        self.entries.write().await.remove(key);
    }

    /// 清空所有记忆
    pub async fn clear(&self) {
        self.entries.write().await.clear();
    }

    /// 清理过期记忆
    pub async fn cleanup_expired(&self) {
        let mut entries = self.entries.write().await;
        entries.retain(|_, entry| !entry.is_expired());
    }

    /// 获取所有记忆
    pub async fn get_all(&self) -> HashMap<String, String> {
        let entries = self.entries.read().await;
        let mut result = HashMap::new();

        for (key, entry) in entries.iter() {
            if !entry.is_expired() {
                result.insert(key.clone(), entry.value.clone());
            }
        }

        result
    }
}

impl Default for ShortTermMemory {
    fn default() -> Self {
        Self::new(3600) // 默认 1 小时
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_set_get() {
        let memory = ShortTermMemory::new(60);

        memory.set("test_key".to_string(), "test_value".to_string()).await;

        let value = memory.get("test_key").await;
        assert_eq!(value, Some("test_value".to_string()));
    }

    #[tokio::test]
    async fn test_memory_expiration() {
        let memory = ShortTermMemory::new(1); // 1 秒过期

        memory.set("expiring_key".to_string(), "value".to_string()).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let value = memory.get("expiring_key").await;
        assert_eq!(value, None);
    }
}
