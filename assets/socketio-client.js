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

// 连接到设备
async function connectToDevice() {
    const deviceSerial = document.getElementById('deviceSelect').value;
    if (!deviceSerial) {
        log('请先选择设备', 'warn');
        return;
    }

    try {
        log(`连接到设备: ${deviceSerial}`, 'info');
        const response = await fetch(`${API_BASE()}/connect`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ serial: deviceSerial })
        });

        if (!response.ok) throw new Error('连接设备失败');

        const data = await response.json();
        log(`设备连接成功, Socket.IO 端口: ${data.data.socketio_port}`, 'success');

        // 更新 socket 端口字段
        document.getElementById('socketPort').value = data.data.socketio_port;
        document.getElementById('deviceStatus').textContent = '已连接';
        document.getElementById('deviceStatus').classList.remove('disconnected');
        document.getElementById('deviceStatus').classList.add('connected');

        return data.socketio_port;
    } catch (error) {
        log(`连接设备失败: ${error.message}`, 'error');
    }
}

// 连接到 Socket.IO
async function connectSocket() {
    const ip = document.getElementById('socketIp').value;
    const port = document.getElementById('socketPort').value;

    if (!port) {
        log('请先连接设备获取 Socket.IO 端口', 'warn');
        return;
    }

    const url = `http://${ip}:${port}`;
    log(`连接到 Socket.IO: ${url}`, 'info');

    // 复用现有的 connect() 函数
    document.getElementById('socketUrl').value = url;
    connect();
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

    updateSocketStatus(false);

    // 清空画布
    ctx.fillStyle = '#000';
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    log('已断开 Socket.IO 连接', 'info');
}

