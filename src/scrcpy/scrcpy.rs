use adb_client::server_device::ADBServerDevice;
use socketioxide::{SocketIo, socket::DisconnectReason};
use bytes::Bytes;
use std::net::TcpListener;
use std::sync::Arc;
use std::io::Write as StdWrite;
use std::collections::HashSet;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{info, error, debug, warn};
use tower_http::cors::{CorsLayer, Any};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use rust_embed::RustEmbed;

/// 嵌入的资源文件
#[derive(RustEmbed)]
#[folder = "assets/"]
struct Assets;

/// Socket read state machine for handling first two special messages
enum ReadState {
    ReadAck,   // Read 1 byte acknowledgment
    ReadMeta,  // Read 64 bytes device metadata
    ReadData,  // Normal data forwarding
}

/// 跟踪单个 scrcpy 会话的所有动态管理任务
struct ScrcpySessionTasks {
    /// scrcpy-server.jar ADB shell 任务句柄
    scrcpy_jar_handle: Option<JoinHandle<()>>,
    /// TCP socket 读取任务句柄
    socket_read_handle: Option<JoinHandle<()>>,
    /// TCP socket 写入任务句柄
    socket_write_handle: Option<JoinHandle<()>>,
    /// Socket.IO 广播任务句柄
    broadcast_handle: Option<JoinHandle<()>>,
    /// 共享的写句柄 (scrcpy_ctl -> device)
    scrcpy_control_write: Arc<Mutex<Option<tokio::net::tcp::OwnedWriteHalf>>>,
    /// 所有连接的 Socket.IO 客户端 ID 集合
    connected_clients: HashSet<String>,
    /// 设备元数据 (设备名称)
    device_meta: Option<String>,
}

impl ScrcpySessionTasks {
    /// 创建新的会话任务跟踪器
    fn new() -> Self {
        Self {
            scrcpy_jar_handle: None,
            socket_read_handle: None,
            socket_write_handle: None,
            broadcast_handle: None,
            scrcpy_control_write: Arc::new(Mutex::new(None)),
            connected_clients: HashSet::new(),
            device_meta: None,
        }
    }

    /// 中止所有运行中的任务并清理资源
    async fn abort_all(&mut self) {
        info!("中止所有 scrcpy 会话任务");

        // 清空控制写句柄
        let mut write_guard = self.scrcpy_control_write.lock().await;
        *write_guard = None;
        drop(write_guard);

        // 中止所有已生成的任务
        if let Some(handle) = self.scrcpy_jar_handle.take() {
            handle.abort();
            info!("已中止 scrcpy_jar 任务");
        }
        if let Some(handle) = self.socket_read_handle.take() {
            handle.abort();
            info!("已中止 socket_read 任务");
        }
        if let Some(handle) = self.socket_write_handle.take() {
            handle.abort();
            info!("已中止 socket_write 任务");
        }
        if let Some(handle) = self.broadcast_handle.take() {
            handle.abort();
            info!("已中止 broadcast 任务");
        }

        // 清空所有连接的客户端
        let client_count = self.connected_clients.len();
        self.connected_clients.clear();
        info!("已清空所有连接的客户端，共 {} 个", client_count);

        // 清空设备元数据
        self.device_meta = None;
    }

    /// 只中止任务，保留客户端集合（用于重启会话）
    async fn abort_tasks_only(&mut self) {
        info!("中止 scrcpy 任务（保留客户端集合）");

        // 清空控制写句柄
        let mut write_guard = self.scrcpy_control_write.lock().await;
        *write_guard = None;
        drop(write_guard);

        // 中止所有已生成的任务
        if let Some(handle) = self.scrcpy_jar_handle.take() {
            handle.abort();
            info!("已中止 scrcpy_jar 任务");
        }
        if let Some(handle) = self.socket_read_handle.take() {
            handle.abort();
            info!("已中止 socket_read 任务");
        }
        if let Some(handle) = self.socket_write_handle.take() {
            handle.abort();
            info!("已中止 socket_write 任务");
        }
        if let Some(handle) = self.broadcast_handle.take() {
            handle.abort();
            info!("已中止 broadcast 任务");
        }

        info!("保留 {} 个连接的客户端", self.connected_clients.len());
    }

