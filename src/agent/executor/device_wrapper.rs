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

    /// 转换坐标：从 1000x1000 逻辑坐标转换为 override_resolution 坐标
    async fn convert_to_physical_coords(&self, logical_x: u32, logical_y: u32) -> Result<(u32, u32), AppError> {
        let override_res = self.override_resolution.read().await;

        match *override_res {
            Some((override_w, override_h)) => {
                // 输入坐标基于 1000x1000，转换为 override_resolution
                let physical_x = (logical_x as f64 * override_w as f64 / 1000.0) as u32;
                let physical_y = (logical_y as f64 * override_h as f64 / 1000.0) as u32;

                debug!("坐标转换: 1000x1000 的 ({}, {}) -> {}x{} 的 ({}, {})",
                    logical_x, logical_y, override_w, override_h, physical_x, physical_y);

                Ok((physical_x, physical_y))
            }
            None => {
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
        use tracing::{debug, warn};

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
            .map_err(|e| AppError::AdbError(format!(
                "点击操作失败：无法执行 ADB 命令\n\n\
                坐标：({}, {})\n\
                错误：{}\n\n\
                建议：\n\
                - 检查设备连接\n\
                - 检查坐标是否在屏幕范围内\n\
                - 尝试重新连接设备",
                x, y, e
            )))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("点击命令执行失败: {}", stderr);

            return Err(AppError::AdbError(format!(
                "点击操作失败：命令执行失败\n\n\
                坐标：({}, {})\n\
                转换后物理坐标：({}, {})\n\
                错误信息：{}\n\n\
                可能的原因：\n\
                1. 设备连接断开\n\
                2. 坐标超出屏幕范围\n\
                3. 屏幕锁定或应用无响应\n\n\
                建议：\n\
                - 检查设备连接状态\n\
                - 确认坐标在屏幕范围内\n\
                - 检查屏幕是否锁定\n\
                - 尝试重新执行操作",
                x, y, physical_x, physical_y, stderr
            )));
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
        use tracing::{debug, warn};

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
            .map_err(|e| AppError::AdbError(format!(
                "滑动操作失败：无法执行 ADB 命令\n\n\
                起点：({}, {})\n\
                终点：({}, {})\n\
                持续时间：{}ms\n\
                错误：{}\n\n\
                建议：\n\
                - 检查设备连接\n\
                - 检查坐标是否在屏幕范围内\n\
                - 尝试重新连接设备",
                start_x, start_y, end_x, end_y, duration_ms, e
            )))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("滑动命令执行失败: {}", stderr);

            return Err(AppError::AdbError(format!(
                "滑动操作失败：命令执行失败\n\n\
                起点：({}, {}) -> 物理坐标：({}, {})\n\
                终点：({}, {}) -> 物理坐标：({}, {})\n\
                持续时间：{}ms\n\
                错误信息：{}\n\n\
                可能的原因：\n\
                1. 设备连接断开\n\
                2. 坐标超出屏幕范围\n\
                3. 屏幕锁定或应用无响应\n\
                4. 滑动距离过短或时间设置不当\n\n\
                建议：\n\
                - 检查设备连接状态\n\
                - 确认坐标在屏幕范围内\n\
                - 检查屏幕是否锁定\n\
                - 尝试增加滑动距离或调整时间\n\
                - 尝试重新执行操作",
                start_x, start_y, phys_start_x, phys_start_y,
                end_x, end_y, phys_end_x, phys_end_y,
                duration_ms, stderr
            )));
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
        use tracing::{debug, warn};

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
            .map_err(|e| AppError::AdbError(format!(
                "输入文本失败：无法执行 ADB 命令\n\n\
                文本内容：{}\n\
                错误：{}\n\n\
                建议：\n\
                - 检查设备连接\n\
                - 确认输入框已激活\n\
                - 尝试重新连接设备",
                text, e
            )))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("输入文本命令执行失败: {}", stderr);

            return Err(AppError::AdbError(format!(
                "输入文本失败：命令执行失败\n\n\
                文本内容：{}\n\
                错误信息：{}\n\n\
                可能的原因：\n\
                1. 设备连接断开\n\
                2. 没有激活的输入框\n\
                3. 输入框不支持文本输入\n\
                4. 特殊字符转义问题\n\n\
                建议：\n\
                - 确保输入框已激活（先点击输入框）\n\
                - 检查设备连接状态\n\
                - 尝试分段输入较长文本\n\
                - 如果是特殊字符，尝试使用其他输入方式",
                text, stderr
            )));
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
        use tracing::{info, debug, warn, error};

        info!("🚀 launch_app: 准备启动应用");
        info!("   设备: {}", self.serial);
        info!("   包名: {}", package);

        // 使用 monkey 命令启动应用
        let cmd = format!(
            "adb -s {} shell monkey -p {} -c android.intent.category.LAUNCHER 1",
            self.serial, package
        );
        debug!("   执行命令: {}", cmd);

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
            .await;

        match output {
            Ok(result) => {
                debug!("   命令执行完成");
                debug!("   退出码: {}", result.status);

                let stdout = String::from_utf8_lossy(&result.stdout);
                let stderr = String::from_utf8_lossy(&result.stderr);

                if !stdout.is_empty() {
                    debug!("   stdout: {}", stdout);
                }
                if !stderr.is_empty() {
                    debug!("   stderr: {}", stderr);
                }

                if !result.status.success() {
                    error!("   ❌ 命令执行失败");
                    error!("   退出码: {:?}", result.status.code());

                    // 检查是否是应用不存在的问题
                    if stderr.contains("No package found") || stdout.contains("No package found") {
                        return Err(AppError::AdbError(format!(
                            "启动应用失败：找不到应用 '{}'\n\n\
                            可能的原因：\n\
                            1. 应用未安装\n\
                            2. 包名错误\n\
                            3. 应用名称不在支持列表中\n\n\
                            建议：\n\
                            - 检查应用是否已安装\n\
                            - 使用完整包名（如 com.tencent.mm）\n\
                            - 或使用支持的应用名称（如：微信、淘宝、抖音等）",
                            package
                        )));
                    }

                    // 检查设备连接问题
                    if stderr.contains("device not found") || stderr.contains("device offline") {
                        return Err(AppError::AdbError(format!(
                            "设备连接失败：设备 '{}' 不可用\n\n\
                            可能的原因：\n\
                            1. 设备未连接\n\
                            2. USB 调试未开启\n\
                            3. ADB 连接断开\n\n\
                            建议：\n\
                            - 检查设备是否连接\n\
                            - 重新连接设备\n\
                            - 重启 ADB 服务",
                            self.serial
                        )));
                    }

                    // 检查权限问题
                    if stderr.contains("permission denied") {
                        return Err(AppError::AdbError(
                            "权限不足：无法启动应用\n\n\
                            可能的原因：\n\
                            1. ADB 权限不足\n\
                            2. 应用需要特殊权限\n\n\
                            建议：\n\
                            - 检查 ADB 调试权限\n\
                            - 尝试手动授权应用".to_string()
                        ));
                    }

                    // 检查其他常见错误
                    let error_msg = if !stderr.is_empty() {
                        stderr.to_string()
                    } else if !stdout.is_empty() {
                        stdout.to_string()
                    } else {
                        format!("未知错误 (退出码: {:?})", result.status.code())
                    };

                    return Err(AppError::AdbError(format!(
                        "启动应用失败：{}\n\n\
                        应用包名：{}\n\
                        错误详情：{}\n\n\
                        建议：\n\
                        - 检查应用是否已安装\n\
                        - 尝试使用其他启动方式\n\
                        - 检查设备状态",
                        package, package, error_msg
                    )));
                }

                info!("   ✅ 命令执行成功");
            }
            Err(e) => {
                error!("   ❌ 命令执行异常: {}", e);
                return Err(AppError::AdbError(format!("ADB 命令执行失败: {}", e)));
            }
        }

        // 等待应用启动
        debug!("   等待应用启动...");
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        info!("   ✅ 应用启动流程完成");

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