// 更新 Socket.IO 状态
function updateSocketStatus(connected) {
    const socketStatus = document.getElementById('socketStatus');
    const connectBtn = document.getElementById('connectSocketBtn');
    const disconnectBtn = document.getElementById('disconnectSocketBtn');
    const loadingHint = document.getElementById('loadingHint');

    if (connected) {
        socketStatus.textContent = '已连接';
        socketStatus.classList.remove('disconnected');
        socketStatus.classList.add('connected');
        connectBtn.classList.add('hidden');
        disconnectBtn.classList.remove('hidden');
        loadingHint.style.display = 'none';
        log('Socket.IO 已连接', 'success');
    } else {
        socketStatus.textContent = '未连接';
        socketStatus.classList.remove('connected');
        socketStatus.classList.add('disconnected');
        connectBtn.classList.remove('hidden');
        disconnectBtn.classList.add('hidden');
        loadingHint.style.display = 'block';
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

// Scrcpy 控制消息类型
const SCRCPY_MSG_TYPE_INJECT_TOUCH_EVENT = 2;
const SCRCPY_MSG_TYPE_SET_DISPLAY_POWER = 10;

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
            droppedFrames: 0
        };

        // WebCodecs 解码器
        this.videoDecoder = null;
        this.useWebCodecs = false;
        this.pendingFrames = 0;
        this.maxPendingFrames = 10;

        // H.264 Parser - 使用累积buffer方式
        this.h264Buffer = new Uint8Array(0);
        this.sps = null;
        this.pps = null;
        this.hasKeyFrame = false;
        this.loggedSps = false;
        this.loggedPps = false;
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
        // 创建 VideoDecoder 实例 - 参考 demo 直接在 output 回调中绘制
        this.videoDecoder = new VideoDecoder({
            output: (frame) => {
                this.stats.decodedFrames++;
                this.pendingFrames--;

                // 第一帧解码成功的日志
                if (this.stats.decodedFrames === 1) {
                    log(`✅ 第一帧解码成功! visible: ${frame.visibleRect?.width || frame.displayWidth || frame.codedWidth}x${frame.visibleRect?.height || frame.displayHeight || frame.codedHeight}`, 'success');
                }

                // // Log every 30 frames
                // if (this.stats.decodedFrames % 30 === 0) {
                //     log(`解码帧计数: ${this.stats.decodedFrames} - visible: ${frame.visibleRect?.width || frame.displayWidth || frame.codedWidth}x${frame.visibleRect?.height || frame.displayHeight || frame.codedHeight}`, 'info');
                // }

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
                log(`❌ VideoDecoder 错误: ${error.message} (code: ${error.code})`, 'error');
                this.stats.droppedFrames++;
            }
        });

        console.log('WebCodecs VideoDecoder 已创建，等待编解码器元数据...');
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
        if (!this.videoDecoder || this.videoDecoder.state === 'configured') {
            return false;
        }

        try {
            // 调试：输出SPS和PPS的前20字节
            const spsHex = Array.from(sps.slice(0, Math.min(20, sps.length)))
                .map(b => b.toString(16).padStart(2, '0')).join(' ');
            const ppsHex = Array.from(pps.slice(0, Math.min(20, pps.length)))
                .map(b => b.toString(16).padStart(2, '0')).join(' ');
            log(`SPS (前${Math.min(20, sps.length)}字节): ${spsHex}`, 'info');
            log(`PPS (前${Math.min(20, pps.length)}字节): ${ppsHex}`, 'info');

            // Parse SPS to get profile/level/constraint
            // H.264 SPS structure after NAL header: [profile_idc][constraint_set_flags][level_idc]...
            const profile = sps[5];  // profile_idc
            const constraint = sps[6];  // constraint_set_flags
            const level = sps[7];  // level_idc

            log(`SPS解析: profile=${profile}, constraint=${constraint}, level=${level}`, 'info');

            // Format: avc1.PPCCLL (6 hex digits)
            const codecString = `avc1.${profile.toString(16).padStart(2, '0')}${(constraint & 0x3F).toString(16).padStart(2, '0')}${Math.max(level, 1).toString(16).padStart(2, '0')}`;

            // Build proper AVCC description (avcC box format)
            // SPS/PPS data without start codes
            const spsData = sps.slice(4);
            const ppsData = pps.slice(4);

            log(`SPS数据长度=${spsData.length}, PPS数据长度=${ppsData.length}`, 'info');

            // Calculate total size
            const spsLen = spsData.length;
            const ppsLen = ppsData.length;
            const descSize = 6 + 2 + spsLen + 1 + 2 + ppsLen;

            const description = new Uint8Array(descSize);
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

            this.videoDecoder.configure({
                codec: codecString,
                description: description,
                codedWidth: screenWidth,
                codedHeight: screenHeight
            });

            log(`Decoder configured: ${codecString} (${screenWidth}x${screenHeight})`, 'success');
            return true;
        } catch (e) {
            log(`Configure decoder failed: ${e.message}`, 'error');

            // Try fallback with generic codec string
            try {
                this.videoDecoder.configure({
                    codec: 'avc1.64001F',  // Generic H.264 High Profile
                    codedWidth: screenWidth,
                    codedHeight: screenHeight
                });
                log('Decoder configured with fallback codec', 'success');
                return true;
            } catch (e2) {
                log(`Fallback also failed: ${e2.message}`, 'error');
                return false;
            }
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
            const nalType = nalUnit[4] & 0x1F;
            const isKeyFrame = (nalType === 5);

            // Store SPS (type 7) and PPS (type 8)
            if (nalType === 7) {
                this.sps = nalUnit;
                log('H.264 NALU: SPS (7), ' + nalUnit.length + '字节', 'success');
            } else if (nalType === 8) {
                this.pps = nalUnit;
                log('H.264 NALU: PPS (8), ' + nalUnit.length + '字节', 'success');
            } else if (nalType === 5) {
                // IDR frame (key frame)
                if (!this.hasKeyFrame) {
                    this.hasKeyFrame = true;
                    log('Found key frame', 'success');
                }
            }

            // Configure decoder when we have codec config AND this is a key frame
            if (isKeyFrame && this.hasCodecConfig() && this.videoDecoder && this.videoDecoder.state === 'unconfigured') {
                if (this.configureDecoder(this.sps, this.pps)) {
                    log('Decoder configured with key frame', 'success');
                }
            }

            // Only decode video frame NAL units (1-5) when decoder is ready
            // 1: non-IDR slice, 5: IDR slice (key frame)
            if (this.videoDecoder && this.videoDecoder.state === 'configured' && (nalType >= 1 && nalType <= 5)) {
                try {
                    // 检查这个NAL单元内部是否包含多个起始码（多个NALUs合并在一起）
                    // scrcpy有时会将一个帧的多个NALUs打包在一起
                    const subNALUnits = this.extractSubNALUnits(nalUnit);

                    if (this.stats.decodedFrames < 3) {
                        log(`NALU类型=${nalType}, 拆分成${subNALUnits.length}个子NALU`, 'info');
                    }

                    // 将每个子NALU单独解码
                    for (const subNALU of subNALUnits) {
                        const naluData = subNALU; // 已经去掉了起始码
                        const avccData = new Uint8Array(4 + naluData.length);
                        // Big-endian length
                        avccData[0] = (naluData.length >> 24) & 0xFF;
                        avccData[1] = (naluData.length >> 16) & 0xFF;
                        avccData[2] = (naluData.length >> 8) & 0xFF;
                        avccData[3] = naluData.length & 0xFF;
                        avccData.set(naluData, 4);

                        const chunk = new EncodedVideoChunk({
                            type: isKeyFrame ? 'key' : 'delta',
                            timestamp: performance.now() * 1000,
                            data: avccData
                        });

                        this.videoDecoder.decode(chunk);
                    }

                    decoded = true;
                } catch (e) {
                    log(`Decode error: ${e.message}`, 'error');
                }
            } else {
                // 调试:为什么没有解码这个NALU
                if (nalType >= 1 && nalType <= 5) {
                    if (!this.videoDecoder) {
                        log(`NALU ${nalType} 跳过: decoder未创建`, 'warn');
                    } else if (this.videoDecoder.state !== 'configured') {
                        log(`NALU ${nalType} 跳过: decoder状态=${this.videoDecoder.state}`, 'warn');
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

        // 跳过第一个起始码 ( nalUnit[0-3] = 00 00 00 01)
        while (pos < nalUnit.length - 4) {
            // 查找下一个起始码
            if (nalUnit[pos] === 0x00 && nalUnit[pos + 1] === 0x00 &&
                nalUnit[pos + 2] === 0x00 && nalUnit[pos + 3] === 0x01) {
                // 找到起始码
                const start = pos + 4; // 跳过起始码本身

                // 查找这个NALU的结束位置（下一个起始码或数据结束）
                let end = start;
                pos = start;

                while (pos < nalUnit.length - 4) {
                    if (nalUnit[pos] === 0x00 && nalUnit[pos + 1] === 0x00 &&
                        nalUnit[pos + 2] === 0x00 && nalUnit[pos + 3] === 0x01) {
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
            return [nalUnit.slice(4)];
        }

        return subNALUs;
    }

    // Extract NAL units from buffer (generator function)
    *extractNALUnits() {
        let i = 0;
        const buf = this.h264Buffer;

        while (i < buf.length - 4) {
            // Look for NAL start code (0x00 0x00 0x00 0x01)
            if (buf[i] === 0x00 && buf[i + 1] === 0x00 &&
                buf[i + 2] === 0x00 && buf[i + 3] === 0x01) {
                const start = i;
                i += 4;

                // Find next NAL unit (look for next start code)
                let end = buf.length;  // Default to end of buffer
                while (i < buf.length - 4) {
                    if (buf[i] === 0x00 && buf[i + 1] === 0x00 &&
                        buf[i + 2] === 0x00 && buf[i + 3] === 0x01) {
                        end = i;
                        break;
                    }
                    i++;
                }

                const nalUnit = buf.slice(start, end);
                yield nalUnit;
            } else {
                i++;
            }
        }

        // Keep remaining data (incomplete NAL unit)
        this.h264Buffer = buf.slice(i);
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
    // const stats = document.getElementById('stats');
    // stats.textContent = `FPS: ${fps} | 帧数: ${frameCount} | 尺寸: ${canvas.width}x${canvas.height}`;
}

/**
 * 连接到 Socket.IO 服务器
 */
function connect() {
    const url = document.getElementById('socketUrl').value;

    if (!url) {
        log('请输入 Socket.IO URL', 'warn');
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

    log(`正在连接到 Socket.IO: ${url}`, 'info');

    // 创建新连接
    socket = io(url, {
        path: '/socket.io/',
        transports: ['websocket', 'polling']
    });

    socket.on('connect', () => {
        log('Socket.IO 连接成功', 'success');
        updateSocketStatus(true);

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
        updateSocketStatus(false);
    });

    socket.on('disconnect', (reason) => {
        log(`断开连接: ${reason}`, 'warn');
        updateSocketStatus(false);
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

// Socket.IO 连接事件
document.getElementById('connectSocketBtn').addEventListener('click', connectSocket);
document.getElementById('disconnectSocketBtn').addEventListener('click', disconnectSocket);

// 日志控制事件
document.getElementById('clearLogBtn').addEventListener('click', () => {
    document.getElementById('logContainer').innerHTML = '';
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
