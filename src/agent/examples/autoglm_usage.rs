//! AutoGLM 客户端使用示例
//!
//! 展示如何使用 AutoGLM 客户端进行手机自动化

use crate::agent::llm::{create_autoglm_client, ModelConfig};
use crate::agent::core::traits::ModelClient;

/// 创建并使用 AutoGLM 客户端的示例
pub async fn example_autoglm_query() {
    // 创建 AutoGLM 客户端
    let client = create_autoglm_client(
        "http://localhost:8000/v1".to_string(),
        "autoglm-phone-9b".to_string(),
    ).expect("Failed to create AutoGLM client");

    // 准备查询
    let prompt = "请分析当前屏幕，告诉我应该点击哪里";
    let screenshot = Some("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="); // 示例 base64

    // 执行查询
    match client.query(prompt, screenshot).await {
        Ok(response) => {
            println!("✅ 查询成功!");
            println!("内容: {}", response.content);

            if let Some(reasoning) = response.reasoning {
                println!("思考过程: {}", reasoning);
            }

            if let Some(action) = response.action {
                println!("操作类型: {}", action.action_type);
                println!("操作参数: {}", action.parameters);
                println!("推理: {}", action.reasoning);
            }

            println!("置信度: {:.2}", response.confidence);
            println!("使用 tokens: {}", response.tokens_used);
        }
        Err(e) => {
            eprintln!("❌ 查询失败: {}", e);
        }
    }
}

/// 使用配置文件创建 AutoGLM 客户端
pub async fn example_with_config() {
    // 创建配置
    let config = ModelConfig {
        provider: "autoglm".to_string(),
        base_url: "http://localhost:8000/v1".to_string(),
        model_name: "autoglm-phone-9b".to_string(),
        api_key: "EMPTY".to_string(),
        max_tokens: 3000,
        temperature: 0.0,
        top_p: 0.85,
        timeout: 60,
    };

    // 使用工厂函数创建客户端
    let client = crate::agent::llm::create_model_client(&config)
        .expect("Failed to create client");

    // 查询模型信息
    let info = client.info();
    println!("模型信息:");
    println!("  名称: {}", info.name);
    println!("  提供商: {}", info.provider);
    println!("  支持视觉: {}", info.supports_vision);
    println!("  最大 tokens: {}", info.max_tokens);
    println!("  上下文窗口: {}", info.context_window);
}

/// AutoGLM 响应格式示例
///
/// AutoGLM 支持以下几种响应格式：
///
/// 1. finish 标记格式:
/// ```text
/// 分析屏幕中...
/// finish(message="任务完成")
/// ```
///
/// 2. do(action=) 标记格式:
/// ```text
/// 分析屏幕中...
/// do(action=tap, x=100, y=200)
/// ```
///
/// 3. XML 标签格式:
/// ```text
/// <thinking>应该点击按钮</thinking>
/// <answer>{"action_type": "tap", "x": 100, "y": 200}</answer>
/// ```
///
/// 4. 纯 JSON 格式:
/// ```json
/// {
///   "action_type": "tap",
///   "x": 100,
///   "y": 200
/// }
/// ```
pub fn autoglm_response_formats() {
    println!("AutoGLM 支持的响应格式:");
    println!("1. finish(message=\"...\")");
    println!("2. do(action=..., param=value)");
    println!("3. <thinking>...</thinking><answer>...</answer>");
    println!("4. 纯 JSON 格式");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 需要实际的 AutoGLM 服务运行
    async fn test_autoglm_example() {
        example_autoglm_query().await;
    }

    #[test]
    fn test_response_formats() {
        autoglm_response_formats();
    }
}
