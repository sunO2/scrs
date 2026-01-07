use std::{net::TcpListener, sync::Arc};
use axum::{
    extract::{State, Path},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
    body::Body,
};
use serde::{Deserialize, Serialize};
use tracing::{info, debug, warn};
use rust_embed::RustEmbed;
use crate::context::context::{IContext};
use crate::scrcpy::scrcpy::ScrcpyConnect;

/// 设备信息结构
#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub serial: String,
    pub status: String,
}

/// 设备列表响应
#[derive(Debug, Serialize)]
pub struct DevicesResponse {
    pub devices: Vec<DeviceInfo>,
    pub count: usize,
}

/// 连接设备请求
#[derive(Debug, Deserialize)]
pub struct ConnectDeviceRequest {
    pub serial: String,
}

/// 连接设备响应
#[derive(Debug, Serialize)]
pub struct ConnectResponse {
    pub serial: String,
    pub socketio_port: u16,
}

/// API 响应
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
}

/// Web 根目录文件资源
#[derive(RustEmbed)]
#[folder = "assets/root/"]
struct RootAssets;

pub struct ApiServer {
    pub app: Router,
}

impl ApiServer {
    pub fn new(ctx: Arc<dyn IContext + Sync + Send>) -> Self {
        let app = Router::new()
            .route("/devices", get(Self::get_devices))
            .route("/connect", post(Self::connect_device))
            .route("/disconnect", post(Self::disconnect_device))
            .route("/device/{serial}/status", get(Self::get_device_status))
            .route("/hello", get(Self::hello))
            .route("/web/{*path}", get(Self::serve_web_file))
            .with_state(ctx);
        ApiServer { app }
    }

    /// 启动 API 服务器
    pub async fn run(self) {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
            .await
            .expect("Failed to bind to 0.0.0.0:3000");
        println!("Server running on http://0.0.0.0:3000");
        
        if let Err(e) = axum::serve(listener, self.app).await {
            eprintln!("Server error: {:?}", e);
        }
    }

    /// 获取设备列表
    async fn get_devices(
        State(ctx): State<Arc<dyn IContext + Sync + Send>>,
    ) -> Json<DevicesResponse> {
        debug!("收到获取设备列表请求");
        
        // 通过 ADBServer 获取当前连接的设备
        let mut adb_server = ctx.get_adb_server().write().unwrap();
        let adb_devices: Result<Vec<adb_client::server::DeviceShort>, adb_client::RustADBError> = adb_server.devices();

        let devices: Vec<DeviceInfo> = match adb_devices {
            Ok(devs) => devs.iter().map(|device: &adb_client::server::DeviceShort| {
                info!("ADB 设备: {} - 状态: {}", device.identifier, device.state);
                DeviceInfo {
                    serial: device.identifier.clone(),
                    status: device.state.to_string(),
                }
            }).collect(),
            Err(e) => {
                warn!("获取设备列表失败: {:?}", e);
                vec![]
            }
        }; 

        let count = devices.len();
        info!("获取设备列表成功，共 {} 个设备", count);
        Json(DevicesResponse {
            devices,
            count,
        })
    }

    /// 连接设备
    async fn connect_device(
        State(ctx): State<Arc<dyn IContext + Sync + Send>>,
        Json(req): Json<ConnectDeviceRequest>,
    ) -> (StatusCode, Json<ApiResponse<ConnectResponse>>) {
        debug!("收到连接设备请求: {}", req.serial);

        // 优先检查设备是否已连接
        {
            let scrcpy_read = ctx.get_scrcpy().read().unwrap();
            if scrcpy_read.is_device_connected(&req.serial) {
                info!("设备 {} 已经连接，返回现有连接信息", req.serial);
                if let Some(connect) = scrcpy_read.get_device_connect(&req.serial) {
                    return (
                        StatusCode::OK,
                        Json(ApiResponse {
                            success: true,
                            message: format!("设备 {} 已连接", req.serial),
                            data: Some(ConnectResponse {
                                serial: req.serial.clone(),
                                socketio_port: connect.get_port(),
                            }),
                        })
                    );
                }
            }
        }
        // 释放读锁

        let mut scrcpy: std::sync::RwLockWriteGuard<'_, crate::context::context::ScrcpyServer> = ctx.get_scrcpy().write().unwrap();
        let mut adb = ctx.get_adb_server().write().unwrap();

        let jar_file = scrcpy.get_server_jar();
        let mut device = adb.get_device_by_name(&req.serial).unwrap();

        // 动态分配可用端口
        let listener = TcpListener::bind("127.0.0.1:0")
            .expect("Failed to bind to an available port");
        let port = listener.local_addr()
            .expect("Failed to get local address")
            .port();
        drop(listener);

        info!("reverse port: {}", port);
        let _ = device.forward_remove_all();
        device.forward(String::from("localabstract:scrcpy"), format!("tcp:{}", port)).unwrap();

        let push_status = device.push(jar_file, "/data/local/tmp/scrcpy-server.jar");
        match push_status {
            Ok(_) => info!("设备 {} 推送文件成功", req.serial),
            Err(e) => warn!("设备 {} 推送文件失败: {:?}", req.serial, e),
        }

        let connect: ScrcpyConnect = ScrcpyConnect::new(port);
        let socket_io_port = connect.get_port();
        let socket_io_port_1 = connect.get_port();
        tokio::spawn(async move {
            ScrcpyConnect::run(Arc::new(ScrcpyConnect::default(socket_io_port_1, port)), Arc::new(device)).await;
        });

        // 添加设备到管理列表
        scrcpy.add_device(req.serial.clone(), connect);
        info!("设备 {} 连接成功，Socket.IO 端口: {}", req.serial, socket_io_port);

        (
            StatusCode::OK,
            Json(ApiResponse {
                success: true,
                message: format!("设备 {} 连接成功", req.serial),
                data: Some(ConnectResponse {
                    serial: req.serial.clone(),
                    socketio_port: socket_io_port,
                }),
            })
        )
    }

