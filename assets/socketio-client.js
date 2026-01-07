/**
 * Scrcpy Web Viewer - Socket.IO Client
 * 实现视频流接收、解码和触摸事件发送
 */

// ========== API 配置和日志系统 ==========

// API 配置 - 使用当前页面的 host 和端口
const API_BASE = () => `${window.location.protocol}//${window.location.host}`;

// 日志系统
function log(message, level = 'info') {
    const logContainer = document.getElementById('logContainer');
    if (!logContainer) return;

    const timestamp = new Date().toLocaleTimeString();
    const entry = document.createElement('div');
    entry.className = `log-entry ${level}`;
    entry.innerHTML = `<span class="log-timestamp">[${timestamp}]</span>${escapeHtml(message)}`;
    logContainer.appendChild(entry);

    if (document.getElementById('autoScroll') && document.getElementById('autoScroll').checked) {
        logContainer.scrollTop = logContainer.scrollHeight;
    }

    // 同时输出到控制台
    console.log(`[${level.toUpperCase()}] ${message}`);
}

// HTML 转义函数
function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// ========== 设备管理 API ==========

// 获取设备列表
async function fetchDevices() {
    try {
        log('获取设备列表...', 'info');
        const response = await fetch(`${API_BASE()}/devices`);
        if (!response.ok) throw new Error('获取设备列表失败');

        const devices_response = await response.json();
        const devices = devices_response.devices;
        log(`获取到 ${devices.length} 个设备`, 'success');

        const select = document.getElementById('deviceSelect');
        select.innerHTML = '<option value="">-- 选择设备 --</option>';
        devices.forEach(device => {
            const option = document.createElement('option');
            option.value = device.serial;
            option.textContent = device.serial + " : " +  device.status;
            select.appendChild(option);
        });
        if(devices.length > 0)[
            select.value = devices[0].serial
        ]
    } catch (error) {
        log(`获取设备列表失败: ${error.message}`, 'error');
    }
}

// 连接到设备并自动连接 Socket.IO
async function connectToDevice() {
    const connectBtn = document.getElementById('connectDeviceBtn');
    const deviceSerial = document.getElementById('deviceSelect').value;

    // 检查是否需要断开连接
    if (connectBtn.textContent === '断开连接') {
        disconnectSocket();
        updateConnectButton(false);
        return;
    }

    if (!deviceSerial) {
        log('请先选择设备', 'warn');
        return;
    }

    try {
        // 禁用按钮，防止重复点击
        connectBtn.disabled = true;
        connectBtn.textContent = '连接中...';

        log(`连接到设备: ${deviceSerial}`, 'info');
        const response = await fetch(`${API_BASE()}/connect`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ serial: deviceSerial })
        });

        if (!response.ok) throw new Error('连接设备失败');

        const data = await response.json();
        const port = data.data.socketio_port;
        log(`设备连接成功, Socket.IO 端口: ${port}`, 'success');

        // 更新设备状态点
        document.getElementById('deviceStatusDot').classList.remove('disconnected');
        document.getElementById('deviceStatusDot').classList.add('connected');

        // 自动连接 Socket.IO
        await connectToSocketIO(port);

        // 更新按钮为断开连接
        updateConnectButton(true);

    } catch (error) {
        log(`连接设备失败: ${error.message}`, 'error');
        updateConnectButton(false);
    } finally {
        connectBtn.disabled = false;
    }
}

// 连接到 Socket.IO
async function connectToSocketIO(port) {
    const ip = '127.0.0.1';
    const url = `http://${ip}:${port}`;

    log(`正在连接 Socket.IO: ${url}`, 'info');

    // 清理旧连接
    if (socket) {
        socket.disconnect();
        socket = null;
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
        log('Socket.IO 连接成功', 'success');
        // 更新 Socket.IO 状态点
        document.getElementById('socketStatusDot').classList.remove('disconnected');
        document.getElementById('socketStatusDot').classList.add('connected');

        // 更新按钮为断开连接
        updateConnectButton(true);

        // 发送测试消息
        socket.emit('test', { message: 'Hello from web client' });
    });

    socket.on('test_response', (data) => {
        log(`收到测试响应: ${JSON.stringify(data)}`, 'info');
    });

    // 处理设备元数据
    socket.on('scrcpy_device_meta', (deviceName) => {
        log(`收到设备元数据: ${deviceName}`, 'success');

        // 重置解码器以处理新的解码数据
        if (decoder) {
            // 销毁旧解码器
            decoder.destroy();

            // 创建新解码器
            decoder = new H264Decoder();
            decoder.init((frameData) => {
                drawFrame(frameData);
            });

            log('解码器已重置，准备接收新的解码数据', 'info');
        }
    });

    socket.on('scrcpy', (base64Data) => {
        // 接收到 scrcpy 视频数据 (base64 编码)
        handleVideoData(base64Data);
    });

    socket.on('scrcpy_ctl_ack', (data) => {
        // log(`✓ 服务器确认收到事件`, 'info');
    });

    socket.on('scrcpy_ctl_error', (data) => {
        log(`❌ 触摸事件发送失败: ${data.error}`, 'error');
    });

    socket.on('connect_error', (err) => {
        log(`连接失败: ${err.message}`, 'error');
        // 更新 Socket.IO 状态点
        document.getElementById('socketStatusDot').classList.remove('connected');
        document.getElementById('socketStatusDot').classList.add('disconnected');

        // 更新按钮为连接设备
        updateConnectButton(false);
    });

    socket.on('disconnect', (reason) => {
        log(`断开连接: ${reason}`, 'warn');
        // 更新 Socket.IO 状态点
        document.getElementById('socketStatusDot').classList.remove('connected');
        document.getElementById('socketStatusDot').classList.add('disconnected');

        // 更新按钮为连接设备
        updateConnectButton(false);
    });

    // 初始化解码器
    decoder = new H264Decoder();
    decoder.init((frameData) => {
        // 解码后的帧数据回调
        drawFrame(frameData);
    });
}

// 断开 Socket.IO 连接
function disconnectSocket() {
    if (socket) {
        socket.disconnect();
        socket = null;
    }

    if (decoder) {
        decoder.destroy();
        decoder = null;
    }

    // 更新状态点
    document.getElementById('socketStatusDot').classList.remove('connected');
    document.getElementById('socketStatusDot').classList.add('disconnected');
    document.getElementById('deviceStatusDot').classList.remove('connected');
    document.getElementById('deviceStatusDot').classList.add('disconnected');

    // 清空画布
    ctx.fillStyle = '#000';
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    // 更新按钮为连接设备
    updateConnectButton(false);

    log('已断开连接', 'info');
}

// 更新连接按钮的状态
function updateConnectButton(isConnected) {
    const connectBtn = document.getElementById('connectDeviceBtn');

    if (isConnected) {
        connectBtn.textContent = '断开连接';
        connectBtn.classList.add('disconnect');
    } else {
        connectBtn.textContent = '连接设备';
        connectBtn.classList.remove('disconnect');
    }
}

// ========== 原有的 Socket.IO 客户端代码 ==========

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

