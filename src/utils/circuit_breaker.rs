use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::{Duration, Instant};

// ============================================================================
// Circuit Breaker Pattern Implementation
// ============================================================================
//
// Prevents cascading failures by tracking errors and temporarily blocking
// requests when a service is unhealthy.
//
// States:
// - Closed: Normal operation, requests pass through
// - Open: Too many failures, requests blocked immediately
// - HalfOpen: Testing if service recovered, limited requests allowed
//
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitState {
    Closed,     // Normal operation
    Open,       // Blocking requests
    HalfOpen,   // Testing recovery
}

#[derive(Clone)]
pub struct CircuitBreaker {
    state: Arc<Mutex<CircuitBreakerState>>,
    config: CircuitBreakerConfig,
}

#[derive(Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening circuit
    pub failure_threshold: u32,
    /// Time to wait before attempting recovery
    pub timeout: Duration,
    /// Number of successes needed to close circuit from half-open
    pub success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            timeout: Duration::from_secs(60),
            success_threshold: 2,
        }
    }
}

struct CircuitBreakerState {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<Instant>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(CircuitBreakerState {
                state: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
                last_failure_time: None,
            })),
            config,
        }
    }

    /// Execute an operation with circuit breaker protection
    pub async fn call<F, T, E>(&self, operation: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: std::future::Future<Output = Result<T, E>>,
    {
        // Check if circuit allows the call
        {
            let mut state = self.state.lock().await;

            match state.state {
                CircuitState::Open => {
                    // Check if timeout has elapsed
                    if let Some(last_failure) = state.last_failure_time {
                        if last_failure.elapsed() >= self.config.timeout {
                            tracing::info!("Circuit breaker transitioning to HalfOpen");
                            state.state = CircuitState::HalfOpen;
                            state.success_count = 0;
                        } else {
                            return Err(CircuitBreakerError::CircuitOpen);
                        }
                    }
                }
                CircuitState::HalfOpen | CircuitState::Closed => {
                    // Allow the call
                }
            }
        }

        // Execute the operation
        match operation.await {
            Ok(result) => {
                self.record_success().await;
                Ok(result)
            }
            Err(err) => {
                self.record_failure().await;
                Err(CircuitBreakerError::OperationFailed(err))
            }
        }
    }

    async fn record_success(&self) {
        let mut state = self.state.lock().await;

        match state.state {
            CircuitState::HalfOpen => {
                state.success_count += 1;
                if state.success_count >= self.config.success_threshold {
                    tracing::info!("Circuit breaker closing after {} successes", state.success_count);
                    state.state = CircuitState::Closed;
                    state.failure_count = 0;
                    state.success_count = 0;
                    state.last_failure_time = None;
                }
            }
            CircuitState::Closed => {
                // Reset failure count on success
                state.failure_count = 0;
            }
            CircuitState::Open => {
                // Should not happen, but reset if it does
                tracing::warn!("Success recorded while circuit is open");
            }
        }
    }

    async fn record_failure(&self) {
        let mut state = self.state.lock().await;

        state.failure_count += 1;
        state.last_failure_time = Some(Instant::now());

        match state.state {
            CircuitState::Closed => {
                if state.failure_count >= self.config.failure_threshold {
                    tracing::warn!(
                        "Circuit breaker opening after {} failures",
                        state.failure_count
                    );
                    state.state = CircuitState::Open;
                }
            }
            CircuitState::HalfOpen => {
                tracing::warn!("Failure during half-open, reopening circuit");
                state.state = CircuitState::Open;
                state.success_count = 0;
            }
            CircuitState::Open => {
                // Already open, just update the time
            }
        }
    }

    pub async fn get_state(&self) -> CircuitState {
        let state = self.state.lock().await;
        state.state
    }

    pub async fn get_failure_count(&self) -> u32 {
        let state = self.state.lock().await;
        state.failure_count
    }

    /// Manually reset the circuit breaker
    pub async fn reset(&self) {
        let mut state = self.state.lock().await;
        tracing::info!("Circuit breaker manually reset");
        state.state = CircuitState::Closed;
        state.failure_count = 0;
        state.success_count = 0;
        state.last_failure_time = None;
    }
}

#[derive(Debug)]
pub enum CircuitBreakerError<E> {
    CircuitOpen,
    OperationFailed(E),
}

impl<E: std::fmt::Display> std::fmt::Display for CircuitBreakerError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitBreakerError::CircuitOpen => write!(f, "Circuit breaker is open"),
            CircuitBreakerError::OperationFailed(e) => write!(f, "Operation failed: {}", e),
        }
    }
}

impl<E: std::error::Error> std::error::Error for CircuitBreakerError<E> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_opens_after_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            timeout: Duration::from_secs(1),
            success_threshold: 2,
        };
        let cb = CircuitBreaker::new(config);

        // First 3 failures should open the circuit
        for _ in 0..3 {
            let result = cb.call(async { Err::<(), _>("error") }).await;
            assert!(result.is_err());
        }

        assert_eq!(cb.get_state().await, CircuitState::Open);

        // Next call should fail immediately
        let result = cb.call(async { Ok::<_, &str>(()) }).await;
        assert!(matches!(result, Err(CircuitBreakerError::CircuitOpen)));
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_after_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            timeout: Duration::from_millis(100),
            success_threshold: 1,
        };
        let cb = CircuitBreaker::new(config);

        // Open the circuit
        for _ in 0..2 {
            let _ = cb.call(async { Err::<(), _>("error") }).await;
        }

        assert_eq!(cb.get_state().await, CircuitState::Open);

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should transition to half-open and allow the call
        let result = cb.call(async { Ok::<_, &str>(()) }).await;
        assert!(result.is_ok());

        // After success threshold, should be closed
        assert_eq!(cb.get_state().await, CircuitState::Closed);
    }
}