    /// 断开设备连接
    async fn disconnect_device(
        State(ctx): State<Arc<dyn IContext + Sync + Send>>,
        Json(req): Json<ConnectDeviceRequest>,
    ) -> (StatusCode, Json<ApiResponse<String>>) {
        debug!("收到断开设备请求: {}", req.serial);
        let mut scrcpy = ctx.get_scrcpy().write().unwrap();
        
        if !scrcpy.is_device_connected(&req.serial) {
            warn!("设备 {} 未连接", req.serial);
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse {
                    success: false,
                    message: format!("设备 {} 未连接", req.serial),
                    data: None,
                })
            );
        }

        scrcpy.remove_device(&req.serial);
        info!("设备 {} 断开连接成功", req.serial);
        
        (
            StatusCode::OK,
            Json(ApiResponse {
                success: true,
                message: format!("设备 {} 断开连接成功", req.serial),
                data: Some(req.serial),
            })
        )
    }

    /// 获取设备状态
    async fn get_device_status(
        State(ctx): State<Arc<dyn IContext + Sync + Send>>,
        axum::extract::Path(serial): axum::extract::Path<String>,
    ) -> (StatusCode, Json<ApiResponse<DeviceInfo>>) {
        debug!("收到获取设备状态请求: {}", serial);
        let scrcpy = ctx.get_scrcpy().read().unwrap();
        
        match scrcpy.get_device_connect(&serial) {
            Some(_connect) => {
                info!("获取设备 {} 状态成功", serial);
                (
                    StatusCode::OK,
                    Json(ApiResponse {
                        success: true,
                        message: "获取设备状态成功".to_string(),
                        data: Some(DeviceInfo {
                            serial: serial.clone(),
                            status: "connected".to_string(),
                        }),
                    })
                )
            },
            None => {
                warn!("设备 {} 未找到", serial);
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse {
                        success: false,
                        message: format!("设备 {} 未找到", serial),
                        data: None,
                    })
                )
            }
        }
    }

    /// 测试端点
    async fn hello() -> String {
        "你好，欢迎使用 Axum Scrcpy API！".to_string()
    }

    /// 服务 Web 静态文件
    /// 支持 /web/* 路径访问 assets/root/ 下的所有文件
    async fn serve_web_file(Path(path): Path<String>) -> impl IntoResponse {
        // 处理根路径请求
        let file_path = if path.is_empty() || path == "/" {
            "index.html"
        } else {
            // 移除前导斜杠
            path.trim_start_matches('/')
        };

        // 根据文件扩展名获取 MIME 类型
        let mime_type = Self::get_mime_type(file_path);

        // 从嵌入的文件中获取
        match RootAssets::get(file_path) {
            Some(content) => {
                Response::builder()
                    .header("Content-Type", mime_type)
                    .body(Body::from(content.data.to_vec()))
                    .unwrap()
            }
            None => {
                // 如果请求的是目录，尝试添加 index.html
                if !file_path.contains('.') {
                    match RootAssets::get(&format!("{}/index.html", file_path)) {
                        Some(content) => {
                            Response::builder()
                                .header("Content-Type", "text/html")
                                .body(Body::from(content.data.to_vec()))
                                .unwrap()
                        }
                        None => {
                            Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .body(Body::from("文件未找到"))
                                .unwrap()
                        }
                    }
                } else {
                    Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::from("文件未找到"))
                        .unwrap()
                }
            }
        }
    }

    /// 根据文件扩展名获取 MIME 类型
    fn get_mime_type(path: &str) -> &'static str {
        let path_lower = path.to_lowercase();
        if path_lower.ends_with(".html") || path_lower.ends_with(".htm") {
            "text/html"
        } else if path_lower.ends_with(".css") {
            "text/css"
        } else if path_lower.ends_with(".js") {
            "application/javascript"
        } else if path_lower.ends_with(".json") {
            "application/json"
        } else if path_lower.ends_with(".png") {
            "image/png"
        } else if path_lower.ends_with(".jpg") || path_lower.ends_with(".jpeg") {
            "image/jpeg"
        } else if path_lower.ends_with(".gif") {
            "image/gif"
        } else if path_lower.ends_with(".svg") {
            "image/svg+xml"
        } else if path_lower.ends_with(".ico") {
            "image/x-icon"
        } else if path_lower.ends_with(".woff") {
            "font/woff"
        } else if path_lower.ends_with(".woff2") {
            "font/woff2"
        } else if path_lower.ends_with(".ttf") {
            "font/ttf"
        } else if path_lower.ends_with(".eot") {
            "application/vnd.ms-fontobject"
        } else if path_lower.ends_with(".mp4") {
            "video/mp4"
        } else if path_lower.ends_with(".webm") {
            "video/webm"
        } else if path_lower.ends_with(".mp3") {
            "audio/mpeg"
        } else if path_lower.ends_with(".wav") {
            "audio/wav"
        } else if path_lower.ends_with(".pdf") {
            "application/pdf"
        } else if path_lower.ends_with(".zip") {
            "application/zip"
        } else {
            "application/octet-stream"
        }
    }
}
