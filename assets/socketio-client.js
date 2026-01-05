/**
 * Scrcpy Web Viewer - Socket.IO Client
 * 实现视频流接收、解码和触摸事件发送
 */

// Socket.IO 客户端
let socket = null;

// Canvas 相关
const canvas = document.getElementById('canvas');
const ctx = canvas.getContext('2d');
const phoneFrame = document.getElementById('phoneFrame');

// 统计信息
let frameCount = 0;
let lastFrameTime = Date.now();
let fps = 0;

// Scrcpy 控制消息类型
const SCRCPY_MSG_TYPE_INJECT_TOUCH_EVENT = 2;

// Android MotionEvent 动作类型
const ACTION_DOWN = 0;
const ACTION_UP = 1;
const ACTION_MOVE = 2;
const ACTION_CANCEL = 3;
const ACTION_OUTSIDE = 4;

// Android MotionEvent 按钮常量
const BUTTON_PRIMARY = 1;

// 触摸点 ID
const POINTER_ID = 0n;

// 屏幕尺寸
let screenWidth = 1080;
let screenHeight = 1920;

// H.264 解码器 (使用 WebCodecs API 或备用方案)
class H264Decoder {
    constructor() {
        this.frameCallback = null;
        this.buffer = [];      // 累积不完整的数据包
        this.bufferSize = 0;
        this.stats = {
            totalBytes: 0,
            totalPackets: 0,
            spsCount: 0,
            ppsCount: 0,
            idrCount: 0,
            pFrameCount: 0,
            decodedFrames: 0,
            droppedFrames: 0
        };

        // WebCodecs 解码器
        this.videoDecoder = null;
        this.useWebCodecs = false;
        this.pendingFrames = 0;
        this.maxPendingFrames = 10; // 最大待处理帧数

        // 配置数据 (SPS/PPS)
        this.decoderConfig = null;
        this.spsData = null;
        this.ppsData = null;

        // Scrcpy 数据格式解析
        // 每个数据包包含: [4字节长度] [H.264数据]
    }

    async init(callback) {
        this.frameCallback = callback;

        // 检查是否支持 WebCodecs API
        if (typeof VideoDecoder !== 'undefined') {
            this.useWebCodecs = true;
            try {
                await this.initWebCodecs();
                console.log('H.264 解码器初始化完成 (使用 WebCodecs API)');
            } catch (e) {
                console.error('初始化 WebCodecs 解码器失败:', e);
                this.useWebCodecs = false;
            }
        }

        if (!this.useWebCodecs) {
            console.warn('WebCodecs API 不可用，使用数据解析模式');
            console.warn('视频流将显示数据统计信息而不是实际画面');
            console.warn('建议使用 Chrome 94+ 或 Edge 94+ 以获得硬件加速解码');
        }
    }

    async initWebCodecs() {
        // 创建 VideoDecoder 实例
        this.videoDecoder = new VideoDecoder({
            output: (frame) => this.handleDecodedFrame(frame),
            error: (error) => this.handleDecodeError(error)
        });

        console.log('WebCodecs VideoDecoder 已创建，等待 SPS/PPS 配置...');
    }

    handleDecodedFrame(frame) {
        this.stats.decodedFrames++;
        this.pendingFrames--;

        try {
            // 将 VideoFrame 转换为 ImageBitmap
            frame.clone().then(async (clonedFrame) => {
                try {
                    const bitmap = await createImageBitmap(clonedFrame);

                    // 绘制到离屏 canvas 获取像素数据
                    const offscreenCanvas = new OffscreenCanvas(
                        clonedFrame.codedWidth,
                        clonedFrame.codedHeight
                    );
                    const offscreenCtx = offscreenCanvas.getContext('2d');
                    offscreenCtx.drawImage(bitmap, 0, 0);

                    // 获取 ImageData
                    const imageData = offscreenCtx.getImageData(
                        0,
                        0,
                        clonedFrame.codedWidth,
                        clonedFrame.codedHeight
                    );

                    // 回调
                    this.frameCallback({
                        type: 'decoded_frame',
                        buffer: imageData.data.buffer,
                        width: clonedFrame.codedWidth,
                        height: clonedFrame.codedHeight,
                        stats: { ...this.stats }
                    });

                    bitmap.close();
                    clonedFrame.close();
                } catch (e) {
                    console.error('处理解码帧失败:', e);
                }
            });
        } catch (e) {
            console.error('克隆帧失败:', e);
        }

        frame.close();
    }