    /// 移除一个客户端，如果没有剩余客户端则返回 true
    fn remove_client(&mut self, client_id: &str) -> bool {
        let removed = self.connected_clients.remove(client_id);
        if removed {
            info!("移除客户端: {}, 剩余客户端数: {}", client_id, self.connected_clients.len());
        }
        self.connected_clients.is_empty()  // 如果没有客户端剩余则返回 true
    }

    /// 添加一个新的客户端
    fn add_client(&mut self, client_id: String) {
        self.connected_clients.insert(client_id);
        info!("添加客户端, 当前客户端数: {}", self.connected_clients.len());
    }

    /// 检查是否有任何已连接的客户端
    fn has_clients(&self) -> bool {
        !self.connected_clients.is_empty()
    }

    /// 检查会话是否正在运行
    fn is_session_running(&self) -> bool {
        self.scrcpy_jar_handle.is_some()
    }
}

impl Default for ScrcpySessionTasks {
    fn default() -> Self {
        Self::new()
    }
}

/// 共享状态，用于管理 scrcpy 会话
struct ScrcpySessionState {
    /// 当前活跃的会话任务
    session: Arc<Mutex<ScrcpySessionTasks>>,
    /// 设备引用 (用于 ADB 命令)
    device: Arc<ADBServerDevice>,
    /// scrcpy-server.jar 端口
    scrcpy_server_port: u16,
    /// Socket.IO 端口
    socket_io_port: u16,
    /// Socket.IO 引用 (用于广播)
    io: Arc<SocketIo>,
}

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
     * 运行连接 - 事件驱动模式
     * Socket.IO 服务器持续运行，scrcpy-server 在客户端连接时启动
     */
    pub async fn run(self: Arc<Self>, device: Arc<ADBServerDevice>) {
        let socket_io_port = self.port;
        let scrcpy_server_port = self.scrcpy_server_port;

        // 创建 Socket.IO 服务器
        let (layer, io) = SocketIo::new_layer();
        let io = Arc::new(io);

        // 创建会话状态
        let session_state = Arc::new(ScrcpySessionState {
            session: Arc::new(Mutex::new(ScrcpySessionTasks::new())),
            device,
            scrcpy_server_port,
            socket_io_port,
            io: io.clone(),
        });

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

        info!("Socket.IO 服务器运行在端口: {}, 等待客户端连接...", socket_io_port);

        // 设置事件处理器
        let state_clone = session_state.clone();
        io.ns("/", move |s: socketioxide::extract::SocketRef| async move {
            let state = state_clone.clone();
            let socket_id = s.id.to_string();

            info!("客户端连接: {}", socket_id);

            // 获取 scrcpy_control_write 引用用于事件处理器
            let scrcpy_control_write = {
                let session = state.session.try_lock();
                if let Ok(session) = session {
                    Arc::clone(&session.scrcpy_control_write)
                } else {
                    Arc::new(Mutex::new(None))
                }
            };

            // test 事件处理器
            s.on("test", |s: socketioxide::extract::SocketRef, data: socketioxide::extract::Data<serde_json::Value>| async move {
                info!("收到 test 事件: {:?}", data.0);
                let _ = s.emit("test_response", &serde_json::json!({
                    "message": "test 事件已接收",
                    "received": data.0
                }));
            });

            // scrcpy_ctl 事件处理器
            let scrcpy_control_write_ref = scrcpy_control_write.clone();
            s.on("scrcpy_ctl", move |s: socketioxide::extract::SocketRef, data: socketioxide::extract::Data<Bytes>| async move {
                info!("收到 scrcpy_ctl 事件，数据长度: {} 字节", data.0.len());

                let preview_len = std::cmp::min(16, data.0.len());
                let preview: Vec<u8> = data.0[..preview_len].to_vec();
                let hex_str: String = preview.iter().map(|b| format!("{:02x}", b)).collect();
                info!("数据预览 (前{}字节): {}", preview_len, hex_str);

                let mut write_guard = scrcpy_control_write_ref.lock().await;
                if let Some(ref mut write_half) = *write_guard {
                    if let Err(e) = write_half.write_all(&data.0).await {
                        error!("写入 scrcpy control socket 失败: {:?}", e);
                        let _ = s.emit("scrcpy_ctl_error", &serde_json::json!({
                            "error": format!("写入失败: {:?}", e),
                            "length": data.0.len()
                        }));
                    } else {
                        debug!("成功写入 scrcpy control socket，长度: {} 字节", data.0.len());
                        let _ = s.emit("scrcpy_ctl_ack", &serde_json::json!({
                            "status": "ok",
                            "length": data.0.len()
                        }));
                    }
                } else {
                    warn!("Scrcpy control socket 写句柄未就绪");
                    let _ = s.emit("scrcpy_ctl_error", &serde_json::json!({
                        "error": "control socket 未就绪",
                        "length": data.0.len()
                    }));
                }
            });

            // 连接处理器 - 启动 scrcpy 会话
            let state_for_connect = state.clone();
            let socket_id_for_connect = socket_id.clone();
            tokio::spawn(async move {
                handle_client_connect(state_for_connect, socket_id_for_connect).await;
            });

            // 断开连接处理器 - 停止 scrcpy 会话
            s.on_disconnect(move |s: socketioxide::extract::SocketRef, _reason: DisconnectReason| async move {
                let socket_id = s.id.to_string();
                info!("客户端断开连接: {}", socket_id);

                let mut session = state.session.lock().await;

                // 移除客户端并检查是否是最后一个
                let should_abort = session.remove_client(&socket_id);

                if should_abort {
                    info!("最后一个客户端断开，中止 scrcpy 会话: {}", socket_id);
                    session.abort_all().await;
                } else {
                    info!("客户端 {} 断开，但仍有 {} 个客户端连接，会话继续",
                          socket_id, session.connected_clients.len());
                }
            });
        });

        // 只运行 Socket.IO 服务器
        axum::serve(listener, app).await.unwrap();
    }
}