// Scrcpy 控制消息类型 (根据 scrcpy control_msg.h)
const SCRCPY_MSG_TYPE_INJECT_KEYCODE = 0;   // 按键代码事件
const SCRCPY_MSG_TYPE_INJECT_TEXT = 1;      // 文本输入事件
const SCRCPY_MSG_TYPE_INJECT_TOUCH_EVENT = 2;  // 触摸事件
const SCRCPY_MSG_TYPE_SET_DISPLAY_POWER = 9;  // 设置电源 (根据枚举位置)

// Android 按键动作
const KEY_ACTION_DOWN = 0;
const KEY_ACTION_UP = 1;

// Android 按键代码 (KEYCODE_*)
const KEYCODE_DEL = 0x0043;      // Backspace (KEYCODE_DEL = 67)
const KEYCODE_FORWARD_DEL = 0x0070;  // Delete (向前删除, KEYCODE_FORWARD_DEL = 112)
const KEYCODE_ENTER = 0x0042;    // Enter (KEYCODE_ENTER = 66)
const KEYCODE_TAB = 0x003d;      // Tab (KEYCODE_TAB = 61)
const KEYCODE_ESCAPE = 0x006f;   // Escape (KEYCODE_ESCAPE = 111)

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

        // Scrcpy 协议状态
        this.state = 'init'; // init, read_codec_meta, read_frame_head, read_frame_data, streaming
        this.buffer = [];
        this.bufferSize = 0;

        // 编解码器元数据
        this.codecMeta = null;

        // 帧头
        this.frameHeader = null;
        this.remainingFrameBytes = 0;

        this.stats = {
            totalBytes: 0,
            totalPackets: 0,
            spsCount: 0,
            ppsCount: 0,
            idrCount: 0,
            pFrameCount: 0,
            decodedFrames: 0,
            droppedFrames: 0,
            decodeErrors: 0,
            garbageBytesSkipped: 0
        };

        // WebCodecs 解码器
        this.videoDecoder = null;
        this.useWebCodecs = false;
        this.pendingFrames = 0;
        this.maxPendingFrames = 10;
        this.decodeQueue = [];  // 解码队列
        this.isDecoding = false;  // 是否正在解码

        // H.264 Parser - 使用累积buffer方式
        this.h264Buffer = new Uint8Array(0);
        this.sps = null;
        this.pps = null;
        this.hasKeyFrame = false;
        this.loggedSps = false;
        this.loggedPps = false;

        // 适配性增强
        this.timestampBase = null;  // 时间戳基准
        this.frameIndex = 0;         // 帧计数器
        this.actualVideoSize = null; // 实际视频尺寸（从 SPS 解析）
        this.decoderConfigured = false;
        this.consecutiveErrors = 0;  // 连续错误计数
        this.maxConsecutiveErrors = 5; // 最大连续错误后重置
        this.isFirstKeyFrameAfterConfigure = false; // 标记是否是configure后的第一个关键帧
        this.decoderNeedsKeyFrame = true; // 解码器是否需要关键帧
    }

    async init(callback) {
        this.frameCallback = callback;

        // 检查是否支持 WebCodecs API
        if (typeof VideoDecoder !== 'undefined') {
            this.useWebCodecs = true;
            try {
                await this.initWebCodecs();
                log('H.264 解码器初始化完成 (使用 WebCodecs API)', 'success');
            } catch (e) {
                console.error('初始化 WebCodecs 解码器失败:', e);
                this.useWebCodecs = false;
            }
        }

        if (!this.useWebCodecs) {
            log('WebCodecs API 不可用，使用数据解析模式', 'warn');
            log('视频流将显示数据统计信息而不是实际画面', 'warn');
            log('建议使用 Chrome 94+ 或 Edge 94+ 以获得硬件加速解码', 'warn');
        }
    }

    async initWebCodecs() {
        // 检查浏览器支持的编解码器
        const supportedCodecs = [];
        const testCodecs = [
            'avc1.64001F',  // H.264 High
            'avc1.4D001F',  // H.264 Main
            'avc1.42001F',  // H.264 Baseline
            'avc1.640028',  // H.264 High Level 40
            'avc1.64002A',  // H.264 High Level 42
        ];

        for (const codec of testCodecs) {
            if (VideoDecoder.isConfigSupported({ codec: codec, codedWidth: 1920, codedHeight: 1080 })) {
                supportedCodecs.push(codec);
            }
        }

        log(`浏览器支持的 H.264 编码器: ${supportedCodecs.join(', ')}`, 'info');

        // 创建 VideoDecoder 实例 - 参考 demo 直接在 output 回调中绘制
        this.videoDecoder = new VideoDecoder({
            output: (frame) => {
                const startTime = performance.now();
                this.stats.decodedFrames++;
                this.pendingFrames--;
                this.consecutiveErrors = 0; // 重置连续错误计数

                // 第一帧解码成功的日志
                if (this.stats.decodedFrames === 1) {
                    const actualWidth = frame.visibleRect?.width || frame.displayWidth || frame.codedWidth;
                    const actualHeight = frame.visibleRect?.height || frame.displayHeight || frame.codedHeight;
                    log(`✅ 第一帧解码成功! 实际尺寸: ${actualWidth}x${actualHeight}`, 'success');

                    // 记录实际视频尺寸
                    if (!this.actualVideoSize) {
                        this.actualVideoSize = { width: actualWidth, height: actualHeight };
                    }
                }

                // 使用 visible rect if available (for cropped videos)
                const visibleWidth = frame.visibleRect?.width || frame.displayWidth || frame.codedWidth;
                const visibleHeight = frame.visibleRect?.height || frame.displayHeight || frame.codedHeight;
                const offsetX = frame.visibleRect?.x || 0;
                const offsetY = frame.visibleRect?.y || 0;

                // 设置 canvas 尺寸为可见尺寸
                if (canvas.width !== visibleWidth || canvas.height !== visibleHeight) {
                    canvas.width = visibleWidth;
                    canvas.height = visibleHeight;

                    // 调整 phoneFrame 尺寸以适应视频
                    const maxWidth = window.innerWidth - 40;
                    const maxHeight = window.innerHeight - 200;
                    const scale = Math.min(maxWidth / visibleWidth, maxHeight / visibleHeight, 1);

                    phoneFrame.style.width = (visibleWidth * scale) + 'px';
                    phoneFrame.style.height = (visibleHeight * scale) + 'px';
                }

                // 直接绘制 VideoFrame 到 canvas (与 demo 相同的方式)
                ctx.drawImage(frame, offsetX, offsetY, visibleWidth, visibleHeight, 0, 0, visibleWidth, visibleHeight);

                // 立即关闭 frame 释放资源
                frame.close();

                // 更新统计信息显示
                updateStatsDisplay({ ...this.stats });
            },
            error: (error) => {
                this.stats.decodeErrors++;
                this.stats.droppedFrames++;
                this.consecutiveErrors++;

                log(`❌ VideoDecoder 错误 (${this.consecutiveErrors}/${this.maxConsecutiveErrors}): ${error.message}`, 'error');

                // 连续错误过多时尝试重置解码器
                if (this.consecutiveErrors >= this.maxConsecutiveErrors) {
                    log('连续错误过多，尝试重置解码器...', 'warn');
                    this.resetDecoder();
                }
            }
        });

        console.log('WebCodecs VideoDecoder 已创建，等待编解码器元数据...');
    }

    // 重置解码器
    resetDecoder() {
        if (this.videoDecoder) {
            try {
                if (this.videoDecoder.state === 'configured') {
                    this.videoDecoder.close();
                }
            } catch (e) {
                console.warn('关闭解码器时出错:', e);
            }
        }

        // 重新初始化
        this.decoderConfigured = false;
        this.decoderNeedsKeyFrame = true; // 重置后需要关键帧
        this.consecutiveErrors = 0;
        this.sps = null;
        this.pps = null;
        this.hasKeyFrame = false;

        // 保留 h264Buffer 中的数据，可能仍有有效帧
        log('解码器已重置，等待下一个关键帧...', 'info');
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

    configureDecoder(sps, pps) {
        if (!this.videoDecoder || this.decoderConfigured) {
            return false;
        }

        try {
            // 辅助函数：检测并去除起始码
            const stripStartCode = (data) => {
                if (data.length >= 4 && data[0] === 0x00 && data[1] === 0x00 &&
                    data[2] === 0x00 && data[3] === 0x01) {
                    return { data: data.slice(4), startCodeLen: 4 }; // 4 字节起始码
                } else if (data.length >= 3 && data[0] === 0x00 && data[1] === 0x00 &&
                           data[2] === 0x01) {
                    return { data: data.slice(3), startCodeLen: 3 }; // 3 字节起始码
                }
                return { data: data, startCodeLen: 0 }; // 无起始码
            };

            // 调试：输出SPS和PPS的完整信息
            const spsHex = Array.from(sps.slice(0, Math.min(25, sps.length)))
                .map(b => b.toString(16).padStart(2, '0')).join(' ');
            const ppsHex = Array.from(pps.slice(0, Math.min(15, pps.length)))
                .map(b => b.toString(16).padStart(2, '0')).join(' ');
            log(`SPS raw (${sps.length}字节): ${spsHex}...`, 'info');
            log(`PPS raw (${pps.length}字节): ${ppsHex}...`, 'info');

            // 去除起始码获取纯 SPS/PPS 数据
            const spsResult = stripStartCode(sps);
            const ppsResult = stripStartCode(pps);

            log(`SPS: 去除${spsResult.startCodeLen}字节起始码，剩余${spsResult.data.length}字节`, 'info');
            log(`PPS: 去除${ppsResult.startCodeLen}字节起始码，剩余${ppsResult.data.length}字节`, 'info');

            // SPS/PPS 数据包含 NAL header (1 byte)
            // NAL header 后面才是真正的 SPS/PPS 数据
            if (spsResult.data.length < 2) {
                log('SPS 数据太短，无法解析', 'error');
                return false;
            }

            // 输出 NAL header
            const nalHeader = spsResult.data[0];
            const nalType = nalHeader & 0x1F;
            log(`SPS NAL header: 0x${nalHeader.toString(16).padStart(2, '0')}, type=${nalType}`, 'info');

            // H.264 SPS structure: [NAL header(1B)][profile_idc(1B)][constraint_set_flags(1B)][level_idc(1B)]...
            const profile = spsResult.data[1];  // profile_idc
            const constraint = spsResult.data[2];  // constraint_set_flags
            const level = spsResult.data[3];  // level_idc

            // Profile 名称映射
            const profileNames = {
                66: 'Baseline',
                77: 'Main',
                88: 'Extended',
                100: 'High',
                110: 'High 10',
                122: 'High 4:2:2',
                244: 'High 4:4:4'
            };
            const profileName = profileNames[profile] || `Unknown(${profile})`;

            log(`SPS解析: profile=${profileName} (0x${profile.toString(16)}), constraint=0x${constraint.toString(16)}, level=0x${level.toString(16)}`, 'info');

            // 检查 profile 是否有效
            if (profile === 103 || profile > 244) {
                log(`警告: 无效的 profile=${profile}，可能 SPS 数据有误`, 'warn');
                log(`SPS 完整数据: ${Array.from(spsResult.data).map(b => b.toString(16).padStart(2, '0')).join(' ')}`, 'warn');
            }

            // Format: avc1.PPCCLL (6 hex digits)
            const codecString = `avc1.${profile.toString(16).padStart(2, '0')}${(constraint & 0x3F).toString(16).padStart(2, '0')}${Math.max(level, 1).toString(16).padStart(2, '0')}`;

            log(`生成的 codec 字符串: ${codecString}`, 'info');

            // Build proper AVCC description (avcC box format)
            // SPS/PPS data without NAL header
            const spsData = spsResult.data.slice(1); // 去除 NAL header (1 byte)
            const ppsData = ppsResult.data.slice(1); // 去除 NAL header (1 byte)

            log(`用于 AVCC 的 SPS 数据长度=${spsData.length}, PPS 数据长度=${ppsData.length}`, 'info');

            // 尝试构建完整的 AVCC description
            // 根据 ISO/IEC 14496-15 标准，可能需要更多字段
            let description = null;

            try {
                // 方法1: 完整的 AVCC description
                const spsLen = spsData.length;
                const ppsLen = ppsData.length;
                const descSize = 6 + 2 + spsLen + 1 + 2 + ppsLen;

                description = new Uint8Array(descSize);
                let offset = 0;

                // AVCC header
                description[offset++] = 1;  // configurationVersion
                description[offset++] = profile;  // AVCProfileIndication
                description[offset++] = constraint;  // profile_compatibility
                description[offset++] = Math.max(level, 1);  // AVCLevelIndication
                description[offset++] = 0xFF;  // lengthSizeMinusOne (all 1s) = 4 bytes

                // SPS
                description[offset++] = 0xE1;  // numOfSequenceParameterSets (5 bits reserved + 3 bits count)
                // SPS length (big-endian 16-bit)
                description[offset++] = (spsLen >> 8) & 0xFF;
                description[offset++] = spsLen & 0xFF;
                // SPS data
                description.set(spsData, offset);
                offset += spsLen;

                // PPS
                description[offset++] = 1;  // numOfPictureParameterSets
                // PPS length (big-endian 16-bit)
                description[offset++] = (ppsLen >> 8) & 0xFF;
                description[offset++] = ppsLen & 0xFF;
                // PPS data
                description.set(ppsData, offset);

                log(`AVCC description 构建: ${descSize}字节`, 'info');
            } catch (e) {
                log(`AVCC description 构建失败: ${e.message}`, 'warn');
                description = null;
            }

            if (description) {
                log(`AVCC 数据 (前20字节): ${Array.from(description.slice(0, 20)).map(b => b.toString(16).padStart(2, '0')).join(' ')}`, 'info');
            }

            // 尝试使用实际视频尺寸（如果已解析），否则使用元数据尺寸
            let configWidth = this.actualVideoSize?.width || screenWidth;
            let configHeight = this.actualVideoSize?.height || screenHeight;

            log(`视频尺寸配置: ${configWidth}x${configHeight}`, 'info');

            // 尝试多种配置方式，按优先级排序
            // 关键修复：首先尝试不带 description 的配置，让解码器从数据流中自动提取 SPS/PPS
            const configs = [
                // 1. 最简配置 - 只包含 codec，让解码器自动从数据流中提取 SPS/PPS
                {
                    codec: codecString,
                    desc: '最简配置（自动提取SPS/PPS）'
                },
                // 2. 最简配置 + 尺寸
                {
                    codec: codecString,
                    codedWidth: configWidth,
                    codedHeight: configHeight,
                    desc: '最简配置 + 尺寸'
                },
                // 3. 使用 description（如果可用）
                ...(description ? [{
                    codec: codecString,
                    description: description,
                    desc: '使用 description'
                }] : []),
                // 4. 通用配置
                {
                    codec: 'avc1.64002A',
                    desc: '通用 High 42'
                }
            ];

            let configured = false;
            for (let i = 0; i < configs.length; i++) {
                try {
                    const cfg = configs[i];
                    log(`尝试配置 #${i}: ${cfg.codec} (${cfg.codedWidth}x${cfg.codedHeight}) - ${cfg.desc}`, 'info');

                    // 检查是否支持
                    const supportCheck = VideoDecoder.isConfigSupported(cfg);
                    log(`  配置支持检查: ${supportCheck.supported}`, 'info');

                    this.videoDecoder.configure(cfg);
                    this.decoderConfigured = true;
                    this.decoderNeedsKeyFrame = true; // 配置后需要关键帧

                    log(`✅ Decoder configured: ${cfg.codec} (${configWidth}x${configHeight}) - ${cfg.desc}`, 'success');
                    log(`⚠️ 重要：配置完成后，下一个关键帧(IDR)才能开始解码`, 'info');

                    configured = true;
                    break;
                } catch (err) {
                    log(`❌ 配置 #${i} 失败: ${err.message}`, 'warn');
                }
            }

            if (!configured) {
                log('❌ 所有配置尝试均失败', 'error');
                return false;
            }

            return true;
        } catch (e) {
            log(`❌ Configure decoder 异常: ${e.message}`, 'error');
            console.error('Configure decoder exception:', e);
            return false;
        }
    }

    decode(data) {
        try {
            // 将新数据追加到缓冲区
            this.buffer.push(new Uint8Array(data));
            this.bufferSize += data.length;
            this.stats.totalBytes += data.length;

            // 合并缓冲区
            const combined = new Uint8Array(this.bufferSize);
            let offset = 0;
            for (const chunk of this.buffer) {
                combined.set(chunk, offset);
                offset += chunk.length;
            }

            let parseOffset = 0;

            while (parseOffset < combined.length) {
                switch (this.state) {
                    case 'init':
                    case 'read_codec_meta':
                        // 需要读取 12 字节编解码器元数据
                        if (combined.length - parseOffset < 12) {
                            // 数据不足，保留剩余数据
                            this.buffer = [combined.slice(parseOffset)];
                            this.bufferSize = combined.length - parseOffset;
                            return;
                        }

                        // 读取 12 字节编解码器元数据
                        this.codecMeta = this.parseCodecMeta(combined, parseOffset);
                        log(`收到编解码器元数据: codec=${this.codecMeta.codecId}, ${this.codecMeta.width}x${this.codecMeta.height}`, 'success');

                        // 更新视频尺寸
                        screenWidth = this.codecMeta.width;
                        screenHeight = this.codecMeta.height;

                        parseOffset += 12;
                        this.state = 'read_frame_head';
                        break;

                    case 'read_frame_head':
                        // 需要读取 12 字节帧头
                        if (combined.length - parseOffset < 12) {
                            this.buffer = [combined.slice(parseOffset)];
                            this.bufferSize = combined.length - parseOffset;
                            return;
                        }

                        // 读取 12 字节帧头
                        this.frameHeader = this.parseFrameHeader(combined, parseOffset);
                        this.remainingFrameBytes = this.frameHeader.packetSize;

                        if (this.frameHeader.packetSize === 0) {
                            // 空帧，跳过
                            parseOffset += 12;
                            break;
                        }

                        parseOffset += 12;
                        this.state = 'read_frame_data';
                        break;

                    case 'read_frame_data':
                        // 检查是否有足够的帧数据
                        if (combined.length - parseOffset < this.remainingFrameBytes) {
                            // 数据不完整，保留剩余数据
                            this.buffer = [combined.slice(parseOffset)];
                            this.bufferSize = combined.length - parseOffset;
                            return;
                        }

                        // 提取完整的 H.264 帧数据
                        const frameData = combined.slice(parseOffset, parseOffset + this.remainingFrameBytes);
                        parseOffset += this.remainingFrameBytes;

                        // 处理 H.264 帧 - 使用demo的方式
                        this.processH264FrameData(frameData);

                        this.state = 'read_frame_head';
                        this.frameHeader = null;
                        this.remainingFrameBytes = 0;
                        break;
                }
            }

            // 清空缓冲区
            this.buffer = [];
            this.bufferSize = 0;

        } catch (e) {
            console.error('解码错误:', e);
            log(`解码错误: ${e.message}`, 'error');
            // 清空缓冲区以恢复
            this.buffer = [];
            this.bufferSize = 0;
            this.state = 'read_frame_head';
        }
    }

    // 使用demo的方式处理H.264帧数据
    processH264FrameData(frameData) {
        // 调试：输出帧数据的前100字节和长度
        if (this.stats.decodedFrames === 0 && frameData.length > 0) {
            const frameHex = Array.from(frameData.slice(0, Math.min(100, frameData.length)))
                .map(b => b.toString(16).padStart(2, '0')).join(' ');
            log(`帧数据 (前100字节): ${frameHex}..., total=${frameData.length}`, 'info');
        }

        // Feed data to H.264 buffer
        const newBuffer = new Uint8Array(this.h264Buffer.length + frameData.length);
        newBuffer.set(this.h264Buffer);
        newBuffer.set(frameData, this.h264Buffer.length);
        this.h264Buffer = newBuffer;

        // 调试：输出h264Buffer状态
        if (this.stats.decodedFrames === 0 && this.h264Buffer.length > 0) {
            log(`h264Buffer长度=${this.h264Buffer.length}, 前20字节: ${Array.from(this.h264Buffer.slice(0, 20)).map(b => b.toString(16).padStart(2, '0')).join(' ')}`, 'info');
        }

        // Check if we have SPS and PPS to log
        if (this.sps && !this.loggedSps) {
            this.loggedSps = true;
            log('Found SPS', 'info');
        }
        if (this.pps && !this.loggedPps) {
            this.loggedPps = true;
            log('Found PPS', 'info');
        }

        // Extract and process NAL units
        let decoded = false;
        for (const nalUnit of this.extractNALUnits()) {
            // 获取 NAL 类型 - 需要跳过起始码
            // 支持 3 和 4 字节起始码
            let nalHeaderOffset = 0;
            if (nalUnit.length >= 4 && nalUnit[0] === 0x00 && nalUnit[1] === 0x00 &&
                nalUnit[2] === 0x00 && nalUnit[3] === 0x01) {
                nalHeaderOffset = 4;
            } else if (nalUnit.length >= 3 && nalUnit[0] === 0x00 && nalUnit[1] === 0x00 &&
                       nalUnit[2] === 0x01) {
                nalHeaderOffset = 3;
            }

            if (nalHeaderOffset === 0 || nalUnit.length <= nalHeaderOffset) {
                continue; // 跳过无效的 NALU
            }

            const nalType = nalUnit[nalHeaderOffset] & 0x1F;
            const isKeyFrame = (nalType === 5);

            // Store SPS (type 7) and PPS (type 8)
            if (nalType === 7) {
                this.sps = nalUnit;
                this.stats.spsCount++;
                log('H.264 NALU: SPS (7), ' + nalUnit.length + '字节', 'success');

                // 尝试在收到 SPS/PPS 后立即配置解码器
                if (this.hasCodecConfig() && !this.decoderConfigured && this.videoDecoder && this.videoDecoder.state === 'unconfigured') {
                    if (this.configureDecoder(this.sps, this.pps)) {
                        log('Decoder configured with SPS/PPS', 'success');
                    }
                }
            } else if (nalType === 8) {
                this.pps = nalUnit;
                this.stats.ppsCount++;
                log('H.264 NALU: PPS (8), ' + nalUnit.length + '字节', 'success');

                // 尝试在收到 SPS/PPS 后立即配置解码器
                if (this.hasCodecConfig() && !this.decoderConfigured && this.videoDecoder && this.videoDecoder.state === 'unconfigured') {
                    if (this.configureDecoder(this.sps, this.pps)) {
                        log('Decoder configured with SPS/PPS', 'success');
                    }
                }
            } else if (nalType === 5) {
                // IDR frame (key frame)
                if (!this.hasKeyFrame) {
                    this.hasKeyFrame = true;
                    this.stats.idrCount++;
                    log('Found key frame', 'success');
                }

                // 如果还没有配置解码器，尝试用关键帧配置
                if (this.hasCodecConfig() && !this.decoderConfigured && this.videoDecoder && this.videoDecoder.state === 'unconfigured') {
                    if (this.configureDecoder(this.sps, this.pps)) {
                        log('Decoder configured with key frame', 'success');
                    }
                }
            } else if (nalType === 1) {
                this.stats.pFrameCount++;
            }

            // Only decode video frame NAL units (1-5) when decoder is ready
            // 1: non-IDR slice, 5: IDR slice (key frame)

            // 关键修复：如果解码器刚配置完成，需要等待关键帧
            if (this.decoderNeedsKeyFrame && !isKeyFrame) {
                if (nalType >= 1 && nalType <= 5 && this.stats.decodedFrames === 0) {
                    log(`⏳ 解码器已配置，等待关键帧(IDR)中... 当前NAL类型=${nalType} (P帧)`, 'info');
                }
                continue; // 跳过非关键帧
            }

            if (this.videoDecoder && this.videoDecoder.state === 'configured' && (nalType >= 1 && nalType <= 5)) {
                try {
                    // 关键修复：EncodedVideoChunk 需要使用 Annex-B 格式（带起始码），而不是 AVCC 格式！
                    // AVCC 格式只用于 configure() 的 description 字段
                    // 直接使用 nalUnit（包含起始码）

                    // 对于第一个关键帧，如果有 SPS/PPS，需要把它们放在关键帧前面
                    let chunkData = nalUnit;

                    if (isKeyFrame && this.stats.decodedFrames === 0 && this.sps && this.pps) {
                        // 第一个关键帧：需要附加 SPS 和 PPS
                        log(`📋 第一个关键帧：附加 SPS (${this.sps.length}字节) 和 PPS (${this.pps.length}字节)`, 'info');

                        const totalSize = this.sps.length + this.pps.length + nalUnit.length;
                        chunkData = new Uint8Array(totalSize);
                        let offset = 0;

                        // 附加 SPS
                        chunkData.set(this.sps, offset);
                        offset += this.sps.length;

                        // 附加 PPS
                        chunkData.set(this.pps, offset);
                        offset += this.pps.length;

                        // 附加关键帧
                        chunkData.set(nalUnit, offset);

                        log(`📦 组合后数据大小: ${totalSize}字节 (SPS+PPS+IDR)`, 'debug');
                    }

                    // 调试：输出 NALU 数据信息
                    if (this.stats.decodedFrames < 3 || isKeyFrame) {
                        const naluHex = Array.from(chunkData.slice(0, Math.min(12, chunkData.length)))
                            .map(b => b.toString(16).padStart(2, '0')).join(' ');
                        log(`🎬 解码帧: 类型=${nalType} (${isKeyFrame ? '关键帧' : 'P帧'}), 数据长度=${chunkData.length}, 前12字节=${naluHex}`, 'info');

                        // 输出解码器状态
                        log(`🔧 解码器状态: state=${this.videoDecoder.state}, configured=${this.decoderConfigured}, needsKeyFrame=${this.decoderNeedsKeyFrame}`, 'debug');
                    }

                    // 使用递增的时间戳，基于帧索引
                    this.frameIndex++;
                    const timestamp = this.frameIndex * 33333; // ~30fps

                    // 直接使用 Annex-B 格式的数据（包含起始码）
                    const chunk = new EncodedVideoChunk({
                        type: isKeyFrame ? 'key' : 'delta',
                        timestamp: timestamp,
                        data: chunkData  // 可能包含 SPS+PPS+IDR 或仅 IDR
                    });

                    this.videoDecoder.decode(chunk);
                    decoded = true;

                    // 如果是关键帧，清除"需要关键帧"标志
                    if (isKeyFrame && this.decoderNeedsKeyFrame) {
                        this.decoderNeedsKeyFrame = false;
                        log(`✅ 配置后的第一个关键帧已解码，后续P帧可以正常解码`, 'success');
                    }

                    if (isKeyFrame) {
                        log(`✅ 关键帧已送入解码器 (frameIndex=${this.frameIndex}, timestamp=${timestamp})`, 'success');
                    }
                } catch (e) {
                    log(`❌ Decode error: ${e.message}`, 'error');
                    console.error('Decode exception:', e);

                    // 如果错误是"需要关键帧"，重新设置标志
                    if (e.message.includes('key frame') || e.message.includes('keyframe')) {
                        this.decoderNeedsKeyFrame = true;
                        log(`🔄 重置 needsKeyFrame 标志，等待下一个关键帧`, 'info');
                    }
                }
            } else {
                // 调试:为什么没有解码这个NALU
                if (nalType >= 1 && nalType <= 5) {
                    if (!this.videoDecoder) {
                        log(`NALU ${nalType} 跳过: decoder未创建`, 'warn');
                    } else if (this.videoDecoder.state !== 'configured') {
                        log(`NALU ${nalType} 跳过: decoder状态=${this.videoDecoder.state}`, 'warn');
                    } else if (this.videoDecoder.state === 'configured' && this.stats.decodedFrames === 0 && isKeyFrame) {
                        log(`🔴 关键帧到达但解码器未就绪! state=${this.videoDecoder.state}, type=${nalType}`, 'error');
                    } else if (this.decoderNeedsKeyFrame && !isKeyFrame) {
                        // 这个日志在上面已经处理了
                    }
                }
            }
        }

        // Update stats
        if (!decoded) {
            this.stats.totalPackets++;
        }
    }

    // 从一个可能包含多个NALUs的单元中提取子NALUs
    // scrcpy有时会将一个帧的多个NALUs打包在一起
    extractSubNALUnits(nalUnit) {
        const subNALUs = [];
        let pos = 0;

        // 辅助函数：检查 3 字节起始码
        const isStartCode3 = (idx) => {
            return idx >= 0 && idx <= nalUnit.length - 3 &&
                   nalUnit[idx] === 0x00 && nalUnit[idx + 1] === 0x00 && nalUnit[idx + 2] === 0x01;
        };

        // 辅助函数：检查 4 字节起始码
        const isStartCode4 = (idx) => {
            return idx >= 0 && idx <= nalUnit.length - 4 &&
                   nalUnit[idx] === 0x00 && nalUnit[idx + 1] === 0x00 &&
                   nalUnit[idx + 2] === 0x00 && nalUnit[idx + 3] === 0x01;
        };

        while (pos < nalUnit.length - 3) {
            // 查找下一个起始码（支持 3 和 4 字节）
            const startCodeLen = isStartCode4(pos) ? 4 : (isStartCode3(pos) ? 3 : 0);

            if (startCodeLen > 0) {
                const start = pos + startCodeLen; // 跳过起始码本身

                // 查找这个NALU的结束位置（下一个起始码或数据结束）
                let end = start;
                pos = start;

                while (pos < nalUnit.length - 3) {
                    if (isStartCode3(pos) || isStartCode4(pos)) {
                        break;
                    }
                    pos++;
                    end++;
                }

                // 提取这个子NALU（不包含起始码）
                if (end > start) {
                    subNALUs.push(nalUnit.slice(start, end));
                }
            } else {
                pos++;
            }
        }

        // 如果没有找到任何子NALU，返回整个NALU（去掉第一个起始码）
        if (subNALUs.length === 0) {
            // 尝试检测起始码长度
            if (nalUnit.length >= 4 && isStartCode4(0)) {
                return [nalUnit.slice(4)];
            } else if (nalUnit.length >= 3 && isStartCode3(0)) {
                return [nalUnit.slice(3)];
            }
            // 没有起始码，返回原始数据
            return [nalUnit];
        }

        return subNALUs;
    }

    // Extract NAL units from buffer (generator function)
    *extractNALUnits() {
        let i = 0;
        const buf = this.h264Buffer;

        // 调试：记录buffer状态
        if (this.stats.decodedFrames < 2) {
            const bufferPreview = Array.from(buf.slice(0, Math.min(40, buf.length)))
                .map(b => b.toString(16).padStart(2, '0')).join(' ');
            log(`📦 extractNALUnits开始: buffer长度=${buf.length}, 前40字节=${bufferPreview}...`, 'debug');
        }

        // 辅助函数：检查 3 字节起始码 (00 00 01)
        const isStartCode3 = (idx) => {
            return idx >= 0 && idx <= buf.length - 3 &&
                   buf[idx] === 0x00 && buf[idx + 1] === 0x00 && buf[idx + 2] === 0x01;
        };

        // 辅助函数：检查 4 字节起始码 (00 00 00 01)
        const isStartCode4 = (idx) => {
            return idx >= 0 && idx <= buf.length - 4 &&
                   buf[idx] === 0x00 && buf[idx + 1] === 0x00 &&
                   buf[idx + 2] === 0x00 && buf[idx + 3] === 0x01;
        };

        // 跳过起始码之前的垃圾数据（支持 3 和 4 字节起始码）
        let skipped = 0;
        while (i < buf.length - 3) {
            if (isStartCode3(i) || isStartCode4(i)) {
                break;
            }
            i++;
            skipped++;
        }

        // 如果跳过了垃圾数据，记录日志
        if (skipped > 0 && skipped < buf.length) {
            this.stats.garbageBytesSkipped += skipped;
            if (this.stats.garbageBytesSkipped <= 100) { // 限制日志输出
                const garbagePreview = Array.from(buf.slice(0, Math.min(6, skipped)))
                    .map(b => b.toString(16).padStart(2, '0')).join(' ');
                // log(`🗑️ 跳过了 ${skipped} 字节的垃圾数据 (垃圾数据: ${garbagePreview})`, 'warn');
            }
            // 继续处理，不return，让i停留在第一个起始码的位置
        }

        const nalUnitsFound = [];
        while (i < buf.length - 3) {
            // Look for NAL start code (支持 3 和 4 字节)
            const startCodeLen = isStartCode4(i) ? 4 : (isStartCode3(i) ? 3 : 0);

            if (startCodeLen > 0) {
                const start = i;
                i += startCodeLen;

                // Find next NAL unit (look for next start code)
                let end = buf.length;  // Default to end of buffer
                while (i < buf.length - 3) {
                    if (isStartCode3(i) || isStartCode4(i)) {
                        end = i;
                        break;
                    }
                    i++;
                }

                const nalUnit = buf.slice(start, end);
                nalUnitsFound.push(nalUnit);
                yield nalUnit;
            } else {
                i++;
            }
        }

        // 调试：记录找到的NALU
        if (this.stats.decodedFrames < 2 && nalUnitsFound.length > 0) {
            const naluSummaries = nalUnitsFound.map(nalu => {
                // 获取NAL类型
                let offset = 0;
                if (nalu.length >= 4 && nalu[0] === 0x00 && nalu[1] === 0x00 &&
                    nalu[2] === 0x00 && nalu[3] === 0x01) {
                    offset = 4;
                } else if (nalu.length >= 3 && nalu[0] === 0x00 && nalu[1] === 0x00 &&
                           nalu[2] === 0x01) {
                    offset = 3;
                }
                const nalType = offset > 0 && nalu.length > offset ? (nalu[offset] & 0x1F) : -1;
                return `type=${nalType}, len=${nalu.length}`;
            }).join(', ');
            log(`✅ extractNALUnits完成: 找到${nalUnitsFound.length}个NALU [${naluSummaries}]`, 'debug');
        }

        // Keep remaining data (incomplete NAL unit)
        const remaining = buf.slice(i);
        this.h264Buffer = remaining;

        // 调试：记录剩余数据
        if (this.stats.decodedFrames < 2 && remaining.length > 0) {
            const remainingPreview = Array.from(remaining.slice(0, Math.min(20, remaining.length)))
                .map(b => b.toString(16).padStart(2, '0')).join(' ');
            log(`📦 剩余buffer: ${remaining.length}字节, 前20字节=${remainingPreview}...`, 'debug');
        }
    }

    hasCodecConfig() {
        return this.sps !== null && this.pps !== null;
    }

    parseCodecMeta(data, offset) {
        // 12 字节编解码器元数据
        // codec_id (u32, big-endian) - 实际协议使用 big-endian
        // width (u32, big-endian)
        // height (u32, big-endian)

        // 直接从 Uint8Array 创建 DataView，避免 buffer 偏移问题
        const view = new DataView(data.slice(offset, offset + 12).buffer);
        const codecId = view.getUint32(0, false);  // big-endian
        const width = view.getUint32(4, false);   // big-endian
        const height = view.getUint32(8, false);  // big-endian

        return {
            codecId,
            width,
            height
        };
    }

    parseFrameHeader(data, offset) {
        // 12 字节帧头
        // byte 7-0: [config(1bit) | key(1bit) | PTS(62bits)] (big-endian)
        // byte 11-8: packet_size (u32, little-endian)

        // 直接从 Uint8Array 创建 DataView，避免 buffer 偏移问题
        const headerData = data.slice(offset, offset + 12);

        // 调试：输出原始字节
        const rawBytes = Array.from(headerData)
            .map(b => b.toString(16).padStart(2, '0'))
            .join(' ');

        const view = new DataView(headerData.buffer);

        // 读取 packet_size - 尝试大端序和小端序
        const packetSizeLittle = view.getUint32(8, true);   // little-endian
        const packetSizeBig = view.getUint32(8, false);     // big-endian

        // 使用看起来合理的值 (应该在 1-1000000 之间)
        const packetSize = (packetSizeBig > 0 && packetSizeBig < 10000000) ? packetSizeBig : packetSizeLittle;

        // 读取标志位 (byte 7)
        const byte7 = headerData[7];
        const configPacket = (byte7 & 0x80) !== 0;
        const keyFrame = (byte7 & 0x40) !== 0;

        // 读取 PTS (62 bits, big-endian)
        let pts = 0;
        for (let i = 0; i < 8; i++) {
            if (i < 7) {
                pts = (pts << 8) | headerData[i];
            } else {
                // 最后一个字节只有 6 位有效
                pts = (pts << 6) | (headerData[7] & 0x3F);
            }
        }

        return {
            configPacket,
            keyFrame,
            pts,
            packetSize
        };
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
        this.h264Buffer = new Uint8Array(0);
        this.sps = null;
        this.pps = null;
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
 * 构建文本输入事件消息
 * 根据 scrcpy control_msg.h，inject_text 结构:
 * type(1) + text_length(4) + text(utf-8)
 */
function buildTextEvent(text) {
    // 将文本编码为 UTF-8 字节数组
    const encoder = new TextEncoder();
    const textBytes = encoder.encode(text);

    // 计算总长度
    const totalLength = 1 + 4 + textBytes.length;
    const buffer = new ArrayBuffer(totalLength);
    const view = new DataView(buffer);

    let offset = 0;

    // 1. 类型: 1 byte
    view.setUint8(offset, SCRCPY_MSG_TYPE_INJECT_TEXT);
    offset += 1;

    // 2. 文本长度: 4 bytes (big-endian)
    view.setUint32(offset, textBytes.length, false);
    offset += 4;

    // 3. 文本内容 (UTF-8)
    const uint8Array = new Uint8Array(buffer);
    uint8Array.set(textBytes, offset);

    return uint8Array;
}

/**
 * 构建按键事件消息
 * @param {number} action - 按键动作 (KEY_ACTION_DOWN or KEY_ACTION_UP)
 * @param {number} keyCode - Android 按键代码 (KEYCODE_*)
 */
function buildKeyEvent(action, keyCode) {
    // 根据 scrcpy 源码 (control_msg.h)，inject_keycode 结构:
    // type(1) + action(1) + keycode(4) + repeat(4) + metastate(4) = 14 bytes
    const totalLength = 1 + 1 + 4 + 4 + 4;
    const buffer = new ArrayBuffer(totalLength);
    const view = new DataView(buffer);

    let offset = 0;

    // 1. 类型: 1 byte (TYPE_INJECT_KEYCODE = 0, 实际是 SC_CONTROL_MSG_TYPE_INJECT_KEYCODE)
    view.setUint8(offset, SCRCPY_MSG_TYPE_INJECT_KEYCODE);
    offset += 1;

    // 2. 动作: 1 byte (0=DOWN, 1=UP)
    view.setUint8(offset, action);
    offset += 1;

    // 3. 按键代码: 4 bytes (big-endian)
    view.setUint32(offset, keyCode, false);
    offset += 4;

    // 4. 重复次数: 4 bytes (通常为 0, big-endian)
    view.setUint32(offset, 0, false);
    offset += 4;

    // 5. 元状态: 4 bytes (修饰键状态, 0=无修饰键, big-endian)
    view.setUint32(offset, 0, false);

    return new Uint8Array(buffer);
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

    // 3. 指针 ID: 8 bytes (big-endian)
    view.setBigUint64(offset, pointerId, false);
    offset += 8;

    // 4. X 坐标: 4 bytes (big-endian)
    view.setInt32(offset, x, false);
    offset += 4;

    // 5. Y 坐标: 4 bytes (big-endian)
    view.setInt32(offset, y, false);
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

    // 9. 动作按钮: 4 bytes (big-endian)
    view.setInt32(offset, actionButton, false);
    offset += 4;

    // 10. 按钮状态: 4 bytes (big-endian)
    view.setInt32(offset, buttons, false);

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
 * 构建屏幕电源控制消息
 */
function buildDisplayPowerMessage(on) {
    // 消息格式 (总共 2 bytes)
    const buffer = new ArrayBuffer(2);
    const view = new DataView(buffer);

    let offset = 0;

    // 1. 类型: 1 byte (TYPE_SET_DISPLAY_POWER = 10)
    view.setUint8(offset, SCRCPY_MSG_TYPE_SET_DISPLAY_POWER);
    offset += 1;

    // 2. 电源状态: 1 byte (0 = 锁屏, 1 = 解锁)
    view.setUint8(offset, on ? 1 : 0);

    return new Uint8Array(buffer);
}

/**
 * 发送屏幕电源控制事件
 * @param {boolean} on - true=解锁屏幕, false=锁屏
 */
function sendDisplayPowerControl(on) {
    if (!socket || !socket.connected) {
        log('Socket.IO 未连接，无法发送电源控制命令', 'warn');
        return;
    }

    const message = buildDisplayPowerMessage(on);

    // 调试：输出实际发送的数据
    const hexPreview = Array.from(message).map(b => b.toString(16).padStart(2, '0')).join(' ');
    log(`发送电源控制: ${on ? '解锁' : '锁屏'} (hex: ${hexPreview})`, 'info');

    // 发送二进制数据
    socket.emit('scrcpy_ctl', message, (ack) => {
        if (ack) {
            log(`电源控制命令已发送`, 'success');
        }
    });
}

/**
 * 发送文本输入事件
 * @param {string} text - 要输入的文本内容
 */
function sendTextEvent(text) {
    if (!socket || !socket.connected) {
        log('Socket.IO 未连接，无法发送文本', 'warn');
        return;
    }

    if (!text || text.length === 0) {
        return;
    }

    const message = buildTextEvent(text);

    // 调试：输出实际发送的数据
    const hexPreview = Array.from(message)
        .map(b => b.toString(16).padStart(2, '0'))
        .join(' ');
    console.log(`发送文本事件: "${text}" (${text.length} 字符)`);
    console.log(`完整数据hex: ${hexPreview}`);

    // 详细调试：显示每个字符的 Unicode 码点
    const codePoints = Array.from(text).map(c => `U+${c.codePointAt(0).toString(16).toUpperCase().padStart(4, '0')}`).join(' ');
    console.log(`Unicode 码点: ${codePoints}`);

    // 发送二进制数据
    socket.emit('scrcpy_ctl', message, (ack) => {
        if (ack) {
            console.log('文本事件已发送');
        }
    });
}

/**
 * 发送按键事件
 * @param {number} keyCode - Android 按键代码 (KEYCODE_*)
 */
function sendKeyEvent(keyCode) {
    if (!socket || !socket.connected) {
        log('Socket.IO 未连接，无法发送按键事件', 'warn');
        return;
    }

    // 发送按下事件
    const downMessage = buildKeyEvent(KEY_ACTION_DOWN, keyCode);
    const downHex = Array.from(downMessage).map(b => b.toString(16).padStart(2, '0')).join(' ');
    console.log(`按下事件: ${downHex}`);
    socket.emit('scrcpy_ctl', downMessage);

    // 短暂延迟后发送抬起事件
    setTimeout(() => {
        const upMessage = buildKeyEvent(KEY_ACTION_UP, keyCode);
        const upHex = Array.from(upMessage).map(b => b.toString(16).padStart(2, '0')).join(' ');
        console.log(`抬起事件: ${upHex}`);
        socket.emit('scrcpy_ctl', upMessage);
    }, 50);

    console.log(`发送按键事件: KEYCODE=0x${keyCode.toString(16).toUpperCase().padStart(4, '0')} (${keyCode})`);
}

/**
 * 发送触摸事件
 */
function sendTouchEvent(action, x, y) {
    if (!socket || !socket.connected) {
        return;
    }

    const pressure = action === ACTION_UP ? 0.0 : 1.0;
    const actionButton = action === ACTION_UP ? 0 : BUTTON_PRIMARY;
    const buttons = action === ACTION_UP ? 0 : BUTTON_PRIMARY;

    const message = buildTouchEvent(action, POINTER_ID, x, y, pressure, actionButton, buttons);

    // 调试：输出实际发送的数据
    const hexPreview = Array.from(message.slice(0, 32)).map(b => b.toString(16).padStart(2, '0')).join(' ');
    console.log(`发送触摸事件: action=${action}, x=${x}, y=${y}, pressure=${pressure}`);
    console.log(`数据hex: ${hexPreview}`);

    // 发送二进制数据
    socket.emit('scrcpy_ctl', message, (ack) => {
        if (ack) {
            // log(`服务器确认收到事件`, 'info');
        }
    });
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
 * 更新统计信息
 */
function updateStats() {
    // const stats = document.getElementById('stats');
    // stats.textContent = `FPS: ${fps} | 帧数: ${frameCount} | 尺寸: ${canvas.width}x${canvas.height}`;
}

/**
 * 处理视频数据 - scrcpy 协议处理
 */
function handleVideoData(base64Data) {
    try {
        // 解码 base64 数据
        const binaryData = atob(base64Data);
        const uint8Array = new Uint8Array(binaryData.length);

        for (let i = 0; i < binaryData.length; i++) {
            uint8Array[i] = binaryData.charCodeAt(i);
        }

        // 将数据传递给解码器（解码器会处理 scrcpy 协议）
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
 * 注意: WebCodecs 的 VideoFrame 已在 output 回调中直接绘制,此函数仅处理错误和状态显示
 */
function drawFrame(frameData) {
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

// ========== 键盘输入支持 ==========

// 设置 canvas 可以获得焦点
canvas.setAttribute('tabindex', '0');

// 当鼠标悬停在 canvas 上时，键盘输入将发送到设备
canvas.addEventListener('keydown', (e) => {
    e.preventDefault();

    // 按键代码映射 - 需要使用 Android KEYCODE 的按键
    const keyCodeMap = {
        'Backspace': KEYCODE_DEL,           // 0x0043
        'Delete': KEYCODE_FORWARD_DEL,      // 0x0070
        'Enter': KEYCODE_ENTER,             // 0x0042
        'Tab': KEYCODE_TAB,                 // 0x003d
        'Escape': KEYCODE_ESCAPE            // 0x006f
    };

    // 处理特殊按键 - 使用 Android KEYCODE 事件
    if (keyCodeMap[e.key]) {
        const keyCode = keyCodeMap[e.key];
        sendKeyEvent(keyCode);
        console.log(`按键事件: ${e.key} -> KEYCODE 0x${keyCode.toString(16).toUpperCase().padStart(4, '0')}`);
        return;
    }

    // 无法识别的按键
    if (e.key === 'Unidentified') {
        console.warn('未识别的按键事件');
        return;
    }

    // 组合键（Ctrl+C, Ctrl+V 等），暂不处理
    if (e.ctrlKey || e.altKey || e.metaKey) {
        console.log(`组合键被忽略: Ctrl=${e.ctrlKey}, Alt=${e.altKey}, Meta=${e.metaKey}`);
        return;
    }

    // 功能键 F1-F12 等，暂不处理
    if (e.key.startsWith('F') && e.key.length > 1) {
        console.log(`功能键被忽略: ${e.key}`);
        return;
    }

    // 普通字符（单个字符，包括中文等）- 使用文本注入
    if (e.key.length === 1) {
        sendTextEvent(e.key);
        console.log(`文本输入: "${e.key}"`);
    }
});

// canvas 获得焦点时添加视觉反馈
canvas.addEventListener('focus', () => {
    canvas.style.outline = '2px solid #4caf50';
    log('Canvas 已聚焦，键盘输入将发送到设备', 'info');
});

canvas.addEventListener('blur', () => {
    canvas.style.outline = 'none';
    log('Canvas 失去焦点', 'info');
});

// 鼠标悬停在 canvas 上时，自动聚焦
canvas.addEventListener('mouseenter', () => {
    canvas.focus();
});

// 鼠标离开 canvas 时，失去焦点
canvas.addEventListener('mouseleave', () => {
    canvas.blur();
});

// ========== 按钮事件 ==========

// 屏幕电源状态 (true = 亮屏, false = 息屏)
let screenPowerOn = true;

// 视频控制按钮事件
document.getElementById('powerToggleBtn').addEventListener('click', () => {
    // 切换屏幕电源状态
    screenPowerOn = !screenPowerOn;

    const powerBtn = document.getElementById('powerToggleBtn');

    if (screenPowerOn) {
        // 亮屏
        powerBtn.setAttribute('data-tooltip', '点击锁屏');
        sendDisplayPowerControl(true);
    } else {
        // 息屏
        powerBtn.setAttribute('data-tooltip', '点击亮屏');
        sendDisplayPowerControl(false);
    }
});

// 设备管理事件
document.getElementById('refreshDevicesBtn').addEventListener('click', fetchDevices);
document.getElementById('connectDeviceBtn').addEventListener('click', connectToDevice);

// 日志控制事件
document.getElementById('clearLogBtn').addEventListener('click', () => {
    const logContainer = document.getElementById('logContainer');
    // 只删除日志条目，保留控件
    const logEntries = logContainer.querySelectorAll('.log-entry');
    logEntries.forEach(entry => entry.remove());
    log('日志已清空', 'info');
});

// ========== 初始化 ==========

// 页面加载完成后执行
window.addEventListener('DOMContentLoaded', () => {
    log('页面已加载', 'success');
    log('开始初始化...', 'info');

    // 设置初始 canvas 尺寸
    canvas.width = 540;
    canvas.height = 960;

    // 清空画布
    ctx.fillStyle = '#000';
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    // 绘制提示文字
    ctx.fillStyle = '#666';
    ctx.font = '20px sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText('请先连接设备', canvas.width / 2, canvas.height / 2);

    // 自动获取设备列表
    fetchDevices();

    log('初始化完成', 'success');
});