    handleDecodeError(error) {
        console.error('WebCodecs 解码错误:', error);
        this.stats.droppedFrames++;

        // 如果解码器状态异常，尝试重新配置
        if (this.videoDecoder.state === 'closed') {
            console.warn('解码器已关闭，尝试重新初始化...');
            this.initWebCodecs();
        }
    }

    async configureDecoder(sps, pps) {
        if (!this.useWebCodecs || !this.videoDecoder) return;

        try {
            // 解析 SPS 获取视频尺寸
            const spsData = this.parseSPS(sps);
            const width = spsData.width;
            const height = spsData.height;

            console.log(`配置解码器: ${width}x${height}`);

            // 构建 codec description (AVCC格式: [length][NALU]...)
            const codecDescription = this.buildAVCCCodecDescription(sps, pps);

            // 配置解码器
            this.decoderConfig = {
                codec: 'avc1.42E01E', // H.264 Baseline Profile Level 4.0
                codedWidth: width,
                codedHeight: height,
                description: codecDescription
            };

            await this.videoDecoder.configure(this.decoderConfig);

            if (this.videoDecoder.state === 'configured') {
                console.log('WebCodecs 解码器配置成功');
            }
        } catch (e) {
            console.error('配置解码器失败:', e);
            throw e;
        }
    }

    buildAVCCCodecDescription(sps, pps) {
        // AVCC 格式: [长度(4字节)][NALU数据]...
        const buffer = new Uint8Array(4 + sps.length + 4 + pps.length);
        let offset = 0;

        // SPS
        buffer[offset++] = (sps.length >> 24) & 0xFF;
        buffer[offset++] = (sps.length >> 16) & 0xFF;
        buffer[offset++] = (sps.length >> 8) & 0xFF;
        buffer[offset++] = sps.length & 0xFF;
        buffer.set(sps, offset);
        offset += sps.length;

        // PPS
        buffer[offset++] = (pps.length >> 24) & 0xFF;
        buffer[offset++] = (pps.length >> 16) & 0xFF;
        buffer[offset++] = (pps.length >> 8) & 0xFF;
        buffer[offset++] = pps.length & 0xFF;
        buffer.set(pps, offset);

        return buffer;
    }

    parseSPS(sps) {
        // 简化的 SPS 解析
        // 实际应用中应完整解析 SPS，这里假设 1080x1920
        return {
            width: 1080,
            height: 1920
        };
    }

    async decodeWithWebCodecs(nalData, nalType) {
        if (!this.useWebCodecs || !this.videoDecoder) return false;

        // 如果解码器未配置，等待 SPS 和 PPS
        if (this.videoDecoder.state !== 'configured') {
            if (nalType === 7 && !this.spsData) {
                this.spsData = nalData;
                console.log('收到 SPS');
            } else if (nalType === 8 && !this.ppsData) {
                this.ppsData = nalData;
                console.log('收到 PPS');

                // 当同时有 SPS 和 PPS 时，配置解码器
                if (this.spsData) {
                    await this.configureDecoder(this.spsData, this.ppsData);
                }
            }
            return false;
        }

        // 检查待处理帧数
        if (this.pendingFrames >= this.maxPendingFrames) {
            this.stats.droppedFrames++;
            return false;
        }

        // 只解码实际的视频帧 (IDR 或 P 帧)
        if (nalType !== 5 && nalType !== 1) {
            return false;
        }

        try {
            // 构造 EncodedVideoChunk
            // 需要添加 AVCC 格式的长度前缀
            const chunkData = new Uint8Array(4 + nalData.length);
            new DataView(chunkData.buffer).setUint32(0, nalData.length, false); // big-endian
            chunkData.set(nalData, 4);

            const chunkType = (nalType === 5) ? 'key' : 'delta';
            const chunk = new EncodedVideoChunk({
                type: chunkType,
                timestamp: performance.now() * 1000, // 微秒
                data: chunkData
            });

            this.pendingFrames++;
            this.videoDecoder.decode(chunk);
            return true;
        } catch (e) {
            console.error('WebCodecs decode 失败:', e);
            this.pendingFrames--;
            return false;
        }
    }

