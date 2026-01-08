use socketioxide::extract::{SocketRef, Data};
use serde::{Deserialize, Serialize};
use tracing::error;
use std::sync::Arc;
use crate::context::IContext;
use crate::agent::core::traits::Agent;

pub async fn register_agent_handlers(socket: SocketRef, context: Arc<dyn IContext>) {
    // 提取 device_pool 的引用，使其可克隆
    let device_pool = {
        let guard = context.get_device_pool().read().await;
        guard.as_ref().map(Arc::clone)
    };

    if let Some(pool) = device_pool {
        // agent/start
        {
            let pool = Arc::clone(&pool);
            socket.on("agent/start", move |s: SocketRef, data: Data<serde_json::Value>| {
                let pool = Arc::clone(&pool);
                async move {
                    let request: AgentStartRequest = match serde_json::from_value(data.0) {
                        Ok(r) => r,
                        Err(e) => {
                            error!("解析请求失败: {}", e);
                            let _ = s.emit("agent/start/response", &serde_json::json!({
                                "success": false,
                                "error": format!("请求解析失败: {}", e)
                            }));
                            return;
                        }
                    };

                    match handle_agent_start_with_pool(request, pool).await {
                        Ok(resp) => { let _ = s.emit("agent/start/response", &resp); }
                        Err(e) => {
                            error!("启动 Agent 失败: {}", e);
                            let _ = s.emit("agent/start/response", &serde_json::json!({
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
            let pool = Arc::clone(&pool);
            socket.on("agent/devices", move |s: SocketRef, _data: Data<serde_json::Value>| {
                let pool = Arc::clone(&pool);
                async move {
                    match handle_get_devices_with_pool(pool).await {
                        Ok(resp) => { let _ = s.emit("agent/devices/response", &resp); }
                        Err(e) => {
                            error!("获取设备列表失败: {}", e);
                            let _ = s.emit("agent/devices/response", &serde_json::json!({
                                "success": false,
                                "error": e.to_string()
                            }));
                        }
                    }
                }
            });
        }
    }
}

async fn handle_agent_start_with_pool(request: AgentStartRequest, pool: Arc<crate::agent::pool::DevicePool>) -> Result<serde_json::Value, crate::error::AppError> {
    let _ = pool.register_device(request.device_serial.clone(), None).await;
    let agent = pool.get_agent(&request.device_serial).await?;
    let agent_id = agent.start(request.task.clone()).await?;
    pool.update_task_status(&request.device_serial, agent_id.clone(), request.task.clone()).await?;

    Ok(serde_json::json!({ "success": true, "agent_id": agent_id, "device_serial": request.device_serial, "task": request.task }))
}

async fn handle_get_devices_with_pool(pool: Arc<crate::agent::pool::DevicePool>) -> Result<serde_json::Value, crate::error::AppError> {
    let devices = pool.get_all_devices_info().await;
    Ok(serde_json::json!({ "success": true, "devices": devices }))
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AgentStartRequest {
    pub device_serial: String,
    pub task: String,
}
