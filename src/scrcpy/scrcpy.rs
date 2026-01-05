use adb_client::ADBDeviceExt;
use adb_client::server_device::ADBServerDevice;
use socketioxide::SocketIo;
use bytes::Bytes;
use std::net::TcpListener;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tracing::{info, error, debug, warn};
use tower_http::cors::{CorsLayer, Any};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct ScrcpyConnect {
    port: u16,
    scrcpy_server_port: u16,
}

impl ScrcpyConnect {

    pub fn default(port: u16,scrcpy_server_port: u16) -> ScrcpyConnect{
        ScrcpyConnect{
            port,
            scrcpy_server_port
        }
    }

    pub fn new(scrcpy_server_port: u16) -> ScrcpyConnect {
        // 动态分配可用端口
        let listener = TcpListener::bind("127.0.0.1:0")
            .expect("Failed to bind to an available port");
        let port = listener.local_addr()
            .expect("Failed to get local address")
            .port();
        drop(listener); // 释放监听器，让 socketio 使用

        info!("为设备动态分配 socketio 端口: {}", port);
        ScrcpyConnect {
            port,
            scrcpy_server_port
        }
    }

    pub fn get_port(&self) -> u16 {
        self.port
    }

    /**
     * 运行连接
     */
     pub async fn run(self: Arc<Self>,device: Arc<ADBServerDevice>){
        // 创建 channel 用于传输数据: socket client -> socketio server
        let (scrcpy_data_tx, mut scrcpy_data_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        // 将第二个 socket 的写句柄包装在 Arc<Mutex<>> 中以便多个任务共享
        let scrcpy_control_write: Arc<Mutex<Option<tokio::net::tcp::OwnedWriteHalf>>> = Arc::new(Mutex::new(None));

        // 创建 Socket.IO 服务器
        let socket_io_port = self.port;
        let scrcpy_jar_server_port = self.scrcpy_server_port;
        let (layer, io) = SocketIo::new_layer();
        // 将 io 包装在 Arc 中，以便在多个任务中共享
        let io = Arc::new(io);
            let cors = CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any);

            let app = axum::Router::new()
                .layer(cors)
                .layer(layer);

            let listener: tokio::net::TcpListener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", socket_io_port))
                .await
                .expect("Failed to bind socketio server");

            // 添加 scrcpy 事件处理 - 用于接收客户端请求
            let scrcpy_control_write_clone = Arc::clone(&scrcpy_control_write);
            io.ns("/", move |s: socketioxide::extract::SocketRef| async move {
                let scrcpy_control_write_ref = scrcpy_control_write_clone.clone();
                s.on("test", |s: socketioxide::extract::SocketRef, data: socketioxide::extract::Data<serde_json::Value>| async move {
                    info!("收到 test 事件: {:?}", data.0);
                    // 回复客户端
                    let _ = s.emit("test_response", &serde_json::json!({
                        "message": "test 事件已接收",
                        "received": data.0
                    }));
                });

                // 处理客户端发送的 scrcpy_ctl 二进制数据，写入到第二个 socket
                s.on("scrcpy_ctl", move |s: socketioxide::extract::SocketRef, data: socketioxide::extract::Data<Bytes>| async move {
                    info!("收到 scrcpy_ctl 事件，数据长度: {} 字节", data.0.len());

                    // 打印前16字节的十六进制数据用于调试
                    let preview_len = std::cmp::min(16, data.0.len());
                    let preview: Vec<u8> = data.0[..preview_len].to_vec();
                    let hex_str: String = preview.iter().map(|b| format!("{:02x}", b)).collect();
                    info!("数据预览 (前{}字节): {}", preview_len, hex_str);

                    let mut write_guard = scrcpy_control_write_ref.lock().await;
                    if let Some(ref mut write_half) = *write_guard {
                        if let Err(e) = write_half.write_all(&data.0).await {
                            error!("写入 scrcpy control socket 失败: {:?}", e);
                            // 发送错误响应
                            let _ = s.emit("scrcpy_ctl_error", &serde_json::json!({
                                "error": format!("写入失败: {:?}", e),
                                "length": data.0.len()
                            }));
                        } else {
                            debug!("成功写入 scrcpy control socket，长度: {} 字节", data.0.len());
                            // 发送成功确认
                            let _ = s.emit("scrcpy_ctl_ack", &serde_json::json!({
                                "status": "ok",
                                "length": data.0.len()
                            }));
                        }
                    } else {
                        warn!("Scrcpy control socket 写句柄未就绪");
                        // 发送未就绪响应
                        let _ = s.emit("scrcpy_ctl_error", &serde_json::json!({
                            "error": "control socket 未就绪",
                            "length": data.0.len()
                        }));
                    }
                });
            });

            info!("Socket.IO 服务器运行在端口: {}",socket_io_port);
            let socket_io_task = axum::serve(listener, app);

            // 启动 scrcpy server
            let device_identifier = device.identifier.clone();
            let scrcpy_jar_task = tokio::spawn(async move {
                let mut log_file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(format!("logs/ws_{}.log", device_identifier.unwrap())).unwrap();
                let mut device_owned = Arc::try_unwrap(device).unwrap();
                device_owned.shell_command(
                    &"CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server 3.3.4 log_level=info audio=false max_size=1920 tunnel_forward=true",
                    &mut log_file
                ).unwrap();
                 info!("设备运行: {}",socket_io_port);
            });

            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            let socket_addr = format!("127.0.0.1:{}", scrcpy_jar_server_port);
            info!("尝试连接到 scrcpy socket 服务端: {}", socket_addr);

            // 第一个 Socket client: 连接到 scrcpy server，只读取数据并通过 channel 发送
            let socket_addr_1 = socket_addr.clone();
            let socket_read = tokio::spawn(async move {
                let stream = TcpStream::connect(&socket_addr_1).await.unwrap();

                info!("第一个 scrcpy socket 连接成功，开始读取数据");

                let mut read = stream;
                let mut buf = vec![0; 8192];
                loop {
                    match read.read(&mut buf).await {
                        Ok(0) => {
                            warn!("第一个 Scrcpy socket 连接关闭");
                            break;
                        }
                        Ok(n) => {
                            // 通过 channel 发送数据
                            let data = buf[..n].to_vec();
                            if let Err(e) = scrcpy_data_tx.send(data) {
                                error!("发送数据到 channel 失败: {:?}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            error!("读取 scrcpy socket 数据错误: {:?}", e);
                            break;
                        }
                    }
                }
            });

            // 等待第一个 socket 连接成功
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            // 第二个 Socket client: 连接到 scrcpy server，用于写入控制数据
            let scrcpy_control_write_for_socket = Arc::clone(&scrcpy_control_write);
            let socket_write = tokio::spawn(async move {
                let stream = TcpStream::connect(&socket_addr).await.unwrap();

                info!("第二个 scrcpy socket 连接成功，用于写入控制数据");

                // 将写句柄保存到共享的 Arc<Mutex<>> 中
                let write = stream.into_split().1;
                {
                    let mut write_guard = scrcpy_control_write_for_socket.lock().await;
                    *write_guard = Some(write);
                    info!("Scrcpy control socket 写句柄已就绪");
                }

                // 保持写句柄活跃，直到任务结束
                // 写句柄已被移到 Arc<Mutex<>> 中，这里只需保持任务运行
                tokio::time::sleep(tokio::time::Duration::from_secs(u64::MAX)).await;
            });

            // SocketIO 广播任务: 从 channel 接收数据并广播给所有 SocketIO 客户端
            let io_clone = Arc::clone(&io);
            let socketio_broadcast = tokio::spawn(async move {
                info!("启动 SocketIO 广播任务");
                while let Some(data) = scrcpy_data_rx.recv().await {

                    // 将二进制数据广播到所有连接到 "/" namespace 的客户端
                    // 使用 base64 编码，因为 Socket.IO 事件通常传输 JSON/文本数据
                    use base64::prelude::*;
                    let base64_data = BASE64_STANDARD.encode(&data);

                    // emit API: io.emit(event, data)，默认广播到 "/" namespace
                    if let Err(e) = io_clone.emit("scrcpy", &base64_data).await {
                        error!("广播 scrcpy 数据失败: {:?}", e);
                    }
                }
                info!("SocketIO 广播任务结束");
            });

            let _join_result = tokio::join!(socket_io_task, scrcpy_jar_task, socket_read, socket_write, socketio_broadcast);
    }

}