    decode(data) {
        try {
            // 将新数据追加到缓冲区
            this.buffer.push(new Uint8Array(data));
            this.bufferSize += data.length;
            this.stats.totalBytes += data.length;

            // 合并缓冲区并解析数据包
            const combined = new Uint8Array(this.bufferSize);
            let offset = 0;
            for (const chunk of this.buffer) {
                combined.set(chunk, offset);
                offset += chunk.length;
            }

            let parseOffset = 0;
            let packetsProcessed = 0;

            while (parseOffset < combined.length) {
                // 读取数据包长度 (4 bytes, big-endian)
                if (parseOffset + 4 > combined.length) {
                    break;
                }

                const packetLength = (combined[parseOffset] << 24) |
                                    (combined[parseOffset + 1] << 16) |
                                    (combined[parseOffset + 2] << 8) |
                                    combined[parseOffset + 3];
                parseOffset += 4;

                // 检查是否有足够的数据
                if (parseOffset + packetLength > combined.length) {
                    // 数据包不完整，保留剩余数据等待下次
                    parseOffset -= 4;
                    break;
                }

                // 提取 H.264 数据
                const h264Data = combined.slice(parseOffset, parseOffset + packetLength);
                parseOffset += packetLength;
                packetsProcessed++;

                // 处理 H.264 NAL 单元
                this.processNALUnit(h264Data, packetsProcessed);
            }

            // 更新缓冲区：保留未解析的数据
            if (parseOffset < combined.length) {
                this.buffer = [combined.slice(parseOffset)];
                this.bufferSize = combined.length - parseOffset;
            } else {
                this.buffer = [];
                this.bufferSize = 0;
            }

            // 如果不使用 Broadway，发送统计信息更新
            if (!this.useBroadway && packetsProcessed > 0) {
                this.frameCallback({
                    type: 'h264_packet',
                    size: data.length,
                    packetsProcessed: packetsProcessed,
                    stats: { ...this.stats },
                    timestamp: Date.now()
                });
            }

        } catch (e) {
            console.error('解码错误:', e);
            // 清空缓冲区以恢复
            this.buffer = [];
            this.bufferSize = 0;
        }
    }

    processNALUnit(h264Data, packetNum) {
        // 检查是否有起始码
        let nalOffset = 0;
        let hasStartCode = false;

        if (h264Data.length >= 4 && h264Data[0] === 0 && h264Data[1] === 0 &&
            h264Data[2] === 0 && h264Data[3] === 1) {
            nalOffset = 4;
            hasStartCode = true;
        } else if (h264Data.length >= 3 && h264Data[0] === 0 && h264Data[1] === 0 &&
                   h264Data[2] === 1) {
            nalOffset = 3;
            hasStartCode = true;
        }

        if (nalOffset >= h264Data.length) {
            return;
        }

        const nalHeader = h264Data[nalOffset];
        const nalType = nalHeader & 0x1F;
        const nalTypeName = this.getNalTypeName(nalType);

        // 更新统计
        this.stats.totalPackets++;
        if (nalType === 7) this.stats.spsCount++;
        if (nalType === 8) this.stats.ppsCount++;
        if (nalType === 5) this.stats.idrCount++;
        if (nalType === 1) this.stats.pFrameCount++;

        // 只打印关键信息
        if (nalType === 7 || nalType === 5 || packetNum % 60 === 0) {
            console.log(`H.264: ${nalTypeName} (${nalType}), ${h264Data.length}字节, 包#${packetNum}`);
        }

        // 使用 WebCodecs 解码
        if (this.useWebCodecs) {
            const nalData = hasStartCode ? h264Data.slice(nalOffset) : h264Data;
            this.decodeWithWebCodecs(nalData, nalType);
            return;
        }

        // 如果不使用 WebCodecs，只做统计，发送更新
        if (!this.useWebCodecs && packetNum % 10 === 0) {
            this.frameCallback({
                type: 'h264_packet',
                size: h264Data.length,
                packetsProcessed: 1,
                stats: { ...this.stats },
                timestamp: Date.now()
            });
        }
    }

    getNalTypeName(nalType) {
        const types = {
            1: 'P帧',
            5: 'IDR关键帧',
            6: 'SEI',
            7: 'SPS',
            8: 'PPS',
            9: 'AUD',
            12: '填充数据',
            14: '前缀NALU'
        };
        return types[nalType] || `NAL(${nalType})`;
    }

