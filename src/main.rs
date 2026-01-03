mod api;
mod context;
mod error;

use std::sync::Arc;
use tracing::{info, error};
use tracing_subscriber::{EnvFilter, fmt};

use context::context::{Context, IContext};

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
    
    // 创建并启动 API 服务器
    let api_server: api::api::ApiServer = api::api::ApiServer::new(ctx as Arc<dyn IContext + Sync + Send>);
    
    if let Err(e) = tokio::spawn(async move {
        api_server.run().await
    }).await {
        error!("API 服务器运行失败: {:?}", e);
    }
}
