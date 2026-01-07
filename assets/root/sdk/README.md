# Scrcpy SDK 使用文档

这是一个用于 Web 端连接和控制 Android 设备的 SDK，基于 scrcpy 协议实现。

## 📦 模块说明

SDK 包含三个主要模块：

- **ScrcpySocket** - Socket.IO 连接管理
- **VideoDecoder** - H.264 视频解码
- **ScrcpyClient** - 完整客户端（推荐使用）

## 🚀 快速开始

### 1. 基本使用

```javascript
import { ScrcpyClient } from './sdk/index.js';

// 创建客户端实例
const client = new ScrcpyClient({
    canvas: document.getElementById('canvas'),
    onConnected: () => {
        console.log('已连接到设备');
    },
    onDisconnected: (reason) => {
        console.log('断开连接:', reason);
    },
    onError: (error) => {
        console.error('错误:', error);
    },
    onFrame: (frameData) => {
        console.log('新帧:', frameData);
    },
    onLog: (message, level) => {
        console.log(`[${level}] ${message}`);
    }
});

// 连接到设备
try {
    await client.connect('device_serial', 3000);
    console.log('连接成功！');
} catch (error) {
    console.error('连接失败:', error);
}
```

### 2. 触摸控制

```javascript
// 发送触摸事件（使用设备坐标）
client.sendTouch(client.Constants.ACTION_DOWN, 540, 960);
client.sendTouch(client.Constants.ACTION_UP, 540, 960);

// 发送触摸事件（使用 Canvas 坐标）
canvas.addEventListener('mousedown', (e) => {
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    client.sendTouchByCanvasCoords(client.Constants.ACTION_DOWN, x, y);
});

canvas.addEventListener('mouseup', (e) => {
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    client.sendTouchByCanvasCoords(client.Constants.ACTION_UP, x, y);
});
```

### 3. 键盘输入

```javascript
// 发送按键代码
client.sendKey(client.Constants.KEYCODE_ENTER);

// 发送文本
client.sendText('Hello World');

// 处理键盘事件
canvas.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
        client.sendKeyByName('Enter');
    } else if (e.key.length === 1) {
        client.sendText(e.key);
    }
});
```

### 4. 电源控制

```javascript
// 解锁屏幕
client.setPower(true);

// 锁屏
client.setPower(false);
```

### 5. 多设备支持

可以在同一个页面创建多个客户端实例，连接到不同的设备：

```javascript
// 客户端 1
const client1 = new ScrcpyClient({
    canvas: document.getElementById('canvas1'),
    onLog: (msg, level) => logTo('log1', msg, level)
});
await client1.connect('device_serial_1', 3000);

// 客户端 2
const client2 = new ScrcpyClient({
    canvas: document.getElementById('canvas2'),
    onLog: (msg, level) => logTo('log2', msg, level)
});
await client2.connect('device_serial_2', 3001);
```

## 📖 API 参考

### ScrcpyClient

#### 构造函数

```javascript
new ScrcpyClient(config)
```

**参数：**
- `config.canvas` (required) - 用于渲染视频的 Canvas 元素
- `config.onConnected` (optional) - 连接成功回调
- `config.onDisconnected` (optional) - 断开连接回调
- `config.onError` (optional) - 错误回调
- `config.onFrame` (optional) - 帧解码回调
- `config.onLog` (optional) - 日志回调
- `config.keyMap` (optional) - 自定义按键映射
- `config.pointerId` (optional) - 触摸点 ID（默认: 0n）

#### 方法

##### connect(deviceSerial, socketPort)

连接到设备。

**参数：**
- `deviceSerial` (string) - 设备序列号
- `socketPort` (number) - Socket.IO 端口

**返回：** Promise<void>

##### disconnect()

断开连接。

##### sendTouch(action, x, y, pressure?)

发送触摸事件（设备坐标）。

**参数：**
- `action` (number) - 动作类型（使用 Constants）
- `x` (number) - 设备 X 坐标
- `y` (number) - 设备 Y 坐标
- `pressure` (number, optional) - 压力值 0.0-1.0

**返回：** boolean

##### sendTouchByCanvasCoords(action, canvasX, canvasY)

发送触摸事件（Canvas 坐标）。

**参数：**
- `action` (number) - 动作类型
- `canvasX` (number) - Canvas X 坐标
- `canvasY` (number) - Canvas Y 坐标

**返回：** boolean

##### sendKey(keyCode)

发送按键事件（按键代码）。

**参数：**
- `keyCode` (number) - Android KEYCODE_*

**返回：** boolean

##### sendKeyByName(keyName)

发送按键事件（按键名称）。

**参数：**
- `keyName` (string) - 按键名称（如 'Enter', 'Backspace'）

**返回：** boolean

##### sendText(text)

发送文本输入。

**参数：**
- `text` (string) - 要输入的文本

**返回：** boolean

##### setPower(on)

设置屏幕电源状态。

**参数：**
- `on` (boolean) - true=亮屏/解锁, false=息屏/锁屏

**返回：** boolean

##### isConnected()

获取连接状态。

**返回：** boolean

##### getScreenSize()

获取屏幕尺寸。

**返回：** { width: number, height: number }

##### getStats()

获取解码器统计信息。

**返回：** Object | null

##### on(event, callback)

注册事件监听器。

**参数：**
- `event` (string) - 事件名称：'connected', 'disconnected', 'error', 'frame'
- `callback` (Function) - 回调函数

##### off(event, callback)

移除事件监听器。

##### destroy()

销毁客户端，释放资源。

### Constants

通过 `client.Constants` 访问：

- `ACTION_DOWN` (0) - 触摸按下
- `ACTION_UP` (1) - 触摸抬起
- `ACTION_MOVE` (2) - 触摸移动
- `ACTION_CANCEL` (3) - 触摸取消
- `KEYCODE_ENTER` (0x42) - 回车键
- `KEYCODE_DEL` (0x43) - 删除键
- `KEYCODE_TAB` (0x3d) - Tab 键
- `KEYCODE_ESCAPE` (0x6f) - Esc 键
- 等等...

## 🔧 高级用法

### 使用 ScrcpySocket 单独

如果你只需要 Socket 连接功能：

```javascript
import { ScrcpySocket } from './sdk/index.js';

const socket = new ScrcpySocket('http://127.0.0.1:3000', {
    onConnect: () => console.log('Connected'),
    onVideoData: (data) => console.log('Video data:', data)
});

await socket.connect();
socket.sendControl(new Uint8Array([...]));
socket.disconnect();
```

### 使用 VideoDecoder 单独

如果你只需要视频解码功能：

```javascript
import { VideoDecoder } from './sdk/index.js';

const decoder = new VideoDecoder(canvas, {
    onFrame: (frameData) => console.log('Frame:', frameData),
    onError: (error) => console.error('Error:', error)
});

await decoder.init();
decoder.decode(rawData);
decoder.destroy();
```

## 💡 最佳实践

1. **始终清理资源**：使用完客户端后调用 `destroy()` 方法
2. **处理错误**：始终监听 `error` 事件
3. **检查连接状态**：发送控制命令前检查 `isConnected()`
4. **使用 Canvas 坐标**：推荐使用 `sendTouchByCanvasCoords` 而不是手动转换坐标

## 📝 许可证

MIT License