    destroy() {
        // 关闭 WebCodecs 解码器
        if (this.videoDecoder) {
            if (this.videoDecoder.state === 'configured') {
                this.videoDecoder.close();
            }
            this.videoDecoder = null;
        }

        this.buffer = [];
        this.spsData = null;
        this.ppsData = null;
        this.decoderConfig = null;
        this.frameCallback = null;
        console.log('H.264 解码器已销毁');
        console.log('统计:', this.stats);
    }
}

// H.264 解码器实例
let decoder = null;

/**
 * 将浮点数转换为 u16 固定点数
 */
function floatToU16FixedPoint(value) {
    return Math.floor(value * 65535);
}

/**
 * 构建触摸事件消息
 */
function buildTouchEvent(action, pointerId, x, y, pressure = 1.0, actionButton = 0, buttons = 0) {
    // 消息格式 (总共 32 bytes)
    const buffer = new ArrayBuffer(32);
    const view = new DataView(buffer);

    let offset = 0;

    // 1. 类型: 1 byte
    view.setUint8(offset, SCRCPY_MSG_TYPE_INJECT_TOUCH_EVENT);
    offset += 1;

    // 2. 动作: 1 byte
    view.setUint8(offset, action);
    offset += 1;

    // 3. 指针 ID: 8 bytes (little-endian)
    view.setBigUint64(offset, pointerId, true);
    offset += 8;

    // 4. X 坐标: 4 bytes (little-endian)
    view.setInt32(offset, x, true);
    offset += 4;

    // 5. Y 坐标: 4 bytes (little-endian)
    view.setInt32(offset, y, true);
    offset += 4;

    // 6. 屏幕宽度: 2 bytes (big-endian)
    view.setUint16(offset, screenWidth, false);
    offset += 2;

    // 7. 屏幕高度: 2 bytes (big-endian)
    view.setUint16(offset, screenHeight, false);
    offset += 2;

    // 8. 压力: 2 bytes (big-endian)
    view.setUint16(offset, floatToU16FixedPoint(pressure), false);
    offset += 2;

    // 9. 动作按钮: 4 bytes (little-endian)
    view.setInt32(offset, actionButton, true);
    offset += 4;

    // 10. 按钮状态: 4 bytes (little-endian)
    view.setInt32(offset, buttons, true);

    return new Uint8Array(buffer);
}

/**
 * 将 canvas 坐标转换为设备坐标
 */
function canvasToDeviceCoords(canvasX, canvasY) {
    // 获取 canvas 的实际显示尺寸
    const rect = canvas.getBoundingClientRect();
    const displayWidth = rect.width;
    const displayHeight = rect.height;

    // 获取 canvas 的内部分辨率
    const internalWidth = canvas.width;
    const internalHeight = canvas.height;

    console.log(`坐标转换: 显示尺寸=${displayWidth}x${displayHeight}, 内部分辨率=${internalWidth}x${internalHeight}`);
    console.log(`点击坐标: canvasX=${canvasX}, canvasY=${canvasY}`);

    // 计算缩放比例 (显示尺寸 -> 内部分辨率)
    const scaleToInternal = internalWidth / displayWidth;

    // 转换到内部分辨率坐标
    const internalX = canvasX * scaleToInternal;
    const internalY = canvasY * scaleToInternal;

    // 转换到设备坐标
    const scaleX = screenWidth / internalWidth;
    const scaleY = screenHeight / internalHeight;

    const deviceX = Math.floor(internalX * scaleX);
    const deviceY = Math.floor(internalY * scaleY);

    console.log(`设备坐标: x=${deviceX}, y=${deviceY} (屏幕: ${screenWidth}x${screenHeight})`);

    return {
        x: deviceX,
        y: deviceY
    };
}

/**
 * 发送触摸事件
 */
