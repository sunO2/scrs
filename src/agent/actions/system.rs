use serde::{Deserialize, Serialize};
use crate::agent::core::traits::{Action, Device, ActionResult, ActionError};
use crate::error::AppError;
use std::time::Instant;
use tokio::time::sleep;
use std::collections::HashMap;

/// 常见应用名称到包名的映射
fn get_app_packages() -> HashMap<String, &'static str> {
    let mut map = HashMap::new();

    // 社交应用
    map.insert("微信".to_string(), "com.tencent.mm");
    map.insert("wechat".to_string(), "com.tencent.mm");
    map.insert("qq".to_string(), "com.tencent.mobileqq");
    map.insert("微博".to_string(), "com.sina.weibo");
    map.insert("钉钉".to_string(), "com.alibaba.android.rimet");
    map.insert("抖音".to_string(), "com.ss.android.ugc.aweme");
    map.insert("快手".to_string(), "com.smile.gifmaker");

    // 购物应用
    map.insert("淘宝".to_string(), "com.taobao.taobao");
    map.insert("天猫".to_string(), "com.tmall.wireless");
    map.insert("京东".to_string(), "com.jingdong.app.mall");
    map.insert("拼多多".to_string(), "com.xunmeng.pinduoduo");

    // 视频应用
    map.insert("腾讯视频".to_string(), "com.tencent.qqlive");
    map.insert("爱奇艺".to_string(), "com.qiyi.video");
    map.insert("优酷".to_string(), "com.youku.phone");
    map.insert("哔哩哔哩".to_string(), "tv.danmaku.bili");
    map.insert("bilibili".to_string(), "tv.danmaku.bili");

    // 音乐应用
    map.insert("网易云音乐".to_string(), "com.netease.cloudmusic");
    map.insert("qq音乐".to_string(), "com.tencent.qqmusic");
    map.insert("酷狗音乐".to_string(), "com.kugou.android");

    // 工具应用
    map.insert("支付宝".to_string(), "com.eg.android.AlipayGphone");
    map.insert("alipay".to_string(), "com.eg.android.AlipayGphone");
    map.insert("邮箱".to_string(), "com.android.email");
    map.insert("文件管理".to_string(), "com.android.filemanager");
    map.insert("设置".to_string(), "com.android.settings");
    map.insert("相机".to_string(), "com.android.camera");
    map.insert("gallery".to_string(), "com.android.gallery3d");
    map.insert("图库".to_string(), "com.android.gallery3d");

    // 浏览器
    map.insert("chrome".to_string(), "com.android.chrome");
    map.insert("浏览器".to_string(), "com.android.browser");
    map.insert("uc浏览器".to_string(), "com.uc.browser");

    // 其他
    map.insert("电话".to_string(), "com.android.contacts");
    map.insert("短信".to_string(), "com.android.mms");
    map.insert("日历".to_string(), "com.android.calendar");
    map.insert("时钟".to_string(), "com.android.deskclock");
    map.insert("计算器".to_string(), "com.android.calculator2");

    map
}

/// 将应用名称转换为包名
pub fn app_name_to_package(app_name: &str) -> Option<String> {
    use tracing::debug;

    debug!("🔍 app_name_to_package: {}", app_name);

    let packages = get_app_packages();

    // 首先尝试直接匹配
    if let Some(package) = packages.get(app_name) {
        debug!("   ✅ 直接匹配: {} -> {}", app_name, package);
        return Some(package.to_string());
    }

    // 尝试小写匹配
    let lower_name = app_name.to_lowercase();
    if let Some(package) = packages.get(&lower_name) {
        debug!("   ✅ 小写匹配: {} -> {} -> {}", app_name, lower_name, package);
        return Some(package.to_string());
    }

    // 如果已经是包名格式（包含点），直接返回
    if app_name.contains('.') {
        debug!("   ✅ 包名格式: {}", app_name);
        return Some(app_name.to_string());
    }

    debug!("   ❌ 未找到匹配: {}", app_name);
    None
}

/// 启动应用操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchAction {
    /// 应用名称（如"微信"）或包名（如"com.tencent.mm"）
    #[serde(alias = "app_name")]
    pub package: String,
    pub activity: Option<String>,
    pub description: Option<String>,
}

impl LaunchAction {
    /// 从应用名称创建 LaunchAction
    pub fn from_app_name(app_name: &str) -> Option<Self> {
        let package = app_name_to_package(app_name)?;
        Some(Self {
            package,
            activity: None,
            description: Some(format!("启动应用: {}", app_name)),
        })
    }
}

impl Action for LaunchAction {
    fn action_type(&self) -> String {
        "launch".to_string()
    }

