use std::time::Duration;
use tokio::time::sleep;

// ============================================================================
// Exponential Backoff Retry Strategy
// ============================================================================
//
// Implements retry logic with exponential backoff for transient failures.
// Useful for handling temporary network issues, service unavailability, etc.
//
// ============================================================================

#[derive(Clone, Debug)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial delay before first retry
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Multiplier for exponential backoff
    pub multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// Create a retry config for high-value operations (more retries)
    pub fn aggressive() -> Self {
        Self {
            max_attempts: 5,
            initial_delay: Duration::from_millis(50),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }

    /// Create a retry config for quick failures (fewer retries)
    pub fn conservative() -> Self {
        Self {
            max_attempts: 2,
            initial_delay: Duration::from_millis(200),
            max_delay: Duration::from_secs(5),
            multiplier: 2.0,
        }
    }
}

/// Result of a retry operation
#[derive(Debug)]
pub enum RetryResult<T, E> {
    /// Operation succeeded
    Success(T),
    /// Operation failed after all retries
    Failed(E),
    /// Operation permanently failed (should not retry)
    PermanentFailure(E),
}

/// Execute an operation with exponential backoff retry
pub async fn retry_with_backoff<F, Fut, T, E>(
    config: RetryConfig,
    mut operation: F,
) -> RetryResult<T, E>
where
    F: FnMut(u32) -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0;
    let mut delay = config.initial_delay;

    loop {
        attempt += 1;

        tracing::debug!(
            attempt = attempt,
            max_attempts = config.max_attempts,
            "Attempting operation"
        );

        match operation(attempt).await {
            Ok(result) => {
                if attempt > 1 {
                    tracing::info!(
                        attempt = attempt,
                        "Operation succeeded after retry"
                    );
                }
                return RetryResult::Success(result);
            }
            Err(error) => {
                // Check if we should retry
                if attempt >= config.max_attempts {
                    tracing::error!(
                        attempt = attempt,
                        error = %error,
                        "Operation failed after all retries"
                    );
                    return RetryResult::Failed(error);
                }

                tracing::warn!(
                    attempt = attempt,
                    error = %error,
                    delay_ms = delay.as_millis(),
                    "Operation failed, retrying after delay"
                );

                // Wait before next attempt
                sleep(delay).await;

                // Calculate next delay with exponential backoff
                delay = Duration::from_millis(
                    ((delay.as_millis() as f64) * config.multiplier) as u64
                );
                delay = delay.min(config.max_delay);
            }
        }
    }
}

/// Check if an error is transient (should retry) or permanent (should not retry)
pub trait IsTransient {
    fn is_transient(&self) -> bool;
}

/// Retry with transient error checking
pub async fn retry_on_transient<F, Fut, T, E>(
    config: RetryConfig,
    mut operation: F,
) -> RetryResult<T, E>
where
    F: FnMut(u32) -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display + IsTransient,
{
    let mut attempt = 0;
    let mut delay = config.initial_delay;

    loop {
        attempt += 1;

        match operation(attempt).await {
            Ok(result) => {
                if attempt > 1 {
                    tracing::info!(
                        attempt = attempt,
                        "Operation succeeded after retry"
                    );
                }
                return RetryResult::Success(result);
            }
            Err(error) => {
                // Check if error is transient
                if !error.is_transient() {
                    tracing::error!(
                        error = %error,
                        "Permanent failure detected, not retrying"
                    );
                    return RetryResult::PermanentFailure(error);
                }

                // Check if we should retry
                if attempt >= config.max_attempts {
                    tracing::error!(
                        attempt = attempt,
                        error = %error,
                        "Operation failed after all retries"
                    );
                    return RetryResult::Failed(error);
                }

                tracing::warn!(
                    attempt = attempt,
                    error = %error,
                    delay_ms = delay.as_millis(),
                    "Transient failure, retrying after delay"
                );

                sleep(delay).await;

                delay = Duration::from_millis(
                    ((delay.as_millis() as f64) * config.multiplier) as u64
                );
                delay = delay.min(config.max_delay);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_succeeds_eventually() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            multiplier: 2.0,
        };

        let result = retry_with_backoff(config, |_attempt| {
            let counter = counter_clone.clone();
            async move {
                let count = counter.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err("temporary failure")
                } else {
                    Ok("success")
                }
            }
        })
        .await;

        assert!(matches!(result, RetryResult::Success("success")));
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_fails_after_max_attempts() {
        let config = RetryConfig {
            max_attempts: 2,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            multiplier: 2.0,
        };

        let result = retry_with_backoff(config, |_attempt| async {
            Err::<(), _>("persistent failure")
        })
        .await;

        assert!(matches!(result, RetryResult::Failed(_)));
    }
}
