mod api;
mod context;
mod error;
mod scrcpy;
mod logger;
mod agent;

use std::sync::Arc;
use tracing::{info, error};
use tracing_subscriber::{EnvFilter, fmt};

use context::context::{Context, IContext};
use agent::{
    DevicePool, DevicePoolConfig,
    AgentConfig, ModelConfig, AgentSocketServer,
};

#[tokio::main]
async fn main() {
    // 初始化日志系统
    let filter = EnvFilter::from_default_env()
        .add_directive("scrcpy_rs=debug".parse().unwrap())
        .add_directive("axum=info".parse().unwrap());

    fmt()
        .with_env_filter(filter)
        .init();

    info!("启动 Scrcpy API 服务器...");

    // 创建 Context 实例，包含 ScrcpyServer 和 ADBServer
    let ctx = Arc::new(Context::new());

    // 初始化 DevicePool
    let device_pool_config = DevicePoolConfig::default();
    let adb_server = Arc::clone(ctx.get_adb_server());

    let model_config = ModelConfig {
        provider: "autoglm".to_string(),
        model_name: "autoglm-phone".to_string(),
        api_key: std::env::var("AUTOGLM_API_KEY")
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .unwrap_or_else(|_| {
                error!("未设置 API Key！请设置环境变量 AUTOGLM_API_KEY 或 OPENAI_API_KEY");
                error!("从 https://open.bigmodel.cn/ 获取 API Key");
                "sk-test".to_string()
            }),
        base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
        max_tokens: 4096,
        temperature: 0.0,
        top_p: 0.85,
        timeout: 60, // 增加超时时间到 60 秒
    };

    // 检查 API Key 是否有效
    if model_config.api_key == "sk-test" {
        error!("⚠️  使用了测试 API Key，Agent 将无法正常工作！");
        error!("⚠️  请设置环境变量 AUTOGLM_API_KEY");
        error!("⚠️  例如: export AUTOGLM_API_KEY=your_actual_api_key");
    } else {
        info!("✓ API Key 已配置: {}...", &model_config.api_key[..model_config.api_key.len().min(10)]);
    }

    let agent_config = AgentConfig::default();

    let device_pool = Arc::new(DevicePool::new(
        device_pool_config,
        adb_server,
        model_config,
        agent_config,
    ));

    // 设置 DevicePool 到 Context
    ctx.set_device_pool(Arc::clone(&device_pool)).await;
    info!("DevicePool 初始化完成");

    // 创建并启动 API 服务器
    let api_server = api::api::ApiServer::new(ctx.clone() as Arc<dyn IContext + Sync + Send>);

    // 启动 API 服务器（端口 3000）
    let api_handle = tokio::spawn(async move {
        api_server.run().await;
    });

    // 创建并启动 Agent Socket.IO 服务器（端口 4000）
    let agent_socket_server = AgentSocketServer::new(4000, device_pool);
    info!("Agent Socket.IO 服务器配置完成，端口: 4000");

    // 启动 Agent Socket.IO 服务器
    let agent_handle = tokio::spawn(async move {
        agent_socket_server.run().await;
    });

    // 等待两个服务器
    tokio::select! {
        result = api_handle => {
            if let Err(e) = result {
                error!("API 服务器运行失败: {:?}", e);
            }
        }
        result = agent_handle => {
            if let Err(e) = result {
                error!("Agent Socket.IO 服务器运行失败: {:?}", e);
            }
        }
    }
}
