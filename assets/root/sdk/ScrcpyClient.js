/**
 * ScrcpyClient - 完整的 Scrcpy 客户端
 * 整合 Socket 连接和视频解码，提供高级 API
 */

import { ScrcpySocket } from './ScrcpySocket.js';
import { VideoDecoder } from './VideoDecoder.js';

// Scrcpy 控制消息类型
const SCRCPY_MSG_TYPE_INJECT_KEYCODE = 0;
const SCRCPY_MSG_TYPE_INJECT_TEXT = 1;
const SCRCPY_MSG_TYPE_INJECT_TOUCH_EVENT = 2;
const SCRCPY_MSG_TYPE_SET_DISPLAY_POWER = 10;

// Android 按键动作
const KEY_ACTION_DOWN = 0;
const KEY_ACTION_UP = 1;

// Android MotionEvent 动作类型
const ACTION_DOWN = 0;
const ACTION_UP = 1;
const ACTION_MOVE = 2;
const ACTION_CANCEL = 3;

// Android MotionEvent 按钮常量
const BUTTON_PRIMARY = 1;

// Android 按键代码
const KEYCODE_DEL = 0x0043;
const KEYCODE_FORWARD_DEL = 0x0070;
const KEYCODE_ENTER = 0x0042;
const KEYCODE_TAB = 0x003d;
const KEYCODE_ESCAPE = 0x006f;

// 默认按键映射
const DEFAULT_KEY_MAP = {
    'Backspace': KEYCODE_DEL,
    'Delete': KEYCODE_FORWARD_DEL,
    'Enter': KEYCODE_ENTER,
    'Tab': KEYCODE_TAB,
    'Escape': KEYCODE_ESCAPE
};

export class ScrcpyClient {
    // 静态常量 - 可通过 ScrcpyClient.Constants 或 client.Constants 访问
    static Constants = {
        SCRCPY_MSG_TYPE_INJECT_KEYCODE,
        SCRCPY_MSG_TYPE_INJECT_TEXT,
        SCRCPY_MSG_TYPE_INJECT_TOUCH_EVENT,
        SCRCPY_MSG_TYPE_SET_DISPLAY_POWER,
        KEY_ACTION_DOWN,
        KEY_ACTION_UP,
        ACTION_DOWN,
        ACTION_UP,
        ACTION_MOVE,
        ACTION_CANCEL,
        BUTTON_PRIMARY,
        KEYCODE_DEL,
        KEYCODE_FORWARD_DEL,
        KEYCODE_ENTER,
        KEYCODE_TAB,
        KEYCODE_ESCAPE
    };

    #socket = null;
    #decoder = null;
    #canvas = null;
    #config = null;
    #eventHandlers = new Map();
    #isConnected = false;
    #screenSize = { width: 1080, height: 1920 };
    #pointerId = 0n;

    /**
     * 创建 Scrcpy 客户端实例
     * @param {Object} config - 配置选项
     * @param {HTMLCanvasElement} config.canvas - 用于渲染视频的 Canvas 元素
     * @param {Function} [config.onConnected] - 连接成功回调
     * @param {Function} [config.onDisconnected] - 断开连接回调
     * @param {Function} [config.onError] - 错误回调
     * @param {Function} [config.onFrame] - 帧解码回调
     * @param {Function} [config.onLog] - 日志回调
     * @param {Object} [config.keyMap] - 自定义按键映射
     * @param {BigInt} [config.pointerId] - 触摸点 ID (默认: 0n)
     */
    constructor(config) {
        if (!config.canvas) {
            throw new Error('Canvas element is required');
        }

        this.#canvas = config.canvas;
        this.#config = {
            keyMap: DEFAULT_KEY_MAP,
            pointerId: 0n,
            ...config
        };

        this.#pointerId = this.#config.pointerId;

        // 为实例提供 Constants 属性（方便访问）
        this.Constants = ScrcpyClient.Constants;

