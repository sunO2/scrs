/**
 * VideoDecoder - H.264 视频解码器模块
 * 负责解析 scrcpy 协议和使用 WebCodecs API 解码 H.264 视频流
 */

export class VideoDecoder {
    #canvas = null;
    #ctx = null;
    #frameCallback = null;
    #errorCallback = null;

    // Scrcpy 协议解析状态
    #state = 'init'; // init, read_codec_meta, read_frame_head, read_frame_data
    #buffer = [];
    #bufferSize = 0;
    #codecMeta = null;
    #frameHeader = null;
    #remainingFrameBytes = 0;

    // H.264 解码相关
    #videoDecoder = null;
    #useWebCodecs = false;
    #h264Buffer = null;
    #sps = null;
    #pps = null;
    #hasKeyFrame = false;
    #decoderConfigured = false;
    #decoderNeedsKeyFrame = true;
    #actualVideoSize = null;
    #frameIndex = 0;
    #consecutiveErrors = 0;
    #maxConsecutiveErrors = 5;

    // 统计信息
    #stats = {
        totalBytes: 0,
        totalPackets: 0,
        spsCount: 0,
        ppsCount: 0,
        idrCount: 0,
        pFrameCount: 0,
        decodedFrames: 0,
        decodeErrors: 0,
        droppedFrames: 0,
        garbageBytesSkipped: 0
    };

    // 屏幕尺寸 (从编解码器元数据获取)
    #screenWidth = 1080;
    #screenHeight = 1920;

    /**
     * 创建视频解码器实例
     * @param {HTMLCanvasElement} canvas - 用于渲染视频的 Canvas 元素
     * @param {Object} options - 配置选项
     * @param {Function} options.onFrame - 帧解码回调 (可选)
     * @param {Function} options.onError - 错误回调 (可选)
     * @param {boolean} options.enableStats - 是否启用统计 (默认: false)
     */
    constructor(canvas, options = {}) {
        this.#canvas = canvas;
        this.#ctx = canvas.getContext('2d');
        this.#frameCallback = options.onFrame || null;
        this.#errorCallback = options.onError || null;
        this.#h264Buffer = new Uint8Array(0);

        // 设置初始 canvas 尺寸
        canvas.width = 540;
        canvas.height = 960;
    }

    /**
     * 初始化解码器
     * @returns {Promise<boolean>} 是否成功初始化
     */
    async init() {
        // 检查是否支持 WebCodecs API（使用 window.VideoDecoder 避免与类名冲突）
        const hasWebCodecs = typeof window !== 'undefined' &&
                             typeof window.VideoDecoder !== 'undefined';

        if (hasWebCodecs) {
            this.#useWebCodecs = true;
            try {
                await this.#initWebCodecs();
                console.log('[VideoDecoder] Initialized with WebCodecs API');
                return true;
            } catch (e) {
                console.error('[VideoDecoder] Failed to initialize WebCodecs:', e);
                this.#useWebCodecs = false;
            }
        }

        // 即使 WebCodecs 不可用也返回 true（旧版本行为）
        // 数据仍然会被解析，只是无法解码显示
        if (!this.#useWebCodecs) {
            console.warn('[VideoDecoder] WebCodecs API not available');
            console.warn('[VideoDecoder] Video data will be parsed but not decoded');
            console.warn('[VideoDecoder] Please use Chrome 94+ or Edge 94+ for hardware-accelerated decoding');
        }

        return true;
    }

    /**
     * 初始化 WebCodecs VideoDecoder
     * @private
     */
    async #initWebCodecs() {
        // 检查浏览器支持的编解码器（使用 window.VideoDecoder）
        const supportedCodecs = [];
        const testCodecs = [
            'avc1.64001F',  // H.264 High
            'avc1.4D001F',  // H.264 Main
            'avc1.42001F',  // H.264 Baseline
            'avc1.640028',  // H.264 High Level 40
            'avc1.64002A',  // H.264 High Level 42
        ];

        for (const codec of testCodecs) {
            try {
                const support = await window.VideoDecoder.isConfigSupported({
                    codec: codec,
                    codedWidth: 1920,
                    codedHeight: 1080
                });
                if (support.supported) {
                    supportedCodecs.push(codec);
                }
            } catch (e) {
                // Ignore unsupported codecs
            }
        }

        console.log('[VideoDecoder] Supported codecs:', supportedCodecs.join(', '));

        // 创建 VideoDecoder 实例（使用 window.VideoDecoder）
        this.#videoDecoder = new window.VideoDecoder({
            output: (frame) => {
                this.#onFrameDecoded(frame);
            },
            error: (error) => {
                this.#onDecodeError(error);
            }
        });

        console.log('[VideoDecoder] WebCodecs VideoDecoder created');
    }

    /**
     * 解码数据
     * @param {Uint8Array} data - 原始数据 (scrcpy 协议格式)
     */
    decode(data) {
        try {
            // 将新数据追加到缓冲区
            this.#buffer.push(new Uint8Array(data));
            this.#bufferSize += data.length;
            this.#stats.totalBytes += data.length;

            // 合并缓冲区
            const combined = new Uint8Array(this.#bufferSize);
            let offset = 0;
            for (const chunk of this.#buffer) {
                combined.set(chunk, offset);
                offset += chunk.length;
            }

            let parseOffset = 0;

            // 解析 scrcpy 协议
            while (parseOffset < combined.length) {
                switch (this.#state) {
                    case 'init':
                    case 'read_codec_meta':
                        // 需要读取 12 字节编解码器元数据
                        if (combined.length - parseOffset < 12) {
                            this.#buffer = [combined.slice(parseOffset)];
                            this.#bufferSize = combined.length - parseOffset;
                            return;
                        }

                        // 读取 12 字节编解码器元数据
                        this.#codecMeta = this.#parseCodecMeta(combined, parseOffset);
                        console.log('[VideoDecoder] Codec meta:', this.#codecMeta);

                        // 更新视频尺寸
                        this.#screenWidth = this.#codecMeta.width;
                        this.#screenHeight = this.#codecMeta.height;

                        parseOffset += 12;
                        this.#state = 'read_frame_head';
                        break;

                    case 'read_frame_head':
                        // 需要读取 12 字节帧头
                        if (combined.length - parseOffset < 12) {
                            this.#buffer = [combined.slice(parseOffset)];
                            this.#bufferSize = combined.length - parseOffset;
                            return;
                        }

                        // 读取 12 字节帧头
                        this.#frameHeader = this.#parseFrameHeader(combined, parseOffset);
                        this.#remainingFrameBytes = this.#frameHeader.packetSize;

                        if (this.#frameHeader.packetSize === 0) {
                            // 空帧，跳过
                            parseOffset += 12;
                            break;
                        }

                        parseOffset += 12;
                        this.#state = 'read_frame_data';
                        break;

                    case 'read_frame_data':
                        // 检查是否有足够的帧数据
                        if (combined.length - parseOffset < this.#remainingFrameBytes) {
                            this.#buffer = [combined.slice(parseOffset)];
                            this.#bufferSize = combined.length - parseOffset;
                            return;
                        }

                        // 提取完整的 H.264 帧数据
                        const frameData = combined.slice(parseOffset, parseOffset + this.#remainingFrameBytes);
                        parseOffset += this.#remainingFrameBytes;

                        // 处理 H.264 帧
                        this.#processH264FrameData(frameData);

                        this.#state = 'read_frame_head';
                        this.#frameHeader = null;
                        this.#remainingFrameBytes = 0;
                        break;
                }
            }

            // 清空缓冲区
            this.#buffer = [];
            this.#bufferSize = 0;

        } catch (e) {
            console.error('[VideoDecoder] Decode error:', e);
            this.#stats.decodeErrors++;
            if (this.#errorCallback) {
                this.#errorCallback(e);
            }
            // 清空缓冲区以恢复
            this.#buffer = [];
            this.#bufferSize = 0;
            this.#state = 'read_frame_head';
        }
    }

    /**
     * 处理 H.264 帧数据
     * @private
     * @param {Uint8Array} frameData - H.264 帧数据
     */
    #processH264FrameData(frameData) {
        // 将数据追加到 H.264 缓冲区
        const newBuffer = new Uint8Array(this.#h264Buffer.length + frameData.length);
        newBuffer.set(this.#h264Buffer);
        newBuffer.set(frameData, this.#h264Buffer.length);
        this.#h264Buffer = newBuffer;

        // 提取并处理 NAL 单元
        for (const nalUnit of this.#extractNALUnits()) {
            // 获取 NAL 类型
            let nalHeaderOffset = 0;
            if (nalUnit.length >= 4 && nalUnit[0] === 0x00 && nalUnit[1] === 0x00 &&
                nalUnit[2] === 0x00 && nalUnit[3] === 0x01) {
                nalHeaderOffset = 4;
            } else if (nalUnit.length >= 3 && nalUnit[0] === 0x00 && nalUnit[1] === 0x00 &&
                       nalUnit[2] === 0x01) {
                nalHeaderOffset = 3;
            }

            if (nalHeaderOffset === 0 || nalUnit.length <= nalHeaderOffset) {
                continue;
            }

            const nalType = nalUnit[nalHeaderOffset] & 0x1F;
            const isKeyFrame = (nalType === 5);

            // 存储 SPS (type 7) 和 PPS (type 8)
            if (nalType === 7) {
                this.#sps = nalUnit;
                this.#stats.spsCount++;
                console.log('[VideoDecoder] Found SPS');

                // 尝试配置解码器（仅在 WebCodecs 可用时）
                if (this.#useWebCodecs && this.#hasCodecConfig() && !this.#decoderConfigured &&
                    this.#videoDecoder && this.#videoDecoder.state === 'unconfigured') {
                    this.#configureDecoder(this.#sps, this.#pps);
                }
            } else if (nalType === 8) {
                this.#pps = nalUnit;
                this.#stats.ppsCount++;
                console.log('[VideoDecoder] Found PPS');

                // 尝试配置解码器（仅在 WebCodecs 可用时）
                if (this.#useWebCodecs && this.#hasCodecConfig() && !this.#decoderConfigured &&
                    this.#videoDecoder && this.#videoDecoder.state === 'unconfigured') {
                    this.#configureDecoder(this.#sps, this.#pps);
                }
            } else if (nalType === 5) {
                // IDR frame (key frame)
                if (!this.#hasKeyFrame) {
                    this.#hasKeyFrame = true;
                    this.#stats.idrCount++;
                    console.log('[VideoDecoder] Found key frame');
                }

                // 尝试配置解码器（仅在 WebCodecs 可用时）
                if (this.#useWebCodecs && this.#hasCodecConfig() && !this.#decoderConfigured &&
                    this.#videoDecoder && this.#videoDecoder.state === 'unconfigured') {
                    this.#configureDecoder(this.#sps, this.#pps);
                }
            } else if (nalType === 1) {
                this.#stats.pFrameCount++;
            }

            // 等待关键帧
            if (this.#decoderNeedsKeyFrame && !isKeyFrame) {
                continue;
            }

            // 解码视频帧 NAL 单元 (1-5)（仅在 WebCodecs 可用时）
            if (this.#useWebCodecs && this.#videoDecoder && this.#videoDecoder.state === 'configured' &&
                (nalType >= 1 && nalType <= 5)) {
                try {
                    let chunkData = nalUnit;

                    // 第一个关键帧需要附加 SPS 和 PPS
                    if (isKeyFrame && this.#stats.decodedFrames === 0 && this.#sps && this.#pps) {
                        const totalSize = this.#sps.length + this.#pps.length + nalUnit.length;
                        chunkData = new Uint8Array(totalSize);
                        let offset = 0;
                        chunkData.set(this.#sps, offset);
                        offset += this.#sps.length;
                        chunkData.set(this.#pps, offset);
                        offset += this.#pps.length;
                        chunkData.set(nalUnit, offset);
                    }

                    // 使用递增的时间戳
                    this.#frameIndex++;
                    const timestamp = this.#frameIndex * 33333; // ~30fps

                    const chunk = new EncodedVideoChunk({
                        type: isKeyFrame ? 'key' : 'delta',
                        timestamp: timestamp,
                        data: chunkData
                    });

                    this.#videoDecoder.decode(chunk);

                    // 清除"需要关键帧"标志
                    if (isKeyFrame && this.#decoderNeedsKeyFrame) {
                        this.#decoderNeedsKeyFrame = false;
                    }

                } catch (e) {
                    console.error('[VideoDecoder] Decode error:', e);
                    this.#stats.decodeErrors++;

                    // 如果错误是"需要关键帧"，重新设置标志
                    if (e.message.includes('key frame') || e.message.includes('keyframe')) {
                        this.#decoderNeedsKeyFrame = true;
                    }

                    if (this.#errorCallback) {
                        this.#errorCallback(e);
                    }
                }
            }
        }

        this.#stats.totalPackets++;
    }

    /**
     * 从缓冲区提取 NAL 单元
     * @private
     * @generator
     * @yields {Uint8Array} NAL 单元
     */
    *#extractNALUnits() {
        let i = 0;
        const buf = this.#h264Buffer;

        // 辅助函数：检查起始码
        const isStartCode3 = (idx) => {
            return idx >= 0 && idx <= buf.length - 3 &&
                   buf[idx] === 0x00 && buf[idx + 1] === 0x00 && buf[idx + 2] === 0x01;
        };

        const isStartCode4 = (idx) => {
            return idx >= 0 && idx <= buf.length - 4 &&
                   buf[idx] === 0x00 && buf[idx + 1] === 0x00 &&
                   buf[idx + 2] === 0x00 && buf[idx + 3] === 0x01;
        };

        // 跳过起始码之前的垃圾数据
        while (i < buf.length - 3) {
            if (isStartCode3(i) || isStartCode4(i)) {
                break;
            }
            i++;
        }

        // 提取 NAL 单元
        while (i < buf.length - 3) {
            const startCodeLen = isStartCode4(i) ? 4 : (isStartCode3(i) ? 3 : 0);

            if (startCodeLen > 0) {
                const start = i;
                i += startCodeLen;

                // 查找下一个 NAL 单元
                let end = buf.length;
                while (i < buf.length - 3) {
                    if (isStartCode3(i) || isStartCode4(i)) {
                        end = i;
                        break;
                    }
                    i++;
                }

                yield buf.slice(start, end);
            } else {
                i++;
            }
        }

        // 保留剩余数据 (不完整的 NAL 单元)
        this.#h264Buffer = buf.slice(i);
    }

    /**
     * 配置解码器
     * @private
     * @param {Uint8Array} sps - SPS 数据
     * @param {Uint8Array} pps - PPS 数据
     */
    #configureDecoder(sps, pps) {
        if (!this.#useWebCodecs || !this.#videoDecoder || this.#decoderConfigured) {
            return;
        }

        try {
            // 去除起始码
            const stripStartCode = (data) => {
                if (data.length >= 4 && data[0] === 0x00 && data[1] === 0x00 &&
                    data[2] === 0x00 && data[3] === 0x01) {
                    return { data: data.slice(4), startCodeLen: 4 };
                } else if (data.length >= 3 && data[0] === 0x00 && data[1] === 0x00 &&
                           data[2] === 0x01) {
                    return { data: data.slice(3), startCodeLen: 3 };
                }
                return { data: data, startCodeLen: 0 };
            };

            const spsResult = stripStartCode(sps);
            const ppsResult = stripStartCode(pps);

            // 解析 SPS 获取编解码器信息
            if (spsResult.data.length < 4) {
                console.error('[VideoDecoder] SPS data too short');
                return;
            }

            const profile = spsResult.data[1];
            const constraint = spsResult.data[2];
            const level = spsResult.data[3];

            // 生成 codec 字符串
            const codecString = `avc1.${profile.toString(16).padStart(2, '0')}${(constraint & 0x3F).toString(16).padStart(2, '0')}${Math.max(level, 1).toString(16).padStart(2, '0')}`;

            console.log('[VideoDecoder] Configuring with codec:', codecString);

            // 尝试配置解码器
            const configWidth = this.#actualVideoSize?.width || this.#screenWidth;
            const configHeight = this.#actualVideoSize?.height || this.#screenHeight;

            this.#videoDecoder.configure({
                codec: codecString,
                codedWidth: configWidth,
                codedHeight: configHeight
            });

            this.#decoderConfigured = true;
            this.#decoderNeedsKeyFrame = true;
            console.log('[VideoDecoder] Decoder configured:', codecString, `${configWidth}x${configHeight}`);

        } catch (e) {
            console.error('[VideoDecoder] Failed to configure decoder:', e);
            if (this.#errorCallback) {
                this.#errorCallback(e);
            }
        }
    }

    /**
     * 帧解码成功回调
     * @private
     * @param {VideoFrame} frame - 解码后的视频帧
     */
    #onFrameDecoded(frame) {
        this.#stats.decodedFrames++;
        this.#consecutiveErrors = 0;

        // 第一帧解码成功
        if (this.#stats.decodedFrames === 1) {
            const actualWidth = frame.visibleRect?.width || frame.displayWidth || frame.codedWidth;
            const actualHeight = frame.visibleRect?.height || frame.displayHeight || frame.codedHeight;
            console.log('[VideoDecoder] First frame decoded!', actualWidth, 'x', actualHeight);

            // 记录实际视频尺寸
            if (!this.#actualVideoSize) {
                this.#actualVideoSize = { width: actualWidth, height: actualHeight };
            }
        }

        // 获取可见区域
        const visibleWidth = frame.visibleRect?.width || frame.displayWidth || frame.codedWidth;
        const visibleHeight = frame.visibleRect?.height || frame.displayHeight || frame.codedHeight;
        const offsetX = frame.visibleRect?.x || 0;
        const offsetY = frame.visibleRect?.y || 0;

        // 设置 canvas 尺寸
        if (this.#canvas.width !== visibleWidth || this.#canvas.height !== visibleHeight) {
            this.#canvas.width = visibleWidth;
            this.#canvas.height = visibleHeight;
        }

        // 绘制到 canvas
        this.#ctx.drawImage(frame, offsetX, offsetY, visibleWidth, visibleHeight,
                           0, 0, visibleWidth, visibleHeight);

        // 立即释放 frame 资源
        frame.close();

        // 触发帧回调
        if (this.#frameCallback) {
            this.#frameCallback({
                width: visibleWidth,
                height: visibleHeight,
                stats: this.#stats
            });
        }
    }

    /**
     * 解码错误回调
     * @private
     * @param {Error} error - 错误信息
     */
    #onDecodeError(error) {
        this.#stats.decodeErrors++;
        this.#stats.droppedFrames++;
        this.#consecutiveErrors++;

        console.error('[VideoDecoder] Decode error:', error);

        // 连续错误过多时尝试重置解码器
        if (this.#consecutiveErrors >= this.#maxConsecutiveErrors) {
            console.warn('[VideoDecoder] Too many consecutive errors, resetting...');
            this.#resetDecoder();
        }

        if (this.#errorCallback) {
            this.#errorCallback(error);
        }
    }

    /**
     * 重置解码器
     * @private
     */
    #resetDecoder() {
        if (this.#videoDecoder) {
            try {
                if (this.#videoDecoder.state === 'configured') {
                    this.#videoDecoder.close();
                }
            } catch (e) {
                console.warn('[VideoDecoder] Error closing decoder:', e);
            }
        }

        this.#decoderConfigured = false;
        this.#decoderNeedsKeyFrame = true;
        this.#consecutiveErrors = 0;
        this.#sps = null;
        this.#pps = null;
        this.#hasKeyFrame = false;

        console.log('[VideoDecoder] Decoder reset');
    }

    /**
     * 检查是否有编解码器配置 (SPS 和 PPS)
     * @private
     * @returns {boolean}
     */
    #hasCodecConfig() {
        return this.#sps !== null && this.#pps !== null;
    }

    /**
     * 解析编解码器元数据
     * @private
     * @param {Uint8Array} data - 数据
     * @param {number} offset - 偏移量
     * @returns {Object} 编解码器元数据
     */
    #parseCodecMeta(data, offset) {
        const view = new DataView(data.slice(offset, offset + 12).buffer);
        const codecId = view.getUint32(0, false);  // big-endian
        const width = view.getUint32(4, false);    // big-endian
        const height = view.getUint32(8, false);   // big-endian

        return { codecId, width, height };
    }

    /**
     * 解析帧头
     * @private
     * @param {Uint8Array} data - 数据
     * @param {number} offset - 偏移量
     * @returns {Object} 帧头
     */
    #parseFrameHeader(data, offset) {
        const headerData = data.slice(offset, offset + 12);
        const view = new DataView(headerData.buffer);

        // 读取 packet_size
        const packetSizeLittle = view.getUint32(8, true);   // little-endian
        const packetSizeBig = view.getUint32(8, false);     // big-endian
        const packetSize = (packetSizeBig > 0 && packetSizeBig < 10000000) ? packetSizeBig : packetSizeLittle;

        // 读取标志位
        const byte7 = headerData[7];
        const configPacket = (byte7 & 0x80) !== 0;
        const keyFrame = (byte7 & 0x40) !== 0;

        // 读取 PTS
        let pts = 0;
        for (let i = 0; i < 8; i++) {
            if (i < 7) {
                pts = (pts << 8) | headerData[i];
            } else {
                pts = (pts << 6) | (headerData[7] & 0x3F);
            }
        }

        return { configPacket, keyFrame, pts, packetSize };
    }

    /**
     * 获取统计信息
     * @returns {Object} 统计信息
     */
    getStats() {
        return { ...this.#stats };
    }

    /**
     * 获取视频尺寸
     * @returns {Object} { width, height }
     */
    getVideoSize() {
        return {
            width: this.#screenWidth,
            height: this.#screenHeight
        };
    }

    /**
     * 清空画布
     */
    clearCanvas() {
        this.#ctx.fillStyle = '#000';
        this.#ctx.fillRect(0, 0, this.#canvas.width, this.#canvas.height);
    }

    /**
     * 销毁解码器，释放资源
     */
    destroy() {
        if (this.#videoDecoder) {
            if (this.#videoDecoder.state === 'configured') {
                this.#videoDecoder.close();
            }
            this.#videoDecoder = null;
        }

        this.#buffer = [];
        this.#h264Buffer = new Uint8Array(0);
        this.#sps = null;
        this.#pps = null;
        this.#decoderConfigured = false;
        this.#frameCallback = null;
        this.#errorCallback = null;

        console.log('[VideoDecoder] Destroyed');
        console.log('[VideoDecoder] Final stats:', this.#stats);
    }
}
