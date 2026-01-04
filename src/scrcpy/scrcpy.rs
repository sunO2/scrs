use adb_client::ADBDeviceExt;
use adb_client::server_device::ADBServerDevice;
use socketioxide::SocketIo;
use std::net::TcpListener;
use std::sync::Arc;
use tracing::{info, error, debug, warn};
use tower_http::cors::{CorsLayer, Any};
use tokio::io::{AsyncReadExt, AsyncWriteExt, split};
use tokio::net::TcpStream;

pub struct ScrcpyConnect {
    device: ADBServerDevice,
    port: u16,
    _socket_io: Arc<SocketIo>, // 保存 SocketIo 实例以保持服务器运行
    socket_writer: Arc<tokio::sync::Mutex<Option<tokio::io::WriteHalf<TcpStream>>>>, // Socket 写入端的共享引用
}

impl ScrcpyConnect {
    pub fn new(mut device: ADBServerDevice, reverse_port: u16) -> ScrcpyConnect {
        // 动态分配可用端口
        let listener = TcpListener::bind("127.0.0.1:0")
            .expect("Failed to bind to an available port");
        let port = listener.local_addr()
            .expect("Failed to get local address")
            .port();
        drop(listener); // 释放监听器，让 socketio 使用

        info!("为设备动态分配 socketio 端口: {}", port);

        // 创建 Socket.IO 服务器
        let (layer, io) = SocketIo::new_layer();
        
        // 在单独的异步任务中运行服务器
        let port_clone = port;
        tokio::spawn(async move {
            let cors = CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any);
            
            let app = axum::Router::new()
                .layer(cors)
                .layer(layer);
            
            let listener: tokio::net::TcpListener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port_clone))
                .await
                .expect("Failed to bind socketio server");
            
            info!("Socket.IO 服务器运行在端口: {}", port_clone);
            if let Err(e) = axum::serve(listener, app).await {
                error!("Socket.IO 服务器错误: {:?}", e);
            }
        });

        // 添加 test 事件处理
        io.ns("/", |s: socketioxide::extract::SocketRef| async move {
            s.on("test", |s: socketioxide::extract::SocketRef, data: socketioxide::extract::Data<serde_json::Value>| async move {
                info!("收到 test 事件: {:?}", data.0);
                // 回复客户端
                let _ = s.emit("test_response", &serde_json::json!({
                    "message": "test 事件已接收",
                    "received": data.0
                }));
            });
        });

        // 创建 socket 客户端连接到 reverse_port
        let socket_addr = format!("127.0.0.1:{}", reverse_port);
        info!("尝试连接到 socket 服务端: {}", socket_addr);
        
        let socket_writer = Arc::new(tokio::sync::Mutex::new(None::<tokio::io::WriteHalf<TcpStream>>));
        let socket_writer_for_event = Arc::clone(&socket_writer);
        let socket_writer_clone = Arc::clone(&socket_writer);
        let io_clone = io.clone();

        // 启动 socket 连接和数据透传任务
        tokio::spawn(async move {
            // 等待 scrcpy server 启动
            info!("等待 scrcpy server 启动 (3秒)...");
            // tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            
            loop {
                match TcpStream::connect(&socket_addr).await {
                    Ok(stream) => {
                        info!("成功连接到 socket 服务端: {}", socket_addr);
                        
                        // 拆分 stream 为 reader 和 writer
                        let (mut reader, writer) = split(stream);
                        
                        // 保存 writer 到共享状态
                        *socket_writer_clone.lock().await = Some(writer);
                        
                        // 创建 io 的克隆用于广播
                        let io_broadcast = io_clone.clone();
                        
                        // 任务1: 从 socket 服务端读取数据，广播到所有 Socket.IO 客户端
                        let read_task = tokio::spawn(async move {
                            let mut buffer = vec![0u8; 8192];
                            loop {
                                match reader.read(&mut buffer).await {
                                    Ok(0) => {
                                        debug!("socket 服务端关闭连接");
                                        break;
                                    }
                                    Ok(n) => {
                                        let data = &buffer[..n];
                                        debug!("从 socket 服务端读取到 {} 字节数据", n);
                                        
                                        // 将数据广播给所有 Socket.IO 客户端
                                        // 使用 scrcpy 事件名，直接发送二进制数据
                                        let _ = io_broadcast.emit("scrcpy", data);
                                        debug!("已广播 scrcpy 事件，数据长度: {}", n);
                                    }
                                    Err(e) => {
                                        error!("读取 socket 数据失败: {:?}", e);
                                        break;
                                    }
                                }
                            }
                        });
                        
                        // 等待读取任务完成
                        if let Err(e) = read_task.await {
                            error!("socket 读取任务出错: {:?}", e);
                        }
                        
                        // 清除 writer
                        *socket_writer_clone.lock().await = None;
                        warn!("与 socket 服务端的连接已断开");
                    }
                    Err(e) => {
                        error!("连接到 socket 服务端失败: {:?}, 3秒后重试...", e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                    }
                }
            }
        
          });

        let mut log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("logs/ws_{}.log", device.identifier.clone().unwrap())).unwrap();
        device.shell_command(  &"CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server 3.3.4 log_level=info audio=false max_size=1920",&mut std::io::stdout()).unwrap();
       

        // 添加 scrcpy_client 事件处理：接收 Socket.IO 客户端数据，透传给 socket 服务端
        io.ns("/", move |s: socketioxide::extract::SocketRef| {
            let writer = Arc::clone(&socket_writer_for_event);
            async move {
                s.on("scrcpy_client", move |_s: socketioxide::extract::SocketRef, data: socketioxide::extract::Data<String>| {
                    let writer = Arc::clone(&writer);
                    async move {
                        info!("收到 scrcpy_client 事件，数据长度: {}", data.0.len());
                        
                        // 获取 socket writer 并写入数据
                        let mut writer_guard = writer.lock().await;
                        if let Some(writer) = writer_guard.as_mut() {
                            if let Err(e) = writer.write_all(data.0.as_bytes()).await {
                                error!("写入 socket 服务端失败: {:?}", e);
                            } else {
                                debug!("成功向 socket 服务端写入 {} 字节数据", data.0.len());
                            }
                            if let Err(e) = writer.flush().await {
                                error!("刷新 socket 失败: {:?}", e);
                            }
                        } else {
                            warn!("socket 连接尚未建立，无法发送数据");
                        }
                    }
                });
            }
        });

        ScrcpyConnect {
            device,
            port,
            _socket_io: Arc::new(io),
            socket_writer,
        }
    }

    pub fn get_port(&self) -> u16 {
        self.port
    }

    pub fn get_device(&self) -> &ADBServerDevice {
        &self.device
    }
}
