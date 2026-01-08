use crate::agent::core::traits::ModelClient;
use crate::agent::llm::client::OpenAIClient;
use crate::agent::llm::autoglm_client::AutoGLMClient;
use crate::agent::llm::types::ModelConfig;
use crate::agent::core::traits::ModelError;
use std::sync::Arc;

/// 创建模型客户端（工厂函数）
pub fn create_model_client(config: &ModelConfig) -> Result<Arc<dyn ModelClient>, ModelError> {
    match config.provider.as_str() {
        "openai" | "azure" => {
            let client = OpenAIClient::new(config.clone())?;
            Ok(Arc::new(client))
        }
        "local" | "autoglm" => {
            // 对于 AutoGLM，使用专门的客户端
            let client = AutoGLMClient::new(config.clone())?;
            Ok(Arc::new(client))
        }
        _ => Err(ModelError::ApiError(format!(
            "不支持的模型提供商: {}",
            config.provider
        ))),
    }
}

/// 创建 AutoGLM 客户端的便捷函数
pub fn create_autoglm_client(
    base_url: String,
    model_name: String,
) -> Result<Arc<dyn ModelClient>, ModelError> {
    let config = ModelConfig::local(base_url, model_name);
    let client = AutoGLMClient::new(config)?;
    Ok(Arc::new(client))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_openai_client() {
        let config = ModelConfig {
            provider: "openai".to_string(),
            ..Default::default()
        };

        let client = create_model_client(&config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_create_autoglm_client() {
        let config = ModelConfig::local(
            "http://localhost:8000/v1".to_string(),
            "autoglm-phone-9b".to_string(),
        );

        let client = create_model_client(&config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_create_autoglm_client_helper() {
        let client = create_autoglm_client(
            "http://localhost:8000/v1".to_string(),
            "autoglm-phone-9b".to_string(),
        );
        assert!(client.is_ok());
    }
}
