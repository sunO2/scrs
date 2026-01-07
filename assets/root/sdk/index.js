/**
 * Scrcpy SDK - 索引文件
 * 导出所有公共 API
 */

export { ScrcpySocket } from './ScrcpySocket.js';
export { VideoDecoder } from './VideoDecoder.js';
export { ScrcpyClient } from './ScrcpyClient.js';

// 为了向后兼容，也可以从 ScrcpyClient 访问 Constants
// 使用方式: import { ScrcpyClient } from './sdk/index.js'; const { ACTION_DOWN } = ScrcpyClient.Constants;

