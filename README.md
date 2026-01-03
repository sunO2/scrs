# Scrcpy-RS Context 实现

这是一个 Rust 封装的 Context 系统，用于在 API 接口中管理 ScrcpyServer 和 ADBServer。

## 功能特性

- ✅ 线程安全的 Context 实现（使用 RwLock）
- ✅ 设备管理（连接、断开、查询状态）
- ✅ RESTful API 接口
- ✅ 完整的日志记录
- ✅ 统一的错误处理
- ✅ 基于 Axum 框架

## 项目结构

```
src/
├── api/
│   ├── api.rs          # API 服务器和路由处理
│   └── mod.rs
├── context/
│   ├── context.rs      # Context 和 ScrcpyServer 实现
│   └── mod.rs
├── error.rs           # 错误类型定义
└── main.rs            # 程序入口
```

## 核心组件

### 1. ScrcpyServer

负责管理设备连接和状态：

```rust
pub struct ScrcpyServer {
    devices: HashMap<String, String>, // 设备序列号 -> 设备状态
}
```

**主要方法：**
- `new()` - 创建新实例
- `get_devices()` - 获取所有设备列表
- `is_device_connected(serial)` - 检查设备是否连接
- `add_device(serial, status)` - 添加设备
- `remove_device(serial)` - 移除设备
- `get_device_status(serial)` - 获取设备状态

### 2. Context

线程安全的 Context，封装 ScrcpyServer 和 ADBServer：

```rust
pub struct Context {
    scrcpy: RwLock<ScrcpyServer>,
    adb_server: ADBServer,
}
```

### 3. IContext Trait

定义访问服务器实例的接口：

```rust
pub trait IContext: Send + Sync {
    fn get_scrcpy(&self) -> &RwLock<ScrcpyServer>;
    fn get_adb_server(&self) -> &ADBServer;
}
```

## API 接口

### 获取设备列表

```
GET /devices
```

响应示例：
```json
{
  "devices": [
    {
      "serial": "emulator-5554",
      "status": "connected"
    }
  ],
  "count": 1
}
```

### 连接设备

```
POST /connect
Content-Type: application/json

{
  "serial": "emulator-5554"
}
```

响应示例：
```json
{
  "success": true,
  "message": "设备 emulator-5554 连接成功",
  "data": "emulator-5554"
}
```

### 断开设备

```
POST /disconnect
Content-Type: application/json

{
  "serial": "emulator-5554"
}
```

### 获取设备状态

```
GET /device/{serial}/status
```

响应示例：
```json
{
  "success": true,
  "message": "获取设备状态成功",
  "data": {
    "serial": "emulator-5554",
    "status": "connected"
  }
}
```

### 测试端点

```
GET /hello
```

## 使用方法

### 1. 启动服务器

```bash
cargo run
```

服务器将在 `http://0.0.0.0:3000` 启动。

### 2. 在 API 处理器中使用 Context

```rust
use crate::context::context::IContext;

async fn get_devices(
    State(ctx): State<Arc<dyn IContext + Sync + Send>>,
) -> Json<DevicesResponse> {
    let scrcpy = ctx.get_scrcpy().read().unwrap();
    let devices = scrcpy.get_devices();
    // ... 处理逻辑
}
```

### 3. 创建自定义 Context 实现

```rust
use crate::context::context::{IContext, Context};

// 使用默认 Context
let ctx = Arc::new(Context::new());

// 或实现自定义 IContext
struct MyContext {
    scrcpy: RwLock<ScrcpyServer>,
    adb_server: ADBServer,
    // 添加自定义字段
}

impl IContext for MyContext {
    fn get_scrcpy(&self) -> &RwLock<ScrcpyServer> {
        &self.scrcpy
    }

    fn get_adb_server(&self) -> &ADBServer {
        &self.adb_server
    }
}
```

## 日志记录

系统使用 `tracing` 进行日志记录，支持不同级别：

- `debug` - 详细的调试信息
- `info` - 一般信息
- `warn` - 警告信息
- `error` - 错误信息

可以通过环境变量控制日志级别：

```bash
RUST_LOG=debug cargo run
```

## 错误处理

系统定义了统一的错误类型 `AppError`：

```rust
pub enum AppError {
    DeviceNotFound(String),
    DeviceAlreadyConnected(String),
    DeviceNotConnected(String),
    AdbError(String),
    ScrcpyError(String),
    IoError(std::io::Error),
    JsonError(serde_json::Error),
    Unknown(String),
}
```

## 开发计划

- [ ] 集成实际的 Scrcpy 屏幕镜像功能
- [ ] 添加 WebSocket 支持用于实时数据传输
- [ ] 实现设备信息同步
- [ ] 添加认证和授权
- [ ] 支持多设备并发
- [ ] 添加配置文件支持

## 依赖项

- `axum` - Web 框架
- `tokio` - 异步运行时
- `serde` - 序列化/反序列化
- `tracing` - 日志记录
- `adb_client` - ADB 客户端
- `thiserror` - 错误处理

## 许可证

MIT License