    async fn execute(&self, device: &dyn Device) -> Result<ActionResult, AppError> {
        use tracing::{info, debug, error};

        info!("🚀 LaunchAction: 开始执行");
        info!("   原始输入: package={}", self.package);
        info!("   activity: {:?}", self.activity);
        info!("   description: {:?}", self.description);

        // 尝试将应用名称转换为包名
        let actual_package = if !self.package.contains('.') {
            debug!("   检测到应用名称，尝试转换为包名...");
            match app_name_to_package(&self.package) {
                Some(pkg) => {
                    info!("   ✅ 应用名称映射: {} -> {}", self.package, pkg);
                    pkg
                }
                None => {
                    error!("   ❌ 无法识别的应用名称: {}", self.package);
                    return Err(AppError::AdbError(format!(
                        "无法识别的应用名称: {}，请使用完整的包名或已知的应用名称",
                        self.package
                    )));
                }
            }
        } else {
            info!("   检测到包名格式: {}", self.package);
            self.package.clone()
        };

        info!("   实际包名: {}", actual_package);

        let start = Instant::now();

        debug!("   调用 device.launch_app...");
        match device.launch_app(&actual_package).await {
            Ok(_) => {
                let elapsed = start.elapsed();
                info!("   ✅ 应用启动成功: {} (耗时: {}ms)", actual_package, elapsed.as_millis());
                Ok(ActionResult::success(
                    self.description
                        .clone()
                        .unwrap_or_else(|| format!("启动应用: {}", actual_package)),
                    elapsed.as_millis() as u32,
                ))
            }
            Err(e) => {
                error!("   ❌ 应用启动失败: {}", e);
                error!("   包名: {}", actual_package);
                error!("   错误详情: {:?}", e);
                Err(e)
            }
        }
    }

    fn validate(&self) -> Result<(), ActionError> {
        use tracing::debug;

        debug!("🔍 LaunchAction: 验证参数");
        debug!("   package={}", self.package);

        if self.package.is_empty() {
            return Err(ActionError::InvalidParameters("应用名称不能为空".to_string()));
        }

        // 检查是否为有效的包名或可识别的应用名称
        if !self.package.contains('.') {
            debug!("   尝试映射应用名称...");
            // 尝试将应用名称转换为包名
            match app_name_to_package(&self.package) {
                Some(package) => {
                    debug!("   ✅ 应用名称映射: {} -> {}", self.package, package);
                }
                None => {
                    debug!("   ❌ 无法识别的应用名称: {}", self.package);
                    return Err(ActionError::InvalidParameters(
                        format!("暂时没有 {} 对应的包名 可以在Home页其他页面查找一下", self.package),
                    ));
                }
            }
        } else {
            debug!("   检测到包名格式，跳过映射");
        }

        debug!("   ✅ 验证通过");
        Ok(())
    }

    fn description(&self) -> String {
        self.description
            .clone()
            .unwrap_or_else(|| format!("启动应用: {}", self.package))
    }
}

/// 等待操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitAction {
    pub duration_ms: u32,
    pub reason: Option<String>,
}

impl Action for WaitAction {
    fn action_type(&self) -> String {
        "wait".to_string()
    }

    async fn execute(&self, device: &dyn Device) -> Result<ActionResult, AppError> {
        let start = Instant::now();
        sleep(std::time::Duration::from_millis(self.duration_ms as u64)).await;
        Ok(ActionResult::success(
            self.reason
                .clone()
                .unwrap_or_else(|| format!("等待 {}ms", self.duration_ms)),
            start.elapsed().as_millis() as u32,
        ))
    }

    fn validate(&self) -> Result<(), ActionError> {
        if self.duration_ms > 60000 {
            return Err(ActionError::DurationTooLong(self.duration_ms));
        }
        Ok(())
    }

    fn description(&self) -> String {
        self.reason
            .clone()
            .unwrap_or_else(|| format!("等待 {}ms", self.duration_ms))
    }
}

/// 截图操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotAction {
    pub description: Option<String>,
}

impl Action for ScreenshotAction {
    fn action_type(&self) -> String {
        "screenshot".to_string()
    }

    async fn execute(&self, device: &dyn Device) -> Result<ActionResult, AppError> {
        let start = Instant::now();
        let screenshot = device.screenshot().await?;
        Ok(ActionResult {
            success: true,
            message: self
                .description
                .clone()
                .unwrap_or_else(|| "截图成功".to_string()),
            duration_ms: start.elapsed().as_millis() as u32,
            screenshot_before: Some(screenshot),
            screenshot_after: None,
        })
    }

    fn validate(&self) -> Result<(), ActionError> {
        Ok(())
    }

    fn description(&self) -> String {
        self.description
            .clone()
            .unwrap_or_else(|| "截图".to_string())
    }
}

/// 完成任务操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinishAction {
    pub result: String,
    pub success: bool,
}

impl Action for FinishAction {
    fn action_type(&self) -> String {
        "finish".to_string()
    }

    async fn execute(&self, _device: &dyn Device) -> Result<ActionResult, AppError> {
        Ok(ActionResult {
            success: self.success,
            message: self.result.clone(),
            duration_ms: 0,
            screenshot_before: None,
            screenshot_after: None,
        })
    }

    fn validate(&self) -> Result<(), ActionError> {
        Ok(())
    }

    fn description(&self) -> String {
        format!("完成任务: {}", self.result)
    }
}
