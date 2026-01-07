/**
 * ScrcpySocket - Socket.IO 连接管理模块
 * 负责与 scrcpy 服务器的 Socket.IO 通信
 */

export class ScrcpySocket {
    #socket = null;
    #url = null;
    #options = null;
    #eventHandlers = new Map();
    #isConnected = false;

    /**
     * 创建 Socket.IO 连接实例
     * @param {string} url - Socket.IO 服务器地址 (如: http://127.0.0.1:3000)
     * @param {Object} options - 连接选项
     * @param {Function} options.onConnect - 连接成功回调
     * @param {Function} options.onDisconnect - 断开连接回调
     * @param {Function} options.onError - 连接错误回调
     * @param {Function} options.onVideoData - 接收视频数据回调
     * @param {Function} options.onDeviceMeta - 接收设备元数据回调
     * @param {Function} options.onControlAck - 控制确认回调
     * @param {Function} options.onControlError - 控制错误回调
     */
    constructor(url, options = {}) {
        this.#url = url;
        this.#options = {
            path: '/socket.io/',
            transports: ['websocket', 'polling'],
            ...options
        };

        // 设置事件处理器
        if (options.onConnect) this.on('connect', options.onConnect);
        if (options.onDisconnect) this.on('disconnect', options.onDisconnect);
        if (options.onError) this.on('connect_error', options.onError);
        if (options.onVideoData) this.on('scrcpy', options.onVideoData);
        if (options.onDeviceMeta) this.on('scrcpy_device_meta', options.onDeviceMeta);
        if (options.onControlAck) this.on('scrcpy_ctl_ack', options.onControlAck);
        if (options.onControlError) this.on('scrcpy_ctl_error', options.onControlError);
    }

    /**
     * 连接到 Socket.IO 服务器
     * @returns {Promise<void>}
     */
    async connect() {
        if (this.#isConnected) {
            console.warn('[ScrcpySocket] Already connected');
            return;
        }

        return new Promise((resolve, reject) => {
            try {
                // 创建 Socket.IO 连接
                this.#socket = io(this.#url, this.#options);

                // 连接成功事件
                this.#socket.on('connect', () => {
                    this.#isConnected = true;
                    console.log('[ScrcpySocket] Connected to', this.#url);
                    this.#emit('connect', this.#socket.id);
                    resolve();
                });

                // 连接错误事件
                this.#socket.on('connect_error', (err) => {
                    this.#isConnected = false;
                    console.error('[ScrcpySocket] Connection error:', err.message);
                    this.#emit('connect_error', err);
                    reject(new Error(`Connection failed: ${err.message}`));
                });

                // 断开连接事件
                this.#socket.on('disconnect', (reason) => {
                    this.#isConnected = false;
                    console.log('[ScrcpySocket] Disconnected:', reason);
                    this.#emit('disconnect', reason);
                });

                // 测试响应事件
                this.#socket.on('test_response', (data) => {
                    this.#emit('test_response', data);
                });

                // 设备元数据事件
                this.#socket.on('scrcpy_device_meta', (deviceName) => {
                    console.log('[ScrcpySocket] Device metadata:', deviceName);
                    this.#emit('scrcpy_device_meta', deviceName);
                });

                // 视频数据事件
                this.#socket.on('scrcpy', (base64Data) => {
                    this.#emit('scrcpy', base64Data);
                });

                // 控制确认事件
                this.#socket.on('scrcpy_ctl_ack', (data) => {
                    this.#emit('scrcpy_ctl_ack', data);
                });

                // 控制错误事件
                this.#socket.on('scrcpy_ctl_error', (data) => {
                    console.error('[ScrcpySocket] Control error:', data);
                    this.#emit('scrcpy_ctl_error', data);
                });

            } catch (error) {
                reject(error);
            }
        });
    }

    /**
     * 断开 Socket.IO 连接
     */
    disconnect() {
        if (this.#socket) {
            this.#socket.disconnect();
            this.#socket = null;
            this.#isConnected = false;
            console.log('[ScrcpySocket] Disconnected');
        }
    }

    /**
     * 发送控制消息到服务器
     * @param {Uint8Array} data - 二进制控制数据
     * @param {Function} ack - 确认回调
     */
    sendControl(data, ack) {
        if (!this.#isConnected || !this.#socket) {
            console.warn('[ScrcpySocket] Not connected, cannot send control');
            console.warn('[ScrcpySocket] isConnected=', this.#isConnected, ', socket=', !!this.#socket);
            return false;
        }

        try {
            this.#socket.emit('scrcpy_ctl', data, ack);
            console.log('[ScrcpySocket] Control message sent, length=', data.length);
            return true;
        } catch (error) {
            console.error('[ScrcpySocket] Failed to send control:', error);
            return false;
        }
    }

    /**
     * 发送测试消息
     * @param {Object} message - 测试消息
     */
    sendTest(message) {
        if (!this.#isConnected || !this.#socket) {
            console.warn('[ScrcpySocket] Not connected, cannot send test');
            return;
        }

        this.#socket.emit('test', message);
    }

    /**
     * 注册事件监听器
     * @param {string} event - 事件名称
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

        if (handlers.length === 0) {
            this.#eventHandlers.delete(event);
        }
    }

    /**
     * 触发事件
     * @private
     * @param {string} event - 事件名称
     * @param {*} data - 事件数据
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
                console.error(`[ScrcpySocket] Error in ${event} handler:`, error);
            }
        }
    }

    /**
     * 获取连接状态
     * @returns {boolean}
     */
    isConnected() {
        return this.#isConnected;
    }

    /**
     * 获取 Socket ID
     * @returns {string|null}
     */
    getId() {
        return this.#socket ? this.#socket.id : null;
    }

    /**
     * 销毁实例，清理资源
     */
    destroy() {
        this.disconnect();
        this.#eventHandlers.clear();
        this.#url = null;
        this.#options = null;
    }
}