function sendTouchEvent(action, x, y) {
    if (!socket || !socket.connected) {
        showError('Socket.IO 未连接');
        console.error('❌ Socket.IO 未连接，无法发送事件');
        return;
    }

    const message = buildTouchEvent(action, POINTER_ID, x, y, action === ACTION_UP ? 0.0 : 1.0, BUTTON_PRIMARY, action === ACTION_UP ? 0 : BUTTON_PRIMARY);

    // 调试：打印消息详细信息
    console.log('=== 发送触摸事件 ===');
    console.log(`动作: ${getActionName(action)} (${action})`);
    console.log(`坐标: x=${x}, y=${y}`);
    console.log(`屏幕尺寸: ${screenWidth}x${screenHeight}`);
    console.log(`Canvas尺寸: ${canvas.width}x${canvas.height}`);
    console.log(`消息长度: ${message.length} 字节`);
    console.log(`消息内容 (hex): ${bufferToHex(message)}`);

    // 发送二进制数据
    socket.emit('scrcpy_ctl', message, (ack) => {
        if (ack) {
            console.log('✓ 服务器确认收到事件:', ack);
        }
    });

    console.log(`✓ 事件已发送到服务器`);
    console.log(`Socket 连接状态: ${socket.connected ? '已连接' : '未连接'}`);
    console.log(`========================\n`);
}

/**
 * 获取动作名称
 */
function getActionName(action) {
    const actions = {
        0: 'ACTION_DOWN',
        1: 'ACTION_UP',
        2: 'ACTION_MOVE',
        3: 'ACTION_CANCEL',
        4: 'ACTION_OUTSIDE'
    };
    return actions[action] || `UNKNOWN(${action})`;
}

/**
 * 将 ArrayBuffer 转换为 hex 字符串 (用于调试)
 */
function bufferToHex(buffer) {
    const bytes = new Uint8Array(buffer);
    let hex = '';
    for (let i = 0; i < Math.min(bytes.length, 64); i++) {
        hex += bytes[i].toString(16).padStart(2, '0') + ' ';
        if ((i + 1) % 8 === 0) hex += ' ';
    }
    if (bytes.length > 64) hex += '...';
    return hex;
}

/**
 * 显示触摸指示器
 */
function showTouchIndicator(x, y) {
    const indicator = document.getElementById('touchIndicator');
    indicator.style.left = x + 'px';
    indicator.style.top = y + 'px';
    indicator.style.display = 'block';

    setTimeout(() => {
        indicator.style.display = 'none';
    }, 200);
}

/**
 * 显示错误消息
 */
function showError(message) {
    const errorDiv = document.getElementById('errorMessage');
    errorDiv.textContent = message;
    errorDiv.classList.add('show');

    setTimeout(() => {
        errorDiv.classList.remove('show');
    }, 3000);
}

/**
 * 更新连接状态
 */
function updateStatus(connected) {
    const indicator = document.querySelector('.status-indicator');
    const statusText = document.getElementById('statusText');
    const connectBtn = document.getElementById('connectBtn');
    const disconnectBtn = document.getElementById('disconnectBtn');
    const loadingHint = document.getElementById('loadingHint');

    if (connected) {
        indicator.classList.remove('disconnected');
        indicator.classList.add('connected');
        statusText.textContent = '已连接';
        connectBtn.style.display = 'none';
        disconnectBtn.style.display = 'inline-block';
        loadingHint.style.display = 'none';
    } else {
        indicator.classList.remove('connected');
        indicator.classList.add('disconnected');
        statusText.textContent = '未连接';
        connectBtn.style.display = 'inline-block';
        disconnectBtn.style.display = 'none';
        loadingHint.style.display = 'block';
    }
}

/**
 * 更新统计信息
 */
function updateStats() {
    const stats = document.getElementById('stats');
    stats.textContent = `FPS: ${fps} | 帧数: ${frameCount} | 尺寸: ${canvas.width}x${canvas.height}`;
}

/**
 * 连接到 Socket.IO 服务器
 */