        // 设置内部事件处理器
        this.#setupInternalHandlers();
    }

    /**
     * 连接到设备
     * @param {string} deviceSerial - 设备序列号
     * @param {number} socketPort - Socket.IO 端口
     * @returns {Promise<void>}
     */
    async connect(deviceSerial, socketPort) {
        if (this.#isConnected) {
            this.#log('Already connected', 'warn');
            return;
        }

        try {
            this.#log(`Connecting to device: ${deviceSerial}`, 'info');

            // 创建 Socket 连接
            const socketUrl = `http://127.0.0.1:${socketPort}`;
            this.#socket = new ScrcpySocket(socketUrl, {
                onConnect: () => this.#onSocketConnect(),
                onDisconnect: (reason) => this.#onSocketDisconnect(reason),
                onError: (err) => this.#onSocketError(err),
                onVideoData: (data) => this.#onVideoData(data),
                onDeviceMeta: (meta) => this.#onDeviceMeta(meta),
                onControlAck: () => this.#onControlAck(),
                onControlError: (err) => this.#onControlError(err)
            });

            await this.#socket.connect();

            this.#log('Connection initiated', 'success');

        } catch (error) {
            this.#log(`Connection failed: ${error.message}`, 'error');
            this.#emit('error', error);
            throw error;
        }
    }

    /**
     * 断开连接
     */
    disconnect() {
        if (this.#decoder) {
            this.#decoder.destroy();
            this.#decoder = null;
        }

        if (this.#socket) {
            this.#socket.disconnect();
            this.#socket = null;
        }

        this.#isConnected = false;
        this.#log('Disconnected', 'info');
    }

    /**
     * 发送触摸事件
     * @param {number} action - 动作类型 (ACTION_DOWN, ACTION_UP, ACTION_MOVE, ACTION_CANCEL)
     * @param {number} x - X 坐标 (设备坐标)
     * @param {number} y - Y 坐标 (设备坐标)
     * @param {number} [pressure] - 压力值 (0.0-1.0, 可选)
     * @returns {boolean} 是否成功发送
     */
    sendTouch(action, x, y, pressure) {
        if (!this.#isConnected) {
            this.#log('Not connected, cannot send touch', 'warn');
            return false;
        }

        const pressureValue = pressure !== undefined ? pressure : (action === ACTION_UP ? 0.0 : 1.0);
        const actionButton = action === ACTION_UP ? 0 : BUTTON_PRIMARY;
        const buttons = action === ACTION_UP ? 0 : BUTTON_PRIMARY;

        const message = this.#buildTouchEvent(action, this.#pointerId, x, y, pressureValue, actionButton, buttons);
        const result = this.#socket.sendControl(message);

        // 调试日志
        const actionNames = { 0: 'DOWN', 1: 'UP', 2: 'MOVE', 3: 'CANCEL' };
        console.log(`[ScrcpyClient] sendTouch: action=${actionNames[action] || action}, x=${x}, y=${y}, result=${result}`);

        return result;
    }

    /**
     * 发送触摸事件 (Canvas 坐标)
     * @param {number} action - 动作类型
     * @param {number} canvasX - Canvas X 坐标
     * @param {number} canvasY - Canvas Y 坐标
     * @returns {boolean}
     */
    sendTouchByCanvasCoords(action, canvasX, canvasY) {
        const deviceCoords = this.#canvasToDeviceCoords(canvasX, canvasY);
        return this.sendTouch(action, deviceCoords.x, deviceCoords.y);
    }

    /**
     * 发送按键事件
     * @param {number} keyCode - Android 按键代码 (KEYCODE_*)
     * @returns {boolean}
     */
    sendKey(keyCode) {
        if (!this.#isConnected) {
            this.#log('Not connected, cannot send key', 'warn');
            return false;
        }

        // 发送按下事件
        const downMessage = this.#buildKeyEvent(KEY_ACTION_DOWN, keyCode);
        this.#socket.sendControl(downMessage);

        // 短暂延迟后发送抬起事件
        setTimeout(() => {
            const upMessage = this.#buildKeyEvent(KEY_ACTION_UP, keyCode);
            this.#socket.sendControl(upMessage);
        }, 50);

        return true;
    }

    /**
     * 发送按键事件 (按键名称)
     * @param {string} keyName - 按键名称 (如: 'Enter', 'Backspace')
     * @returns {boolean}
     */
    sendKeyByName(keyName) {
        const keyCode = this.#config.keyMap[keyName];
        if (keyCode === undefined) {
            this.#log(`Unknown key: ${keyName}`, 'warn');
            return false;
        }
        return this.sendKey(keyCode);
    }

    /**
     * 发送文本输入
     * @param {string} text - 要输入的文本
     * @returns {boolean}
     */
    sendText(text) {
        if (!this.#isConnected) {
            this.#log('Not connected, cannot send text', 'warn');
            return false;
        }

        if (!text || text.length === 0) {
            return false;
        }

        const message = this.#buildTextEvent(text);
        return this.#socket.sendControl(message);
    }

    /**
     * 设置屏幕电源状态
     * @param {boolean} on - true=解锁/亮屏, false=锁屏/息屏
     * @returns {boolean}
     */
    setPower(on) {
        if (!this.#isConnected) {
            this.#log('Not connected, cannot set power', 'warn');
            return false;
        }

        const message = this.#buildDisplayPowerMessage(on);
        return this.#socket.sendControl(message);
    }

    /**
     * 获取连接状态
     * @returns {boolean}
     */
    isConnected() {
        return this.#isConnected;
    }

    /**
     * 获取屏幕尺寸
     * @returns {Object} { width, height }
     */
    getScreenSize() {
        return { ...this.#screenSize };
    }

    /**
     * 获取解码器统计信息
     * @returns {Object}
     */
    getStats() {
        if (this.#decoder) {
            return this.#decoder.getStats();
        }
        return null;
    }

    /**
     * 注册事件监听器
     * @param {string} event - 事件名称 ('connected', 'disconnected', 'error', 'frame')
     * @param {Function} callback - 回调函数
     */
    on(event, callback) {
        if (!this.#eventHandlers.has(event)) {
            this.#eventHandlers.set(event, []);
        }
        this.#eventHandlers.get(event).push(callback);
    }

    /**
     * 移除事件监听器
     * @param {string} event - 事件名称
     * @param {Function} callback - 回调函数
     */
    off(event, callback) {
        if (!this.#eventHandlers.has(event)) {
            return;
        }

        const handlers = this.#eventHandlers.get(event);
        const index = handlers.indexOf(callback);
        if (index !== -1) {
            handlers.splice(index, 1);
        }
    }

    /**
     * 销毁客户端
     */
    destroy() {
        this.disconnect();
        this.#eventHandlers.clear();
        this.#canvas = null;
        this.#config = null;
    }

    // ========== 私有方法 ==========

    /**
     * 设置内部事件处理器
     * @private
     */
    #setupInternalHandlers() {
        if (this.#config.onConnected) {
            this.on('connected', this.#config.onConnected);
        }
        if (this.#config.onDisconnected) {
            this.on('disconnected', this.#config.onDisconnected);
        }
        if (this.#config.onError) {
            this.on('error', this.#config.onError);
        }
        if (this.#config.onFrame) {
            this.on('frame', this.#config.onFrame);
        }
    }

    /**
     * Socket 连接成功回调
     * @private
     */
    #onSocketConnect() {
        this.#isConnected = true;
        this.#log('Socket connected', 'success');
        this.#emit('connected');
    }

    /**
     * Socket 断开连接回调
     * @private
     */
    #onSocketDisconnect(reason) {
        this.#isConnected = false;
        this.#log(`Socket disconnected: ${reason}`, 'warn');
        this.#emit('disconnected', reason);
    }

    /**
     * Socket 错误回调
     * @private
     */
    #onSocketError(err) {
        this.#log(`Socket error: ${err.message}`, 'error');
        this.#emit('error', err);
    }

    /**
     * 接收视频数据回调
     * @private
     */
    #onVideoData(base64Data) {
        if (!this.#decoder) {
            return;
        }

        try {
            // 解码 base64 数据
            const binaryData = atob(base64Data);
            const uint8Array = new Uint8Array(binaryData.length);

            for (let i = 0; i < binaryData.length; i++) {
                uint8Array[i] = binaryData.charCodeAt(i);
            }

            // 传递给解码器
            this.#decoder.decode(uint8Array);

        } catch (e) {
            this.#log(`Video data error: ${e.message}`, 'error');
        }
    }

    /**
     * 接收设备元数据回调
     * @private
     */
    #onDeviceMeta(deviceName) {
        this.#log(`Device metadata: ${deviceName}`, 'info');

        // 重置解码器
        if (this.#decoder) {
            this.#decoder.destroy();
        }

        // 创建新解码器
        this.#decoder = new VideoDecoder(this.#canvas, {
            onFrame: (frameData) => this.#onFrameDecoded(frameData),
            onError: (error) => this.#onDecoderError(error)
        });

        this.#decoder.init();

        // 解码器会在解析编解码器元数据后更新尺寸
        // 我们需要等待第一帧来获取实际的屏幕尺寸
        this.#log('Waiting for video stream to get screen size...', 'info');
    }

    /**
     * 控制确认回调
     * @private
     */
    #onControlAck() {
        // 可以在这里添加控制确认的处理逻辑
    }

    /**
     * 控制错误回调
     * @private
     */
    #onControlError(err) {
        this.#log(`Control error: ${err.error}`, 'error');
    }

    /**
     * 帧解码成功回调
     * @private
     */
    #onFrameDecoded(frameData) {
        // 更新屏幕尺寸（从解码器获取实际尺寸）
        if (this.#decoder) {
            const videoSize = this.#decoder.getVideoSize();
            if (videoSize.width !== this.#screenSize.width || videoSize.height !== this.#screenSize.height) {
                this.#screenSize = videoSize;
                this.#log(`Screen size updated: ${videoSize.width}x${videoSize.height}`, 'info');
                console.log(`[ScrcpyClient] Screen size updated to:`, videoSize);
            }
        }

        this.#emit('frame', frameData);
    }

    /**
     * 解码器错误回调
     * @private
     */
    #onDecoderError(error) {
        this.#log(`Decoder error: ${error.message}`, 'error');
        this.#emit('error', error);
    }

    /**
     * 将 Canvas 坐标转换为设备坐标
     * @private
     */
    #canvasToDeviceCoords(canvasX, canvasY) {
        const rect = this.#canvas.getBoundingClientRect();
        const displayWidth = rect.width;
        const displayHeight = rect.height;

        const internalWidth = this.#canvas.width;
        const internalHeight = this.#canvas.height;

        // 转换到内部分辨率
        const scaleToInternal = internalWidth / displayWidth;
        const internalX = canvasX * scaleToInternal;
        const internalY = canvasY * scaleToInternal;

        // 转换到设备坐标
        const scaleX = this.#screenSize.width / internalWidth;
        const scaleY = this.#screenSize.height / internalHeight;

        return {
            x: Math.floor(internalX * scaleX),
            y: Math.floor(internalY * scaleY)
        };
    }

    /**
     * 构建触摸事件消息
     * @private
     */
    #buildTouchEvent(action, pointerId, x, y, pressure, actionButton, buttons) {
        const buffer = new ArrayBuffer(32);
        const view = new DataView(buffer);

        let offset = 0;

        view.setUint8(offset, SCRCPY_MSG_TYPE_INJECT_TOUCH_EVENT);
        offset += 1;

        view.setUint8(offset, action);
        offset += 1;

        view.setBigUint64(offset, pointerId, false);
        offset += 8;

        view.setInt32(offset, x, false);
        offset += 4;

        view.setInt32(offset, y, false);
        offset += 4;

        view.setUint16(offset, this.#screenSize.width, false);
        offset += 2;

        view.setUint16(offset, this.#screenSize.height, false);
        offset += 2;

        view.setUint16(offset, this.#floatToU16FixedPoint(pressure), false);
        offset += 2;

        view.setInt32(offset, actionButton, false);
        offset += 4;

        view.setInt32(offset, buttons, false);

        return new Uint8Array(buffer);
    }

    /**
     * 构建按键事件消息
     * @private
     */
    #buildKeyEvent(action, keyCode) {
        const buffer = new ArrayBuffer(14);
        const view = new DataView(buffer);

        let offset = 0;

        view.setUint8(offset, SCRCPY_MSG_TYPE_INJECT_KEYCODE);
        offset += 1;

        view.setUint8(offset, action);
        offset += 1;

        view.setUint32(offset, keyCode, false);
        offset += 4;

        view.setUint32(offset, 0, false);
        offset += 4;

        view.setUint32(offset, 0, false);

        return new Uint8Array(buffer);
    }

    /**
     * 构建文本输入事件消息
     * @private
     */
    #buildTextEvent(text) {
        const encoder = new TextEncoder();
        const textBytes = encoder.encode(text);

        const totalLength = 1 + 4 + textBytes.length;
        const buffer = new ArrayBuffer(totalLength);
        const view = new DataView(buffer);

        let offset = 0;

        view.setUint8(offset, SCRCPY_MSG_TYPE_INJECT_TEXT);
        offset += 1;

        view.setUint32(offset, textBytes.length, false);
        offset += 4;

        const uint8Array = new Uint8Array(buffer);
        uint8Array.set(textBytes, offset);

        return uint8Array;
    }

    /**
     * 构建电源控制消息
     * @private
     */
    #buildDisplayPowerMessage(on) {
        const buffer = new ArrayBuffer(2);
        const view = new DataView(buffer);

        view.setUint8(0, SCRCPY_MSG_TYPE_SET_DISPLAY_POWER);
        view.setUint8(1, on ? 1 : 0);

        return new Uint8Array(buffer);
    }

    /**
     * 将浮点数转换为 u16 固定点数
     * @private
     */
    #floatToU16FixedPoint(value) {
        return Math.floor(value * 65535);
    }

    /**
     * 触发事件
     * @private
     */
    #emit(event, data) {
        if (!this.#eventHandlers.has(event)) {
            return;
        }

        const handlers = this.#eventHandlers.get(event);
        for (const handler of handlers) {
            try {
                handler(data);
            } catch (error) {
                console.error(`[ScrcpyClient] Error in ${event} handler:`, error);
            }
        }
    }

    /**
     * 日志输出
     * @private
     */
    #log(message, level = 'info') {
        if (this.#config.onLog) {
            this.#config.onLog(message, level);
        } else {
            console.log(`[ScrcpyClient] [${level.toUpperCase()}] ${message}`);
        }
    }
}

