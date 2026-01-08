use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use crate::agent::llm::types::ModelConfig;
use crate::agent::core::state::AgentConfig as CoreAgentConfig;

/// 完整的 Agent 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullAgentConfig {
    pub agent: CoreAgentConfig,
    pub model: ModelConfig,
}

impl Default for FullAgentConfig {
    fn default() -> Self {
        Self {
            agent: CoreAgentConfig::default(),
            model: ModelConfig::default(),
        }
    }
}

impl FullAgentConfig {
    /// 从 TOML 文件加载配置
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path)
            .map_err(|e| ConfigError::IoError(e.to_string()))?;

        let config: FullAgentConfig = toml::from_str(&content)
            .map_err(|e| ConfigError::ParseError(e.to_string()))?;

        Ok(config)
    }

    /// 从文件加载，并使用环境变量覆盖
    pub fn from_file_with_env<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let mut config = Self::from_file(path)?;

        // 使用环境变量覆盖 API Key
        if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
            config.model.api_key = api_key;
        }

        if let Ok(base_url) = std::env::var("OPENAI_BASE_URL") {
            config.model.base_url = base_url;
        }

        Ok(config)
    }

    /// 保存到文件
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::SerializeError(e.to_string()))?;

        fs::write(path, content)
            .map_err(|e| ConfigError::IoError(e.to_string()))?;

        Ok(())
    }

    /// 获取默认配置
    pub fn default_config() -> Self {
        Self::default()
    }

    /// 创建本地模型配置
    pub fn with_local_model(base_url: String, model_name: String) -> Self {
        Self {
            agent: CoreAgentConfig::default(),
            model: ModelConfig::local(base_url, model_name),
        }
    }
}

/// 配置错误
#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("IO 错误: {0}")]
    IoError(String),

    #[error("解析错误: {0}")]
    ParseError(String),

    #[error("序列化错误: {0}")]
    SerializeError(String),

    #[error("验证错误: {0}")]
    ValidationError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = FullAgentConfig::default();
        assert_eq!(config.agent.max_steps, 50);
        assert_eq!(config.model.provider, "openai");
    }

    #[test]
    fn test_serialize_config() {
        let config = FullAgentConfig::default();
        let toml_str = toml::to_string_pretty(&config);
        assert!(toml_str.is_ok());
    }
}