function connect() {
    const url = document.getElementById('socketUrl').value;

    if (!url) {
        showError('请输入 Socket.IO URL');
        return;
    }

    // 清理旧连接
    if (socket) {
        socket.disconnect();
    }

    if (decoder) {
        decoder.destroy();
        decoder = null;
    }

    // 创建新连接
    socket = io(url, {
        path: '/socket.io/',
        transports: ['websocket', 'polling']
    });

    socket.on('connect', () => {
        console.log('Socket.IO 连接成功');
        updateStatus(true);

        // 发送测试消息
        socket.emit('test', { message: 'Hello from web client' });
    });

    socket.on('test_response', (data) => {
        console.log('收到测试响应:', data);
    });

    socket.on('scrcpy', (base64Data) => {
        // 接收到 scrcpy 视频数据 (base64 编码)
        handleVideoData(base64Data);
    });

    socket.on('scrcpy_ctl_ack', (data) => {
        console.log('✅ 收到服务器确认:', data);
    });

    socket.on('scrcpy_ctl_error', (data) => {
        console.error('❌ 服务器错误:', data);
        showError('触摸事件发送失败: ' + data.error);
    });

    socket.on('connect_error', (err) => {
        console.error('连接错误:', err);
        showError('连接失败: ' + err.message);
        updateStatus(false);
    });

    socket.on('disconnect', (reason) => {
        console.log('断开连接:', reason);
        updateStatus(false);
    });

    // 初始化解码器
    decoder = new H264Decoder();
    decoder.init((frameData) => {
        // 解码后的帧数据回调
        drawFrame(frameData);
    });
}

/**
 * 断开连接
 */
function disconnect() {
    if (socket) {
        socket.disconnect();
        socket = null;
    }

    if (decoder) {
        decoder.destroy();
        decoder = null;
    }

    updateStatus(false);

    // 清空画布
    ctx.fillStyle = '#000';
    ctx.fillRect(0, 0, canvas.width, canvas.height);
}

/**
 * 处理视频数据
 */
function handleVideoData(base64Data) {
    try {
        // 解码 base64 数据
        const binaryData = atob(base64Data);
        const uint8Array = new Uint8Array(binaryData.length);

        for (let i = 0; i < binaryData.length; i++) {
            uint8Array[i] = binaryData.charCodeAt(i);
        }

        // 这里是 H.264 编码的视频数据
        // 需要使用 H.264 解码器解码
        // 由于浏览器没有内置的 H.264 解码器，需要使用第三方库
        // 例如：ffmpeg.wasm, broadway.js, 或 jsmpeg

        // 临时方案：假设数据是简单的图像格式（用于测试）
        // 实际需要集成 H.264 解码器
        decoder.decode(uint8Array);

        // 更新统计
        frameCount++;
        const now = Date.now();
        if (now - lastFrameTime >= 1000) {
            fps = frameCount;
            frameCount = 0;
            lastFrameTime = now;
            updateStats();
        }
    } catch (e) {
        console.error('处理视频数据错误:', e);
    }
}

/**
 * 绘制帧到 canvas
 */
