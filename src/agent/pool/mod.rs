//! 设备池模块
//!
//! 提供统一的设备管理、连接池化、Agent 按需创建等功能

mod device_pool;
mod device_entry;
mod types;

pub use device_pool::DevicePool;
pub use device_entry::DeviceEntry;
pub use types::{
    DeviceStatus,
    DevicePoolConfig,
    DevicePoolEvent,
    DevicePoolError,
};
