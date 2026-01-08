use std::time::Duration;
use tracing::{debug, warn};

/// 重试策略
#[derive(Debug, Clone)]
pub enum RetryStrategy {
    /// 立即重试
    Immediate,

    /// 固定延迟重试
    FixedDelay { delay_ms: u64 },

    /// 指数退避重试
    ExponentialBackoff {
        initial_delay_ms: u64,
        max_delay_ms: u64,
        multiplier: f64,
    },

    /// 无重试
    None,
}

impl Default for RetryStrategy {
    fn default() -> Self {
        Self::ExponentialBackoff {
            initial_delay_ms: 1000,
            max_delay_ms: 10000,
            multiplier: 2.0,
        }
    }
}

impl RetryStrategy {
    /// 获取下一次重试的延迟时间
    pub fn next_delay(&self, attempt: u32) -> Option<Duration> {
        match self {
            RetryStrategy::Immediate => Some(Duration::from_millis(0)),
            RetryStrategy::FixedDelay { delay_ms } => Some(Duration::from_millis(*delay_ms)),
            RetryStrategy::ExponentialBackoff {
                initial_delay_ms,
                max_delay_ms,
                multiplier,
            } => {
                let delay = (*initial_delay_ms as f64 * multiplier.powi(attempt as i32)) as u64;
                let delay = delay.min(*max_delay_ms);
                Some(Duration::from_millis(delay))
            }
            RetryStrategy::None => None,
        }
    }

    /// 创建指数退避策略
    pub fn exponential(initial_delay_ms: u64, max_delay_ms: u64, multiplier: f64) -> Self {
        Self::ExponentialBackoff {
            initial_delay_ms,
            max_delay_ms,
            multiplier,
        }
    }

    /// 创建固定延迟策略
    pub fn fixed(delay_ms: u64) -> Self {
        Self::FixedDelay { delay_ms }
    }
}

/// 重试配置
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// 最大重试次数
    pub max_attempts: u32,

    /// 重试策略
    pub strategy: RetryStrategy,

    /// 可重试的错误类型（可选）
    pub retryable_errors: Vec<String>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            strategy: RetryStrategy::default(),
            retryable_errors: vec![],
        }
    }
}

impl RetryConfig {
    /// 创建新的重试配置
    pub fn new(max_attempts: u32, strategy: RetryStrategy) -> Self {
        Self {
            max_attempts,
            strategy,
            retryable_errors: vec![],
        }
    }

    /// 设置可重试的错误类型
    pub fn with_retryable_errors(mut self, errors: Vec<String>) -> Self {
        self.retryable_errors = errors;
        self
    }

    /// 检查错误是否可重试
    pub fn is_retryable(&self, error_message: &str) -> bool {
        if self.retryable_errors.is_empty() {
            return true; // 默认所有错误都可重试
        }

        self.retryable_errors.iter().any(|pattern| {
            error_message.contains(pattern)
                || error_message.to_lowercase().contains(&pattern.to_lowercase())
        })
    }

    /// 执行带重试的操作
    pub async fn execute<F, Fut, T, E>(&self, mut operation: F) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        let mut last_error = None;

        for attempt in 0..self.max_attempts {
            match operation().await {
                Ok(result) => {
                    if attempt > 0 {
                        debug!("操作在第 {} 次重试后成功", attempt);
                    }
                    return Ok(result);
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    warn!("操作失败（第 {} 次尝试）: {}", attempt + 1, error_msg);

                    if !self.is_retryable(&error_msg) {
                        debug!("错误不可重试，放弃重试");
                        return Err(e);
                    }

                    last_error = Some(e);

                    // 计算延迟并等待
                    if let Some(delay) = self.strategy.next_delay(attempt) {
                        debug!("等待 {:?} 后重试", delay);
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_backoff() {
        let strategy = RetryStrategy::exponential(1000, 10000, 2.0);

        assert_eq!(strategy.next_delay(0), Some(Duration::from_millis(1000)));
        assert_eq!(strategy.next_delay(1), Some(Duration::from_millis(2000)));
        assert_eq!(strategy.next_delay(2), Some(Duration::from_millis(4000)));
        assert_eq!(strategy.next_delay(3), Some(Duration::from_millis(8000)));
        assert_eq!(strategy.next_delay(4), Some(Duration::from_millis(10000))); // 限制在最大值
    }

    #[test]
    fn test_fixed_delay() {
        let strategy = RetryStrategy::fixed(2000);

        assert_eq!(strategy.next_delay(0), Some(Duration::from_millis(2000)));
        assert_eq!(strategy.next_delay(1), Some(Duration::from_millis(2000)));
        assert_eq!(strategy.next_delay(2), Some(Duration::from_millis(2000)));
    }

    #[tokio::test]
    async fn test_retry_config_execute() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU32, Ordering};

        let config = RetryConfig {
            max_attempts: 3,
            strategy: RetryStrategy::Immediate,
            retryable_errors: vec![],
        };

        let attempts = Arc::new(AtomicU32::new(0));

        let result = config
            .execute(|| {
                let attempts = Arc::clone(&attempts);
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    let count = attempts.load(Ordering::SeqCst);
                    if count < 3 {
                        Err::<(), _>("error")
                    } else {
                        Ok(())
                    }
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }
}
