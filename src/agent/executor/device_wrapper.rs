use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
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
    /// 物理分辨率（实际屏幕像素）
    physical_resolution: Arc<RwLock<Option<(u32, u32)>>>,
    /// 渲染分辨率（应用看到的逻辑分辨率）
    override_resolution: Arc<RwLock<Option<(u32, u32)>>>,
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
            physical_resolution: Arc::new(RwLock::new(None)),
            override_resolution: Arc::new(RwLock::new(None)),
        }
    }

    /// 转换坐标：从逻辑坐标（Override resolution）转换为物理坐标（Physical resolution）
    async fn convert_to_physical_coords(&self, logical_x: u32, logical_y: u32) -> Result<(u32, u32), AppError> {
        let physical = self.physical_resolution.read().await;
        let override_res = self.override_resolution.read().await;

        match (*physical, *override_res) {
            (Some((phys_w, phys_h)), Some((override_w, override_h))) => {
                // 计算缩放比例
                let scale_x = (phys_w as f64) / (override_w as f64);
                let scale_y = (phys_h as f64) / (override_h as f64);

                let physical_x = (logical_x as f64 * scale_x) as u32;
                let physical_y = (logical_y as f64 * scale_y) as u32;

                debug!("坐标转换: ({}, {}) -> ({}, {}) [缩放: x={:.2}, y={:.2}]",
                    logical_x, logical_y, physical_x, physical_y, scale_x, scale_y);

                Ok((physical_x, physical_y))
            }
            _ => {
                // 如果没有分辨率信息，直接返回原始坐标
                debug!("没有分辨率信息，不进行坐标转换: ({}, {})", logical_x, logical_y);
                Ok((logical_x, logical_y))
            }
        }
    }

    /// 刷新分辨率信息
    pub async fn refresh_resolution(&self) -> Result<(), AppError> {
        let output = self.adb_shell("wm size").await?;
        self.parse_and_store_resolution(&output).await
    }

    /// 解析并存储分辨率信息
    async fn parse_and_store_resolution(&self, output: &str) -> Result<(), AppError> {
        let mut physical = self.physical_resolution.write().await;
        let mut override_res = self.override_resolution.write().await;

        *physical = None;
        *override_res = None;

        for line in output.lines() {
            if line.contains("Physical size:") {
                if let Some(size_part) = line.split("Physical size:").nth(1) {
                    let size_str = size_part.trim();
                    if let Some(pos) = size_str.find('x') {
                        let width_str = &size_str[..pos];
                        let height_str = &size_str[pos + 1..];

                        let width = width_str.trim().parse::<u32>().ok();
                        let height = height_str.trim().parse::<u32>().ok();

                        if let (Some(w), Some(h)) = (width, height) {
                            *physical = Some((w, h));
                            info!("物理分辨率: {}x{}", w, h);
                        }
                    }
                }
            }

            if line.contains("Override size:") {
                if let Some(size_part) = line.split("Override size:").nth(1) {
                    let size_str = size_part.trim();
                    if let Some(pos) = size_str.find('x') {
                        let width_str = &size_str[..pos];
                        let height_str = &size_str[pos + 1..];

                        let width = width_str.trim().parse::<u32>().ok();
                        let height = height_str.trim().parse::<u32>().ok();

                        if let (Some(w), Some(h)) = (width, height) {
                            *override_res = Some((w, h));
                            info!("渲染分辨率: {}x{}", w, h);
                        }
                    }
                }
            }
        }

        Ok(())
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
        debug!("解析屏幕尺寸输出: {}", output);

        // 优先查找 "Physical size:" 行
        // 格式示例：
        // "Physical size: 1440x3200"
        // "Override size: 1080x2400"
        for line in output.lines() {
            if line.contains("Physical size:") {
                // 提取 "Physical size: 1440x3200" 中的尺寸部分
                if let Some(size_part) = line.split("Physical size:").nth(1) {
                    let size_str = size_part.trim();
                    debug!("找到 Physical size 行，尺寸部分: '{}'", size_str);

                    if let Some(pos) = size_str.find('x') {
                        let width_str = &size_str[..pos];
                        let height_str = &size_str[pos + 1..];

                        let width = width_str
                            .trim()
                            .trim_end_matches(|c: char| !c.is_ascii_digit())
                            .parse::<u32>()
                            .unwrap_or(0);
                        let height = height_str
                            .trim()
                            .trim_end_matches(|c: char| !c.is_ascii_digit())
                            .parse::<u32>()
                            .unwrap_or(0);

                        debug!("解析结果: width={}, height={}", width, height);

                        if width > 0 && height > 0 {
                            return Ok((width, height));
                        }
                    }
                }
            }
        }

        // 如果没找到 "Physical size:"，尝试查找 "Override size:" 行（作为备用）
        for line in output.lines() {
            if line.contains("Override size:") {
                if let Some(size_part) = line.split("Override size:").nth(1) {
                    let size_str = size_part.trim();
                    debug!("找到 Override size 行，尺寸部分: '{}'", size_str);

                    if let Some(pos) = size_str.find('x') {
                        let width_str = &size_str[..pos];
                        let height_str = &size_str[pos + 1..];

                        let width = width_str
                            .trim()
                            .trim_end_matches(|c: char| !c.is_ascii_digit())
                            .parse::<u32>()
                            .unwrap_or(0);
                        let height = height_str
                            .trim()
                            .trim_end_matches(|c: char| !c.is_ascii_digit())
                            .parse::<u32>()
                            .unwrap_or(0);

                        debug!("解析结果: width={}, height={}", width, height);

                        if width > 0 && height > 0 {
                            return Ok((width, height));
                        }
                    }
                }
            }
        }

        // 最后尝试直接解析 "WIDTHxHEIGHT" 格式
        for line in output.lines() {
            if let Some(pos) = line.find('x') {
                let before = &line[..pos];
                let after = &line[pos + 1..];

                // 只提取数字部分
                let width = before
                    .chars()
                    .rev()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
                    .parse::<u32>()
                    .unwrap_or(0);
                let height = after
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse::<u32>()
                    .unwrap_or(0);

                debug!("备用解析: width={}, height={}", width, height);

                if width > 0 && height > 0 {
                    return Ok((width, height));
                }
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

        // 刷新分辨率信息（确保是最新的）
        let _ = self.refresh_resolution().await;

        // 返回 Override resolution（渲染分辨率），这是 LLM 和应用看到的逻辑分辨率
        let override_res = self.override_resolution.read().await;

        if let Some((w, h)) = *override_res {
            debug!("返回渲染分辨率: {}x{}", w, h);
            Ok((w, h))
        } else {
            // 如果没有 override resolution，回退到 physical resolution
            let physical = self.physical_resolution.read().await;
            if let Some((w, h)) = *physical {
                debug!("没有渲染分辨率，返回物理分辨率: {}x{}", w, h);
                Ok((w, h))
            } else {
                Err(AppError::AdbError("无法获取屏幕分辨率".to_string()))
            }
        }
    }

    async fn tap(&self, x: u32, y: u32) -> Result<(), AppError> {
        debug!("执行点击: ({}, {})", x, y);

        // 转换坐标：从逻辑坐标转换为物理坐标
        let (physical_x, physical_y) = self.convert_to_physical_coords(x, y).await?;

        let output = tokio::process::Command::new("adb")
            .args([
                "-s",
                &self.serial,
                "shell",
                "input",
                "tap",
                &physical_x.to_string(),
                &physical_y.to_string(),
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

        // 转换坐标：从逻辑坐标转换为物理坐标
        let (phys_start_x, phys_start_y) = self.convert_to_physical_coords(start_x, start_y).await?;
        let (phys_end_x, phys_end_y) = self.convert_to_physical_coords(end_x, end_y).await?;

        let output = tokio::process::Command::new("adb")
            .args([
                "-s",
                &self.serial,
                "shell",
                "input",
                "swipe",
                &phys_start_x.to_string(),
                &phys_start_y.to_string(),
                &phys_end_x.to_string(),
                &phys_end_y.to_string(),
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
