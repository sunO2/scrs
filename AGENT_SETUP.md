# Agent Socket.IO 服务器设置说明

## 概述

本项目已成功集成 Agent Socket.IO 服务器，支持通过前端页面对话区域与 AI Agent 进行交互。

## 架构

### 服务器端口
- **API 服务器**: 端口 3000 (Axum HTTP)
- **Agent Socket.IO 服务器**: 端口 4000 (Socket.IO)

### 前端集成
- 自动连接到 `localhost:4000` 的 Agent Socket.IO 服务器
- 支持通过对话区域发送任务指令
- 实时显示 Agent 执行状态和结果

## 设置 AutoGLM API Key

### 1. 获取 API Key
访问 [智谱AI开放平台](https://open.bigmodel.cn/) 注册并获取 API Key

### 2. 设置环境变量

**Linux/macOS:**
```bash
export AUTOGLM_API_KEY=your_api_key_here
```

**Windows (PowerShell):**
```powershell
$env:AUTOGLM_API_KEY="your_api_key_here"
```

**Windows (CMD):**
```cmd
set AUTOGLM_API_KEY=your_api_key_here
```

### 3. 永久设置（可选）

创建 `.env` 文件在项目根目录：
```bash
AUTOGLM_API_KEY=your_api_key_here
```

## 使用方式

### 1. 启动服务器
```bash
cargo run
```

### 2. 访问前端
打开浏览器访问: `http://localhost:3000`

### 3. 连接设备
- 在左侧面板选择设备并点击"连接设备"
- 等待设备屏幕显示

### 4. 使用 Agent
- 在右侧对话区域输入任务指令，例如：
  - "打开微信"
  - "发送消息给张三"
  - "截图"
  - "返回主页"
- 按回车键或点击发送按钮
- Agent 会分析屏幕并执行相应操作

## Socket.IO 事件

### 客户端发送事件

#### agent/start
启动 Agent 任务
```javascript
socket.emit('agent/start', {
  device_serial: "device_serial",
  task: "打开微信"
});
```

#### agent/devices
获取设备列表
```javascript
socket.emit('agent/devices', {});
```

#### agent/stop
停止 Agent
```javascript
socket.emit('agent/stop', {
  device_serial: "device_serial"
});
```

### 服务器响应事件

#### agent/start/response
Agent 启动响应
```javascript
{
  "success": true,
  "agent_id": "uuid",
  "device_serial": "device_serial",
  "task": "任务描述"
}
```

#### agent/devices/response
设备列表响应
```javascript
{
  "success": true,
  "devices": [
    {
      "serial": "device_serial",
      "status": "connected"
    }
  ]
}
```

#### agent/stop/response
Agent 停止响应
```javascript
{
  "success": true,
  "device_serial": "device_serial"
}
```

## 故障排查

### Agent 连接失败
1. 检查 Agent Socket.IO 服务器是否正在运行（端口 4000）
2. 查看浏览器控制台是否有错误信息
3. 确认防火墙设置

### LLM 请求失败
1. **检查 API Key**: 确认 `AUTOGLM_API_KEY` 环境变量已正确设置
2. **网络连接**: 确认可以访问 `https://open.bigmodel.cn`
3. **API 配额**: 确认 API Key 有足够的配额
4. **超时设置**: 如果网络较慢，可以增加 `timeout` 值（已设置为 60 秒）

### 查看详细日志
服务器会输出详细的错误信息，包括：
- AutoGLM 客户端创建信息
- API 端点和配置
- 请求和响应详情
- 错误原因和解决建议

## 开发模式

### 添加新的 Agent 操作
在 `src/agent/actions/` 目录下添加新的 Action 实现。

### 修改 LLM 提示词
在 `src/agent/llm/autoglm_client.rs` 中修改系统提示。

### 自定义 Socket.IO 事件
在 `src/agent/socket_server.rs` 的 `register_agent_handlers_with_pool` 函数中添加新的事件处理器。
