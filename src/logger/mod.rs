use std::{
    fs::OpenOptions,
    io::Write,
    sync::Arc,
};

/// 设备日志记录器
#[derive(Clone)]
pub struct DeviceLogger {
    device_serial: String,
    log_path: String,
    file_handle: Arc<std::sync::Mutex<Option<std::fs::File>>>,
}

impl DeviceLogger {
    /// 为指定设备创建一个新的日志记录器
    pub fn new(device_serial: &str) -> Self {
        // 创建 logs 目录（如果不存在）
        std::fs::create_dir_all("logs").expect("无法创建 logs 目录");

        let log_path = format!("logs/ws_{}.log", device_serial);

        DeviceLogger {
            device_serial: device_serial.to_string(),
            log_path,
            file_handle: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// 写入日志到文件
    fn write_to_file(&self, message: &str) {
        let mut file_guard = self.file_handle.lock().unwrap();

        // 如果文件句柄不存在或需要重新打开
        if file_guard.is_none() {
            *file_guard = Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.log_path)
                    .expect("无法打开日志文件"),
            );
        }

        if let Some(ref mut file) = *file_guard {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let log_line = format!("{} [{}] {}\n", timestamp, self.device_serial, message);

            if let Err(e) = file.write_all(log_line.as_bytes()) {
                eprintln!("写入日志文件失败: {:?}", e);
            }

            // 立即刷新，确保日志写入
            if let Err(e) = file.flush() {
                eprintln!("刷新日志文件失败: {:?}", e);
            }
        }
    }

    /// 记录 INFO 级别日志
    pub fn info(&self, message: &str) {
        self.write_to_file(&format!("INFO  {}", message));
        tracing::info!(device = %self.device_serial, "{}", message);
    }

    /// 记录 WARN 级别日志
    pub fn warn(&self, message: &str) {
        self.write_to_file(&format!("WARN  {}", message));
        tracing::warn!(device = %self.device_serial, "{}", message);
    }

    /// 记录 ERROR 级别日志
    pub fn error(&self, message: &str) {
        self.write_to_file(&format!("ERROR {}", message));
        tracing::error!(device = %self.device_serial, "{}", message);
    }

    /// 记录 DEBUG 级别日志
    pub fn debug(&self, message: &str) {
        self.write_to_file(&format!("DEBUG {}", message));
        tracing::debug!(device = %self.device_serial, "{}", message);
    }
}