function drawFrame(frameData) {
    // 处理解码后的视频帧
    if (frameData && frameData.type === 'decoded_frame') {
        const { buffer, width, height, stats } = frameData;

        // 调整 canvas 尺寸
        if (canvas.width !== width || canvas.height !== height) {
            canvas.width = width;
            canvas.height = height;

            // 调整 phoneFrame 尺寸以适应视频
            const maxWidth = window.innerWidth - 40;
            const maxHeight = window.innerHeight - 200;
            const scale = Math.min(maxWidth / width, maxHeight / height, 1);

            phoneFrame.style.width = (width * scale) + 'px';
            phoneFrame.style.height = (height * scale) + 'px';
        }

        // 创建 ImageData 并绘制
        const imageData = new ImageData(new Uint8ClampedArray(buffer), width, height);
        ctx.putImageData(imageData, 0, 0);

        // 更新统计信息显示
        updateStatsDisplay(stats);

        return;
    }

    // 处理解码器错误
    if (frameData && frameData.type === 'error') {
        ctx.fillStyle = '#000';
        ctx.fillRect(0, 0, canvas.width, canvas.height);

        ctx.fillStyle = '#f44336';
        ctx.font = '20px monospace';
        ctx.textAlign = 'center';
        ctx.fillText('解码器加载失败', canvas.width / 2, canvas.height / 2 - 20);
        ctx.font = '16px monospace';
        ctx.fillText(frameData.message, canvas.width / 2, canvas.height / 2 + 20);
        return;
    }

    // 显示 H.264 数据接收状态（解码器未就绪时）
    if (frameData && frameData.type === 'h264_packet') {
        // 在 canvas 上显示接收状态
        ctx.fillStyle = '#000';
        ctx.fillRect(0, 0, canvas.width, canvas.height);

        // 绘制状态信息
        ctx.fillStyle = '#4caf50';
        ctx.font = '16px monospace';
        ctx.textAlign = 'left';

        const stats = frameData.stats || {};
        const hasWebCodecs = typeof VideoDecoder !== 'undefined';
        const waitingForSPS = stats.spsCount === 0 || stats.ppsCount === 0;

        const lines = [
            '📺 H.264 视频流接收状态',
            '',
            `✓ 总接收: ${(stats.totalBytes / 1024).toFixed(1)} KB`,
            `✓ 数据包: ${stats.totalPackets || 0} 个`,
            '',
            '视频帧类型:',
            `  • SPS (配置): ${stats.spsCount || 0}`,
            `  • PPS (参数): ${stats.ppsCount || 0}`,
            `  • IDR (关键帧): ${stats.idrCount || 0}`,
            `  • P 帧 (预测): ${stats.pFrameCount || 0}`,
            '',
        ];

        // 根据状态添加不同的提示信息
        if (!hasWebCodecs) {
            lines.push('⚠️  浏览器不支持 WebCodecs API');
            lines.push('   建议: Chrome 94+ 或 Edge 94+');
            lines.push('   当前显示: 数据统计信息');
        } else if (waitingForSPS) {
            lines.push('⏳ 正在等待 SPS/PPS 配置...');
        } else if (stats.idrCount === 0) {
            lines.push('⏳ 等待首个 IDR 关键帧...');
        } else {
            lines.push('✅ 视频流接收正常');
        }

        let y = 30;
        lines.forEach(line => {
            if (line.includes('⚠️') || line.includes('⏳')) {
                ctx.fillStyle = '#ff9800';
            } else if (line.includes('✅')) {
                ctx.fillStyle = '#4caf50';
            } else {
                ctx.fillStyle = '#999';
            }
            ctx.fillText(line, 20, y);
            y += 22;
        });

        // 绘制边框表示正在接收数据
        ctx.strokeStyle = hasWebCodecs ? '#4caf50' : '#ff9800';
        ctx.lineWidth = 3;
        ctx.strokeRect(10, 10, canvas.width - 20, canvas.height - 20);

        // 左上角状态指示器
        ctx.fillStyle = hasWebCodecs ? 'rgba(76, 175, 80, 0.2)' : 'rgba(255, 152, 0, 0.2)';
        ctx.fillRect(10, 10, 120, 25);
        ctx.fillStyle = hasWebCodecs ? '#4caf50' : '#ff9800';
        ctx.font = '14px monospace';
        ctx.fillText(hasWebCodecs ? 'LIVE' : 'NOCODEC', 20, 28);
    }

    // 如果将来集成了真正的 H.264 解码器
    // 这里会处理解码后的 ImageData
    if (frameData instanceof ImageData) {
        canvas.width = frameData.width;
        canvas.height = frameData.height;
        ctx.putImageData(frameData, 0, 0);

        // 调整 phoneFrame 尺寸
        const maxWidth = window.innerWidth - 40;
        const maxHeight = window.innerHeight - 200;
        const scale = Math.min(maxWidth / canvas.width, maxHeight / canvas.height, 1);

        phoneFrame.style.width = (canvas.width * scale) + 'px';
        phoneFrame.style.height = (canvas.height * scale) + 'px';
    }
}

/**
 * 更新统计信息显示（在视频上方叠加显示）
 */
function updateStatsDisplay(stats) {
    if (!stats) return;

    // 只在左上角显示简化的统计信息
    const statsDiv = document.getElementById('stats');
    if (statsDiv) {
        statsDiv.textContent = `FPS: ${fps} | 帧: ${stats.decodedFrames || 0} | ${canvas.width}x${canvas.height}`;
    }
}

/**
 * 应用屏幕尺寸
 */
function applyScreenSize() {
    screenWidth = parseInt(document.getElementById('screenWidth').value) || 1080;
    screenHeight = parseInt(document.getElementById('screenHeight').value) || 1920;
    console.log(`屏幕尺寸已更新: ${screenWidth}x${screenHeight}`);
}

// ========== Canvas 事件处理 ==========

let isDragging = false;
let lastTouchX = 0;
let lastTouchY = 0;

