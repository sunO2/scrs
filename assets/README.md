# Scrcpy Web Viewer

这是一个基于 Web 的 Scrcpy 客户端，可以通过浏览器远程查看和控制 Android 设备。

## 功能特性

- ✅ 实时视频流显示
- ✅ 触摸/点击事件传输
- ✅ Socket.IO 双向通信
- ✅ 响应式设计
- ⏳ H.264 视频解码（需要集成）

## 文件说明

```
assets/
├── index.html              # 主页面
├── socketio-client.js      # Socket.IO 客户端逻辑
├── socketio.js            # Node.js 测试脚本
└── README.md              # 本文件
```

## 使用方法

### 1. 启动服务端

```bash
cargo run
```

### 2. 连接设备

使用 API 或工具连接设备：

```bash
curl -X POST http://localhost:3000/connect \
  -H "Content-Type: application/json" \
  -d '{"serial": "your_device_serial"}'
```

### 3. 打开 Web 页面

在浏览器中打开：
```
http://localhost:3000/assets/index.html
```

### 4. 连接 Socket.IO

输入 Socket.IO URL（从 API 响应中获取），点击"连接"按钮。

## H.264 视频解码集成

由于 scrcpy 使用 H.264 编码视频，需要在浏览器中集成 H.264 解码器。有以下几种方案：

### 方案 1: FFmpeg.wasm (推荐)

使用 FFmpeg 的 WebAssembly 版本：

```html
<script src="https://unpkg.com/@ffmpeg/ffmpeg@0.12.7/dist/umd/ffmpeg.min.js"></script>
```

```javascript
const ffmpeg = new FFmpeg();
await ffmpeg.load();

// 解码 H.264 数据
await ffmpeg.writeFile('input.h264', new Uint8Array(h264Data));
await ffmpeg.exec(['-i', 'input.h264', '-f', 'rawvideo', 'output.yuv']);
const data = await ffmpeg.readFile('output.yuv');
```

### 方案 2: Broadway.js

轻量级的 H.264 解码器：

```html
<script src="https://cdn.jsdelivr.net/npm/broadway@4.3.1/dist/broadway.min.js"></script>
```

### 方案 3: JSMpeg

支持 MPEG-TS 格式的解码器：

```html
<script src="https://cdn.jsdelivr.net/npm/jsmpeg@0.2.0/jsmpeg.min.js"></script>
```

### 方案 4: 软件解码（临时方案）

修改 scrcpy server 参数，使用更简单的编码格式：

```bash
# 使用较低质量的编码
CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server 3.3.4 video_bit_rate=2000000
```

### 快速集成示例

在 `socketio-client.js` 中替换 `H264Decoder` 类：

```javascript
class H264Decoder {
    constructor() {
        this.decoder = new Worker('worker.js');
    }

    async init(callback) {
        this.decoder.onmessage = (e) => {
            if (e.data.frame) {
                callback(e.data.frame);
            }
        };
    }

    decode(data) {
        this.decoder.postMessage({ type: 'decode', data: data }, [data.buffer]);
    }
}
```

创建 `worker.js`：

```javascript
// 使用 FFmpeg.wasm 或其他解码器
importScripts('https://unpkg.com/@ffmpeg/ffmpeg@0.12.7/dist/umd/ffmpeg.min.js');

let ffmpeg = null;

async function init() {
    ffmpeg = new FFmpeg();
    await ffmpeg.load();
}

self.onmessage = async function(e) {
    if (!ffmpeg) await init();

    // 解码 H.264
    // ... 解码逻辑

    self.postMessage({ type: 'frame', frame: decodedFrame });
};
```

## API 端点

### 连接设备

```bash
POST /connect
Content-Type: application/json

{
  "serial": "device_serial_number"
}

# 响应
{
  "success": true,
  "message": "设备 xxx 连接成功",
  "data": {
    "serial": "device_serial_number",
    "socketio_port": 60482
  }
}
```

### 断开设备

```bash
POST /disconnect
Content-Type: application/json

{
  "serial": "device_serial_number"
}
```

### 获取设备列表

```bash
GET /devices
```

### 获取设备状态

```bash
GET /device/{serial}/status
```

## Socket.IO 事件

### 客户端 → 服务端

| 事件 | 数据类型 | 说明 |
|------|----------|------|
| `scrcpy_ctl` | Uint8Array (32 bytes) | 触摸控制事件 |
| `test` | JSON | 测试消息 |

### 服务端 → 客户端

| 事件 | 数据类型 | 说明 |
|------|----------|------|
| `scrcpy` | String (base64) | 视频数据 (H.264) |
| `test_response` | JSON | 测试响应 |

## 触摸事件格式

```javascript
// 32 bytes 二进制数据
// [类型(1B)] [动作(1B)] [指针ID(8B)] [X(4B)] [Y(4B)]
// [屏幕宽(2B)] [屏幕高(2B)] [压力(2B)] [动作按钮(4B)] [按钮(4B)]

const message = buildTouchEvent(
    ACTION_DOWN,    // 动作: 0=DOWN, 1=UP, 2=MOVE
    0n,             // 指针 ID (BigInt)
    160,            // X 坐标
    260,            // Y 坐标
    1.0,            // 压力 (0.0 - 1.0)
    1,              // 动作按钮
    1               // 按钮状态
);
```

## 坐标转换

Canvas 坐标 → 设备坐标：

```javascript
function canvasToDeviceCoords(canvasX, canvasY) {
    const scaleX = screenWidth / canvas.width;
    const scaleY = screenHeight / canvas.height;
    return {
        x: Math.floor(canvasX * scaleX),
        y: Math.floor(canvasY * scaleY)
    };
}
```

## 故障排查

### 连接失败

1. 检查 Socket.IO URL 是否正确
2. 确认设备已连接
3. 查看浏览器控制台错误信息

### 视频不显示

1. 确认 H.264 解码器已正确集成
2. 检查数据传输是否正常（控制台日志）
3. 尝试降低视频质量参数

### 触摸无响应

1. 检查控制 socket 是否连接成功
2. 确认坐标转换正确
3. 查看服务端日志

## 技术栈

- **服务端**: Rust + Axum + Socket.IO + Tokio
- **客户端**: HTML5 + Canvas + Socket.IO Client
- **视频编码**: H.264 (scrcpy)
- **视频解码**: FFmpeg.wasm / Broadway.js / JSMpeg

## 参考资料

- [Scrcpy GitHub](https://github.com/Genymobile/scrcpy)
- [Socket.IO Documentation](https://socket.io/docs/)
- [FFmpeg.wasm](https://ffmpegwasm.netlify.app/)
- [Canvas API](https://developer.mozilla.org/en-US/docs/Web/API/Canvas_API)

## 许可证

MIT
