use adb_client::server::ADBServer;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use crate::scrcpy::scrcpy::ScrcpyConnect;

/// Scrcpy 服务器，负责管理设备连接和屏幕镜像
pub struct ScrcpyServer {
    devices: HashMap<String, Arc<ScrcpyConnect>>, // 设备序列号 -> ScrcpyConnect Arc
}

impl ScrcpyServer {
    /// 创建新的 Scrcpy 服务器实例
    pub fn new() -> Self {
        ScrcpyServer {
            devices: HashMap::new(),
        }
    }

    /// 检查设备是否已连接
    pub fn is_device_connected(&self, serial: &str) -> bool {
        self.devices.contains_key(serial)
    }

    /// 添加设备到管理列表
    pub fn add_device(&mut self, serial: String, connect: Arc<ScrcpyConnect>) {
        self.devices.insert(serial, connect);
    }

    /// 从管理列表中移除设备
    pub fn remove_device(&mut self, serial: &str) {
        self.devices.remove(serial);
    }

    /// 获取设备连接实例
    pub fn get_device_connect(&self, serial: &str) -> Option<&Arc<ScrcpyConnect>> {
        self.devices.get(serial)
    }

}

impl Default for ScrcpyServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Context trait，定义获取服务器实例的接口
pub trait IContext: Send + Sync {
    fn get_scrcpy(&self) -> &RwLock<ScrcpyServer>;
    fn get_adb_server(&self) -> &RwLock<ADBServer>;
}

/// 线程安全的 Context，管理 ScrcpyServer 和 ADBServer
pub struct Context {
    scrcpy: RwLock<ScrcpyServer>,
    adb_server: RwLock<ADBServer>,
}

impl Context {
    /// 创建新的 Context 实例
    pub fn new() -> Self {
        Context {
            scrcpy: RwLock::new(ScrcpyServer::new()),
            adb_server: RwLock::new(ADBServer::default()),
        }
    }
}

impl IContext for Context {
    fn get_scrcpy(&self) -> &RwLock<ScrcpyServer> {
        &self.scrcpy
    }

    fn get_adb_server(&self) -> &RwLock<ADBServer> {
        &self.adb_server
    }
}