canvas.addEventListener('mousedown', (e) => {
    isDragging = true;

    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;

    lastTouchX = x;
    lastTouchY = y;

    // 转换坐标并发送 DOWN 事件
    const deviceCoords = canvasToDeviceCoords(x, y);
    sendTouchEvent(ACTION_DOWN, deviceCoords.x, deviceCoords.y);

    // 显示触摸指示器
    showTouchIndicator(x, y);
});

canvas.addEventListener('mousemove', (e) => {
    if (!isDragging) return;

    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;

    // 发送 MOVE 事件
    const deviceCoords = canvasToDeviceCoords(x, y);
    sendTouchEvent(ACTION_MOVE, deviceCoords.x, deviceCoords.y);

    lastTouchX = x;
    lastTouchY = y;

    // 显示触摸指示器
    showTouchIndicator(x, y);
});

canvas.addEventListener('mouseup', (e) => {
    if (!isDragging) return;
    isDragging = false;

    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;

    // 发送 UP 事件
    const deviceCoords = canvasToDeviceCoords(x, y);
    sendTouchEvent(ACTION_UP, deviceCoords.x, deviceCoords.y);
});

canvas.addEventListener('mouseleave', (e) => {
    if (isDragging) {
        isDragging = false;

        const rect = canvas.getBoundingClientRect();
        const x = e.clientX - rect.left;
        const y = e.clientY - rect.top;

        // 发送 CANCEL 事件
        const deviceCoords = canvasToDeviceCoords(x, y);
        sendTouchEvent(ACTION_CANCEL, deviceCoords.x, deviceCoords.y);
    }
});

// 触摸事件支持 (移动设备)
canvas.addEventListener('touchstart', (e) => {
    e.preventDefault();

    const touch = e.touches[0];
    const rect = canvas.getBoundingClientRect();
    const x = touch.clientX - rect.left;
    const y = touch.clientY - rect.top;

    lastTouchX = x;
    lastTouchY = y;

    const deviceCoords = canvasToDeviceCoords(x, y);
    sendTouchEvent(ACTION_DOWN, deviceCoords.x, deviceCoords.y);

    showTouchIndicator(x, y);
});

canvas.addEventListener('touchmove', (e) => {
    e.preventDefault();

    const touch = e.touches[0];
    const rect = canvas.getBoundingClientRect();
    const x = touch.clientX - rect.left;
    const y = touch.clientY - rect.top;

    const deviceCoords = canvasToDeviceCoords(x, y);
    sendTouchEvent(ACTION_MOVE, deviceCoords.x, deviceCoords.y);

    showTouchIndicator(x, y);
});

canvas.addEventListener('touchend', (e) => {
    e.preventDefault();

    const rect = canvas.getBoundingClientRect();
    const x = lastTouchX;
    const y = lastTouchY;

    const deviceCoords = canvasToDeviceCoords(x, y);
    sendTouchEvent(ACTION_UP, deviceCoords.x, deviceCoords.y);
});

// ========== 按钮事件 ==========

document.getElementById('connectBtn').addEventListener('click', connect);
document.getElementById('disconnectBtn').addEventListener('click', disconnect);
document.getElementById('resizeBtn').addEventListener('click', applyScreenSize);
document.getElementById('testClickBtn').addEventListener('click', () => {
    const x = parseInt(document.getElementById('testX').value) || 540;
    const y = parseInt(document.getElementById('testY').value) || 960;

    console.log(`\n========== 测试点击 ==========`);
    console.log(`直接发送设备坐标: (${x}, ${y})`);

    // 直接发送设备坐标,不经过坐标转换
    sendTouchEvent(ACTION_DOWN, x, y);

    setTimeout(() => {
        sendTouchEvent(ACTION_UP, x, y);
    }, 50);

    console.log(`============================\n`);
});

// ========== 初始化 ==========

// 设置初始 canvas 尺寸
canvas.width = 1080 / 2;
canvas.height = 1920 / 2;
phoneFrame.style.width = canvas.width + 'px';
phoneFrame.style.height = canvas.height + 'px';

// 清空画布
ctx.fillStyle = '#000';
ctx.fillRect(0, 0, canvas.width, canvas.height);

// 绘制提示文字
ctx.fillStyle = '#666';
ctx.font = '24px sans-serif';
ctx.textAlign = 'center';
ctx.fillText('请点击"连接"按钮开始', canvas.width / 2, canvas.height / 2);

console.log('Scrcpy Web Viewer 已加载');
