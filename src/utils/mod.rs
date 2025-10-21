// Private module declarations
mod circuit_breaker;
mod retry;

// Re-export items used within the crate
pub(crate) use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitState};
pub(crate) use retry::{retry_with_backoff, retry_on_transient, RetryConfig, RetryResult, IsTransient};
