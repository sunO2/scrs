use async_trait::async_trait;
use std::sync::Arc;
use crate::agent::core::traits::Device;
use crate::error::AppError;
use crate::scrcpy::scrcpy::ScrcpyConnect;
use adb_client::server_device::ADBServerDevice;
use tracing::{debug, info, error, warn};

/// Scrcpy 设备包装器，实现 Device trait
/// 将现有的 ScrcpyConnect 和 ADB 功能封装成统一的接口
pub struct ScrcpyDeviceWrapper {
    serial: String,
    name: String,
    scrcpy_connect: Arc<ScrcpyConnect>,
    adb_device: Arc<ADBServerDevice>,
}

impl ScrcpyDeviceWrapper {
    /// 创建新的设备包装器
    pub fn new(
        serial: String,
        name: String,
        scrcpy_connect: Arc<ScrcpyConnect>,
        adb_device: Arc<ADBServerDevice>,
    ) -> Self {
        Self {
            serial,
            name,
            scrcpy_connect,
            adb_device,
        }
    }

    /// 执行 ADB shell 命令
    async fn adb_shell(&self, command: &str) -> Result<String, AppError> {
        debug!("执行 ADB 命令: adb -s {} shell {}", self.serial, command);

        let output = tokio::process::Command::new("adb")
            .args(["-s", &self.serial, "shell", command])
            .output()
            .await
            .map_err(|e| AppError::AdbError(format!("执行命令失败: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::AdbError(format!(
                "命令执行失败: {}",
                stderr
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// 解析屏幕尺寸
    fn parse_screen_size(&self, output: &str) -> Result<(u32, u32), AppError> {
        // 格式: "Physical size: 1080x2400" 或 "1080x2400"
        let parts: Vec<&str> = output
            .split('x')
            .map(|s| s.trim())
            .collect();

        if parts.len() >= 2 {
            let width_str = parts
                .last()
                .unwrap_or(&"0")
                .trim()
                .trim_end_matches(|c: char| !c.is_ascii_digit());
            let height_str = parts
                .first()
                .unwrap_or(&"0")
                .trim()
                .trim_end_matches(|c: char| !c.is_ascii_digit());

            let width = width_str
                .parse::<u32>()
                .unwrap_or(0);
            let height = height_str
                .parse::<u32>()
                .unwrap_or(0);

            if width > 0 && height > 0 {
                return Ok((height, width)); // 注意：ADB 返回的是 WIDTHxHEIGHT
            }
        }

        // 尝试另一种解析方式
        if let Some(pos) = output.find('x') {
            let before = &output[..pos];
            let after = &output[pos + 1..];

            let width = before
                .trim()
                .trim_end_matches(|c: char| !c.is_ascii_digit())
                .parse::<u32>()
                .unwrap_or(0);
            let height = after
                .trim()
                .trim_end_matches(|c: char| !c.is_ascii_digit())
                .parse::<u32>()
                .unwrap_or(0);

            if width > 0 && height > 0 {
                return Ok((width, height));
            }
        }

        Err(AppError::AdbError(format!(
            "无法解析屏幕尺寸: {}",
            output
        )))
    }
}

#[async_trait]
impl Device for ScrcpyDeviceWrapper {
    fn serial(&self) -> &str {
        &self.serial
    }

    fn name(&self) -> &str {
        &self.name
    }

    async fn is_connected(&self) -> bool {
        // 检查设备是否仍在线
        match tokio::process::Command::new("adb")
            .args(["-s", &self.serial, "shell", "echo", "ping"])
            .output()
            .await
        {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    async fn screenshot(&self) -> Result<String, AppError> {
        debug!("截取设备屏幕: {}", self.serial);

        // 使用 ADB 截图并转换为 base64
        let output = tokio::process::Command::new("adb")
            .args([
                "-s",
                &self.serial,
                "shell",
                "screencap",
                "-p",
            ])
            .output()
            .await
            .map_err(|e| AppError::AdbError(format!("截图失败: {}", e)))?;

        if !output.status.success() {
            return Err(AppError::AdbError("截图命令执行失败".to_string()));
        }

        // 转换为 base64
        use base64::Engine;
        let base64_string = base64::engine::general_purpose::STANDARD.encode(&output.stdout);
        Ok(base64_string)
    }

    async fn screen_size(&self) -> Result<(u32, u32), AppError> {
        debug!("获取屏幕尺寸: {}", self.serial);

        let output = self.adb_shell("wm size").await?;
        self.parse_screen_size(&output)
    }

    async fn tap(&self, x: u32, y: u32) -> Result<(), AppError> {
        debug!("执行点击: ({}, {})", x, y);

        let output = tokio::process::Command::new("adb")
            .args([
                "-s",
                &self.serial,
                "shell",
                "input",
                "tap",
                &x.to_string(),
                &y.to_string(),
            ])
            .output()
            .await
            .map_err(|e| AppError::AdbError(format!("点击失败: {}", e)))?;

        if !output.status.success() {
            return Err(AppError::AdbError("点击命令执行失败".to_string()));
        }

        Ok(())
    }

    async fn swipe(
        &self,
        start_x: u32,
        start_y: u32,
        end_x: u32,
        end_y: u32,
        duration_ms: u32,
    ) -> Result<(), AppError> {
        debug!(
            "执行滑动: ({}, {}) -> ({}, {}) {}ms",
            start_x, start_y, end_x, end_y, duration_ms
        );

        let output = tokio::process::Command::new("adb")
            .args([
                "-s",
                &self.serial,
                "shell",
                "input",
                "swipe",
                &start_x.to_string(),
                &start_y.to_string(),
                &end_x.to_string(),
                &end_y.to_string(),
                &duration_ms.to_string(),
            ])
            .output()
            .await
            .map_err(|e| AppError::AdbError(format!("滑动失败: {}", e)))?;

        if !output.status.success() {
            return Err(AppError::AdbError("滑动命令执行失败".to_string()));
        }

        Ok(())
    }

    async fn long_press(&self, x: u32, y: u32, duration_ms: u32) -> Result<(), AppError> {
        debug!("执行长按: ({}, {}) {}ms", x, y, duration_ms);

        // 长按可以通过滑动实现（起点和终点相同）
        self.swipe(x, y, x, y, duration_ms).await
    }

    async fn double_tap(&self, x: u32, y: u32) -> Result<(), AppError> {
        debug!("执行双击: ({}, {})", x, y);

        // 双击通过两次快速点击实现
        self.tap(x, y).await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        self.tap(x, y).await
    }

    async fn input_text(&self, text: &str) -> Result<(), AppError> {
        debug!("输入文本: {}", text);

        // 转义特殊字符
        let escaped_text = text
            .replace(' ', "%s")
            .replace('&', "\\&")
            .replace('(', "\\(")
            .replace(')', "\\)")
            .replace(';', "\\;")
            .replace('|', "\\|")
            .replace('<', "\\<")
            .replace('>', "\\>");

        let output = tokio::process::Command::new("adb")
            .args([
                "-s",
                &self.serial,
                "shell",
                "input",
                "text",
                &escaped_text,
            ])
            .output()
            .await
            .map_err(|e| AppError::AdbError(format!("输入文本失败: {}", e)))?;

        if !output.status.success() {
            return Err(AppError::AdbError("输入文本命令执行失败".to_string()));
        }

        Ok(())
    }

    async fn press_key(&self, keycode: u32) -> Result<(), AppError> {
        debug!("按下按键: {}", keycode);

        let output = tokio::process::Command::new("adb")
            .args([
                "-s",
                &self.serial,
                "shell",
                "input",
                "keyevent",
                &keycode.to_string(),
            ])
            .output()
            .await
            .map_err(|e| AppError::AdbError(format!("按键失败: {}", e)))?;

        if !output.status.success() {
            return Err(AppError::AdbError("按键命令执行失败".to_string()));
        }

        Ok(())
    }

    async fn back(&self) -> Result<(), AppError> {
        debug!("按下返回键");
        self.press_key(4).await // KEYCODE_BACK = 4
    }

    async fn home(&self) -> Result<(), AppError> {
        debug!("按下 Home 键");
        self.press_key(3).await // KEYCODE_HOME = 3
    }

    async fn recent(&self) -> Result<(), AppError> {
        debug!("打开最近任务");
        self.press_key(187).await // KEYCODE_APP_SWITCH = 187
    }

    async fn notification(&self) -> Result<(), AppError> {
        debug!("打开通知栏");
        self.swipe(540, 0, 540, 500, 300).await // 从顶部向下滑动
    }

    async fn launch_app(&self, package: &str) -> Result<(), AppError> {
        info!("启动应用: {}", package);

        // 使用 monkey 命令启动应用
        let output = tokio::process::Command::new("adb")
            .args([
                "-s",
                &self.serial,
                "shell",
                "monkey",
                "-p",
                package,
                "-c",
                "android.intent.category.LAUNCHER",
                "1",
            ])
            .output()
            .await
            .map_err(|e| AppError::AdbError(format!("启动应用失败: {}", e)))?;

        if !output.status.success() {
            return Err(AppError::AdbError(format!(
                "启动应用失败: {}",
                package
            )));
        }

        // 等待应用启动
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        Ok(())
    }

    async fn current_app(&self) -> Result<String, AppError> {
        debug!("获取当前应用");

        let output = self
            .adb_shell("dumpsys window windows | grep -E 'mCurrentFocus'")
            .await?;

        // 解析输出获取当前应用包名
        // 格式: "mCurrentFocus=Window{... u0 com.package.name/com.activity.Name}"
        if let Some(start) = output.find(' ') {
            let app_info = &output[start + 1..];
            if let Some(end) = app_info.find('/') {
                let package = &app_info[..end];
                return Ok(package.to_string());
            }
        }

        Err(AppError::AdbError(
            "无法解析当前应用包名".to_string(),
        ))
    }
}