/// 处理客户端连接事件
async fn handle_client_connect(state: Arc<ScrcpySessionState>, socket_id: String) {
    let mut session = state.session.lock().await;

    // 添加此客户端到连接集合
    session.add_client(socket_id.clone());

    // 检查是否已有会话在运行
    if session.is_session_running() {
        info!("新客户端 {} 连接，中止旧的 scrcpy 任务并重启（保留所有客户端）", socket_id);
        // 只中止任务，保留客户端集合
        session.abort_tasks_only().await;
        // 等待清理完成
        drop(session);
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        // 启动新的会话（会广播给所有客户端）
        start_scrcpy_session(state, socket_id).await;
    } else {
        info!("第一个客户端连接，启动新的 scrcpy 会话");
        drop(session);
        start_scrcpy_session(state, socket_id).await;
    }
}

/// 启动 scrcpy 会话的所有任务
async fn start_scrcpy_session(state: Arc<ScrcpySessionState>, client_socket_id: String) {
    info!("为客户端 {} 启动 scrcpy 会话", client_socket_id);

    // 创建通信通道
    let (scrcpy_data_tx, mut scrcpy_data_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    let scrcpy_control_write = Arc::clone(&state.session.lock().await.scrcpy_control_write);
    let device = Arc::clone(&state.device);
    let io = Arc::clone(&state.io);
    let socket_addr = format!("127.0.0.1:{}", state.scrcpy_server_port);

    // 任务 1: 启动 scrcpy-server.jar (使用 ADB shell 命令)
    let device_identifier = device.identifier.clone();
    let client_socket_id_jar = client_socket_id.clone();
    let scrcpy_jar_handle = tokio::spawn(async move {
        let device_serial = device_identifier.unwrap();
        let log_path = format!("logs/ws_{}.log", device_serial);
        let mut log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path).unwrap();

        info!("客户端 {} 的 scrcpy jar 任务启动，日志: {}", client_socket_id_jar, log_path);

        // 步骤 1: 推送 scrcpy-server.jar 到设备
        info!("客户端 {} 正在推送 scrcpy-server.jar 到设备 {}", client_socket_id_jar, device_serial);

        // 获取嵌入的 jar 文件
        let jar_data = Assets::get("jar/scrcpy-server-v3.3.4.jar");
        if jar_data.is_none() {
            error!("无法找到嵌入的 scrcpy-server.jar 文件");
            let _ = writeln!(&mut log_file, "错误: 无法找到嵌入的 scrcpy-server.jar 文件");
            return;
        }

        let jar_data = jar_data.unwrap().data.to_vec();

        // 先将 jar 文件写入临时文件
        let temp_jar_path = format!("/tmp/scrcpy-server-{}.jar", client_socket_id_jar);
        if let Err(e) = std::fs::write(&temp_jar_path, &jar_data) {
            error!("写入临时 jar 文件失败: {:?}", e);
            let _ = writeln!(&mut log_file, "写入临时 jar 文件失败: {:?}", e);
            return;
        }

        info!("临时 jar 文件已创建: {}", temp_jar_path);

        // 使用 adb push 推送 jar 文件 (指定设备)
        let push_result = tokio::process::Command::new("adb")
            .args(["-s", &device_serial, "push", &temp_jar_path, "/data/local/tmp/scrcpy-server.jar"])
            .output()
            .await;

        match &push_result {
            Ok(output) => {
                if output.status.success() {
                    info!("推送 scrcpy-server.jar 到设备 {} 成功", device_serial);
                    let _ = writeln!(&mut log_file, "推送 scrcpy-server.jar 成功");
                } else {
                    error!("推送 scrcpy-server.jar 失败: {:?}", String::from_utf8_lossy(&output.stderr));
                    let _ = writeln!(&mut log_file, "推送失败: {:?}", String::from_utf8_lossy(&output.stderr));
                    return;
                }
            }
            Err(e) => {
                error!("执行 adb push 失败: {:?}", e);
                let _ = writeln!(&mut log_file, "adb push 执行失败: {:?}", e);
                return;
            }
        }

        // 步骤 2: 启动 scrcpy-server
        let command = "CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server 3.3.4 log_level=info audio=false max_size=1920 tunnel_forward=true";

        info!("客户端 {} 正在为设备 {} 启动 scrcpy-server", client_socket_id_jar, device_serial);

        let result = tokio::process::Command::new("adb")
            .args(["-s", &device_serial, "shell", command])
            .output()
            .await;

        match result {
            Ok(output) => {
                if let Err(e) = log_file.write_all(&output.stdout) {
                    error!("写入 scrcpy jar 日志失败: {:?}", e);
                }
                if !output.stderr.is_empty() {
                    if let Err(e) = log_file.write_all(&output.stderr) {
                        error!("写入 scrcpy jar 错误日志失败: {:?}", e);
                    }
                }
                info!("客户端 {} 的 scrcpy jar 任务完成，退出码: {:?}", client_socket_id_jar, output.status);
            }
            Err(e) => {
                error!("启动 scrcpy jar 失败: {:?}", e);
                let _ = writeln!(&mut log_file, "启动失败: {:?}", e);
            }
        }

        // 清理临时文件
        let _ = std::fs::remove_file(&temp_jar_path);
    });

    // 等待 jar 文件推送和 scrcpy-server 启动
    // 推送 jar 文件可能需要一些时间，增加等待时间
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // 创建 channel 的克隆，用于在任务间传递
    let scrcpy_data_tx_for_read = scrcpy_data_tx.clone();
    let state_for_read = state.clone();
    let io_for_read = io.clone();

    // 任务 2: TCP socket 读取数据
    let socket_addr_1 = socket_addr.clone();
    let client_socket_id_1 = client_socket_id.clone();
    let socket_read_handle = tokio::spawn(async move {
        let stream = match TcpStream::connect(&socket_addr_1).await {
            Ok(s) => s,
            Err(e) => {
                error!("客户端 {} 的 socket read 连接失败: {:?}", client_socket_id_1, e);
                return;
            }
        };

        info!("客户端 {} 的 socket read 连接成功", client_socket_id_1);

        let mut read = stream;

        // 状态机处理前两个特殊消息
        let mut state = ReadState::ReadAck;
        let mut ack_buf = [0u8; 1];
        let mut meta_buf = [0u8; 64];

        loop {
            match state {
                ReadState::ReadAck => {
                    // 读取 1 字节确认消息
                    match read.read_exact(&mut ack_buf).await {
                        Ok(_) => {
                            info!("收到 scrcpy socket 确认字节: {}", ack_buf[0]);
                            if ack_buf[0] != 0 {
                                warn!("意外的确认字节: {}", ack_buf[0]);
                            }
                            state = ReadState::ReadMeta;
                        }
                        Err(e) => {
                            error!("读取确认字节失败: {:?}", e);
                            break;
                        }
                    }
                }
                ReadState::ReadMeta => {
                    // 读取 64 字节设备元数据
                    match read.read_exact(&mut meta_buf).await {
                        Ok(_) => {
                            // 从元数据解析设备名称
                            let device_name = String::from_utf8_lossy(&meta_buf)
                                .trim_end_matches('\0')
                                .to_string();

                            info!("收到设备元数据: {} ({} 字节)", device_name, meta_buf.len());

                            // 通过 scrcpy_device_meta 事件发送设备元数据
                            if let Err(e) = io_for_read.emit("scrcpy_device_meta", &device_name).await {
                                error!("发送设备元数据失败: {:?}", e);
                            }

                            // 存储到会话状态
                            {
                                let mut session = state_for_read.session.lock().await;
                                session.device_meta = Some(device_name.clone());
                            }

                            state = ReadState::ReadData;
                        }
                        Err(e) => {
                            error!("读取设备元数据失败: {:?}", e);
                            break;
                        }
                    }
                }
                ReadState::ReadData => {
                    // 正常数据转发
                    let mut buf = vec![0; 8192];
                    match read.read(&mut buf).await {
                        Ok(0) => {
                            warn!("客户端 {} 的 socket read 连接关闭", client_socket_id_1);
                            break;
                        }
                        Ok(n) => {
                            let data = buf[..n].to_vec();
                            if let Err(e) = scrcpy_data_tx_for_read.send(data) {
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
            }
        }
    });

    // 等待第一个 socket 建立
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // 任务 3: TCP socket 写入控制数据
    let client_socket_id_2 = client_socket_id.clone();
    let socket_write_handle = tokio::spawn(async move {
        let stream = match TcpStream::connect(&socket_addr).await {
            Ok(s) => s,
            Err(e) => {
                error!("客户端 {} 的 socket write 连接失败: {:?}", client_socket_id_2, e);
                return;
            }
        };

        info!("客户端 {} 的 socket write 连接成功", client_socket_id_2);

        let write = stream.into_split().1;
        let mut write_guard = scrcpy_control_write.lock().await;
        *write_guard = Some(write);
        info!("客户端 {} 的 control socket 就绪", client_socket_id_2);
        drop(write_guard);

        // 保持任务活跃
        tokio::time::sleep(tokio::time::Duration::from_secs(u64::MAX)).await;
    });

    // 任务 4: Socket.IO 广播
    let client_socket_id_3 = client_socket_id.clone();
    let broadcast_handle = tokio::spawn(async move {
        info!("客户端 {} 的广播任务启动", client_socket_id_3);

        while let Some(data) = scrcpy_data_rx.recv().await {
            use base64::prelude::*;
            let base64_data = BASE64_STANDARD.encode(&data);

            if let Err(e) = io.emit("scrcpy", &base64_data).await {
                error!("广播 scrcpy 数据失败: {:?}", e);
            }
        }

        info!("客户端 {} 的广播任务结束", client_socket_id_3);
    });

    // 存储句柄到会话状态
    let mut session = state.session.lock().await;
    session.scrcpy_jar_handle = Some(scrcpy_jar_handle);
    session.socket_read_handle = Some(socket_read_handle);
    session.socket_write_handle = Some(socket_write_handle);
    session.broadcast_handle = Some(broadcast_handle);
    // 客户端已在 handle_client_connect 中添加到集合
    // 只需验证客户端在集合中（用于调试）
    assert!(session.connected_clients.contains(&client_socket_id));

    info!("Scrcpy 会话已启动，服务于 {} 个客户端", session.connected_clients.len());
}
