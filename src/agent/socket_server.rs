//! Agent Socket.IO 服务器
//!
//! 提供全局的 Agent 控制 Socket.IO 服务，与设备屏幕流分离

use socketioxide::{
    SocketIo,
    extract::SocketRef,
    layer::SocketIoLayer,
};
use std::sync::Arc;
use tracing::{info, error, debug};
use crate::agent::pool::DevicePool;
use crate::agent::core::traits::Agent;
use axum::Router;

/// Agent Socket.IO 服务器
///
/// 独立于 ScrcpyConnect 的屏幕流服务，专门用于 Agent 控制
pub struct AgentSocketServer {
    io: Arc<SocketIo>,
    layer: SocketIoLayer,
    port: u16,
}

impl AgentSocketServer {
    /// 创建新的 Agent Socket.IO 服务器
    ///
    /// # 参数
    /// - `port`: Socket.IO 服务端口（建议 4000）
    /// - `device_pool`: 设备池实例
    pub fn new(port: u16, device_pool: Arc<DevicePool>) -> Self {
        let (layer, io) = SocketIo::new_layer();
        let io = Arc::new(io);

        info!("创建 Agent Socket.IO 服务器，端口: {}", port);

        // 注册默认命名空间的 Agent 处理器
        let device_pool_clone = Arc::clone(&device_pool);

        io.ns("/", move |socket: SocketRef| async move {
            debug!("新客户端连接到 Agent Socket.IO: {}", socket.id);
            register_agent_handlers_with_pool(socket, Arc::clone(&device_pool_clone)).await;
        });

        Self { io, layer, port }
    }

    /// 启动服务器
    pub async fn run(self) {
        let addr = format!("0.0.0.0:{}", self.port);
        info!("Agent Socket.IO 服务器启动于: {}", addr);

        // 创建 axum 应用，集成 Socket.IO layer
        let app = Router::new()
            .layer(self.layer);

        // 绑定到地址
        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                error!("无法绑定到 {}: {}", addr, e);
                return;
            }
        };

        info!("Agent Socket.IO 服务器正在监听 {}", addr);

        // 使用 axum 运行服务器
        if let Err(e) = axum::serve(listener, app).await {
            error!("Agent Socket.IO 服务器错误: {:?}", e);
        }
    }

    /// 获取 Socket.IO 实例（用于集成到 axum）
    pub fn io(&self) -> &Arc<SocketIo> {
        &self.io
    }

    /// 获取端口
    pub fn port(&self) -> u16 {
        self.port
    }
}

/// 直接使用 DevicePool 注册 Agent 处理器
///
/// 这是 register_agent_handlers 的简化版本，不需要完整的 IContext
async fn register_agent_handlers_with_pool(socket: SocketRef, device_pool: Arc<DevicePool>) {
    use socketioxide::extract::Data;
    use serde_json::json;

    // agent/start
    {
        let pool = Arc::clone(&device_pool);
        socket.on("agent/start", move |s: SocketRef, data: Data<serde_json::Value>| {
            let pool = Arc::clone(&pool);
            async move {
                debug!("收到 agent/start 请求: {:?}", data.0);

                // 解析请求
                let device_serial = data.0.get("device_serial")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let task = data.0.get("task")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if device_serial.is_empty() || task.is_empty() {
                    let _ = s.emit("agent/start/response", &json!({
                        "success": false,
                        "error": "缺少 device_serial 或 task 参数"
                    }));
                    return;
                }

                // 注册设备（如果尚未注册）
                let _ = pool.register_device(device_serial.to_string(), None).await;

                // 获取或创建 Agent
                match pool.get_agent(device_serial).await {
                    Ok(agent) => {
                        // 启动任务
                        match agent.start(task.to_string()).await {
                            Ok(agent_id) => {
                                // 更新任务状态
                                let _ = pool.update_task_status(
                                    device_serial,
                                    agent_id.clone(),
                                    task.to_string(),
                                ).await;

                                let _ = s.emit("agent/start/response", &json!({
                                    "success": true,
                                    "agent_id": agent_id,
                                    "device_serial": device_serial,
                                    "task": task
                                }));
                            }
                            Err(e) => {
                                error!("启动 Agent 任务失败: {}", e);
                                let _ = s.emit("agent/start/response", &json!({
                                    "success": false,
                                    "error": e.to_string()
                                }));
                            }
                        }
                    }
                    Err(e) => {
                        error!("获取 Agent 失败: {}", e);
                        let _ = s.emit("agent/start/response", &json!({
                            "success": false,
                            "error": e.to_string()
                        }));
                    }
                }
            }
        });
    }

    // agent/devices
    {
        let pool = Arc::clone(&device_pool);
        socket.on("agent/devices", move |s: SocketRef, _data: Data<serde_json::Value>| {
            let pool = Arc::clone(&pool);
            async move {
                debug!("收到 agent/devices 请求");

                let devices = pool.get_all_devices_info().await;

                let _ = s.emit("agent/devices/response", &json!({
                    "success": true,
                    "devices": devices
                }));
            }
        });
    }

    // agent/stop
    {
        let pool = Arc::clone(&device_pool);
        socket.on("agent/stop", move |s: SocketRef, data: Data<serde_json::Value>| {
            let pool = Arc::clone(&pool);
            async move {
                debug!("收到 agent/stop 请求: {:?}", data.0);

                let device_serial = data.0.get("device_serial")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if device_serial.is_empty() {
                    let _ = s.emit("agent/stop/response", &json!({
                        "success": false,
                        "error": "缺少 device_serial 参数"
                    }));
                    return;
                }

                match pool.release_agent(device_serial).await {
                    Ok(_) => {
                        let _ = s.emit("agent/stop/response", &json!({
                            "success": true,
                            "device_serial": device_serial
                        }));
                    }
                    Err(e) => {
                        error!("停止 Agent 失败: {}", e);
                        let _ = s.emit("agent/stop/response", &json!({
                            "success": false,
                            "error": e.to_string()
                        }));
                    }
                }
            }
        });
    }

    debug!("Agent Socket.IO 处理器已注册");
}
