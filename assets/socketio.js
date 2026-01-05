// Node.js 测试脚本
const { io } = require('socket.io-client');

// 配置
const SOCKETIO_URL = 'http://127.0.0.1:60482';
const DEFAULT_X = 160;
const DEFAULT_Y = 260;

// 创建 socket 连接
const socket = io(SOCKETIO_URL, {
    path: '/socket.io/',
    transports: ['websocket', 'polling']
});

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
const POINTER_ID = 0n; // 使用 BigInt 表示 64 位整数

// 默认屏幕尺寸 (可以根据实际设备调整)
const SCREEN_WIDTH = 1080;
const SCREEN_HEIGHT = 1920;

/**
 * 将浮点数转换为 u16 固定点数 (16.16)
 * @param {number} value - 浮点数值 (0.0 - 1.0)
 * @returns {number} uint16 固定点数
 */
function floatToU16FixedPoint(value) {
    // scrcpy 使用 u16FixedPoint: value * 0x10000 (但返回的是 uint16)
    // 实际上是将 [0, 1] 映射到 [0, 65535]
    return Math.floor(value * 65535);
}

/**
 * 构建触摸事件消息
 * 参考: https://github.com/Genymobile/scrcpy/blob/master/server/src/main/java/com/genymobile/scrcpy/control/ControlMessageReader.java
 *
 * @param {number} action - 动作类型 (0=DOWN, 1=UP, 2=MOVE, etc.)
 * @param {bigint} pointerId - 指针 ID (64 位整数)
 * @param {number} x - X 坐标
 * @param {number} y - Y 坐标
 * @param {number} pressure - 按压强度 (0.0 - 1.0)
 * @param {number} actionButton - 动作按钮 (MotionEvent.BUTTON_*)
 * @param {number} buttons - 当前按钮状态
 * @returns {Buffer} 编码后的二进制消息 (31 bytes)
 */
function buildTouchEvent(action, pointerId, x, y, pressure = 1.0, actionButton = 0, buttons = 0) {
    // 消息格式 (总共 32 bytes):
    // [类型(1B)] [动作(1B)] [指针ID(8B)] [X(4B)] [Y(4B)] [屏幕宽(2B)] [屏幕高(2B)] [压力(2B)] [动作按钮(4B)] [按钮(4B)]

    const buffer = Buffer.alloc(32);

    let offset = 0;

    // 1. 类型: 1 byte (TYPE_INJECT_TOUCH_EVENT = 2)
    buffer.writeUInt8(SCRCPY_MSG_TYPE_INJECT_TOUCH_EVENT, offset);
    offset += 1;

    // 2. 动作: 1 byte (ACTION_DOWN, ACTION_UP, ACTION_MOVE, etc.)
    buffer.writeUInt8(action, offset);
    offset += 1;

    // 3. 指针 ID: 8 bytes (signed long)
    // 使用 BigInt 处理 64 位整数
    buffer.writeBigUInt64LE(pointerId, offset);
    offset += 8;

    // 4. X 坐标: 4 bytes (signed int, little-endian)
    buffer.writeInt32LE(x, offset);
    offset += 4;

    // 5. Y 坐标: 4 bytes (signed int, little-endian)
    buffer.writeInt32LE(y, offset);
    offset += 4;

    // 6. 屏幕宽度: 2 bytes (unsigned short, big-endian)
    buffer.writeUInt16BE(SCREEN_WIDTH, offset);
    offset += 2;

    // 7. 屏幕高度: 2 bytes (unsigned short, big-endian)
    buffer.writeUInt16BE(SCREEN_HEIGHT, offset);
    offset += 2;

    // 8. 压力: 2 bytes (u16 fixed point, big-endian)
    // 将 [0, 1] 的浮点数转换为 16 位无符号整数
    buffer.writeUInt16BE(floatToU16FixedPoint(pressure), offset);
    offset += 2;

    // 9. 动作按钮: 4 bytes (signed int, little-endian)
    buffer.writeInt32LE(actionButton, offset);
    offset += 4;

    // 10. 按钮状态: 4 bytes (signed int, little-endian)
    buffer.writeInt32LE(buttons, offset);

    return buffer;
}

/**
 * 发送点击事件 (DOWN + UP)
 * @param {number} x - X 坐标
 * @param {number} y - Y 坐标
 */
function sendClick(x, y) {
    console.log(`\n📱 发送点击事件: (${x}, ${y})`);

    // 发送 DOWN 事件
    const downMsg = buildTouchEvent(ACTION_DOWN, POINTER_ID, x, y, 1.0, BUTTON_PRIMARY, BUTTON_PRIMARY);
    socket.emit('scrcpy_ctl', downMsg);
    console.log(`  ✓ DOWN 事件已发送 (${downMsg.length} bytes)`);

    // 延迟 50ms 后发送 UP 事件
    setTimeout(() => {
        const upMsg = buildTouchEvent(ACTION_UP, POINTER_ID, x, y, 0.0, BUTTON_PRIMARY, 0);
        socket.emit('scrcpy_ctl', upMsg);
        console.log(`  ✓ UP 事件已发送 (${upMsg.length} bytes)`);
    }, 50);
}

// Socket 事件处理
socket.on('connect', () => {
    console.log('✅ Socket.IO 连接成功！');
    console.log(`   Socket ID: ${socket.id}`);

    // 发送测试消息
    socket.emit('test', { message: 'Hello from client' });
});

socket.on('test_response', (data) => {
    console.log('✅ 收到 test 响应:', data);
});

socket.on('scrcpy', (base64Data) => {
    console.log(`📺 收到 scrcpy 视频数据 (${base64Data.length} chars)`);
    // 可以在这里解码 base64 数据
    // const binaryData = Buffer.from(base64Data, 'base64');
});

socket.on('connect_error', (err) => {
    console.log('❌ 连接错误:', err.message);
});

socket.on('disconnect', (reason) => {
    console.log('❌ 断开连接:', reason);
});

// 命令行交互
const readline = require('readline');

const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout
});

console.log('\n📝 命令:');
console.log('  click [x] [y]  - 发送点击事件 (默认: 160 260)');
console.log('  test           - 发送测试消息');
console.log('  quit           - 退出\n');

rl.on('line', (input) => {
    const parts = input.trim().split(' ');
    const cmd = parts[0].toLowerCase();

    if (cmd === 'click' || cmd === 'c') {
        const x = parseInt(parts[1]) || DEFAULT_X;
        const y = parseInt(parts[2]) || DEFAULT_Y;
        sendClick(x, y);
    } else if (cmd === 'test' || cmd === 't') {
        socket.emit('test', { message: 'Test message', timestamp: Date.now() });
        console.log('✓ 测试消息已发送');
    } else if (cmd === 'quit' || cmd === 'exit' || cmd === 'q') {
        socket.disconnect();
        rl.close();
        process.exit(0);
    } else if (cmd === 'help' || cmd === 'h') {
        console.log('\n📝 命令:');
        console.log('  click [x] [y]  - 发送点击事件 (默认: 160 260)');
        console.log('  test           - 发送测试消息');
        console.log('  quit           - 退出\n');
    } else {
        console.log(`❌ 未知命令: ${cmd}`);
        console.log('输入 "help" 查看可用命令');
    }
});

// 自动测试：连接后 2 秒自动发送一次点击
setTimeout(() => {
    if (socket.connected) {
        console.log('\n🔄 自动测试: 发送点击事件...');
        sendClick(DEFAULT_X, DEFAULT_Y);
    }
}, 2000);

