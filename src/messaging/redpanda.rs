use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    config::ClientConfig,
};
use anyhow::Result;
use crate::utils::{CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError};

pub struct RedpandaClient {
    producer: FutureProducer,
    circuit_breaker: CircuitBreaker,
}

impl RedpandaClient {
    pub fn new(brokers: &str) -> Self {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .create()
            .expect("Failed to create Redpanda producer");

        // Configure circuit breaker for Redpanda
        let cb_config = CircuitBreakerConfig {
            failure_threshold: 5,           // Open after 5 failures
            timeout: std::time::Duration::from_secs(30),  // Wait 30s before retry
            success_threshold: 3,           // Need 3 successes to close
        };

        Self {
            producer,
            circuit_breaker: CircuitBreaker::new(cb_config),
        }
    }

    pub async fn publish(&self, topic: &str, key: &str, payload: &str) -> Result<()> {
        let topic = topic.to_string();
        let key = key.to_string();
        let payload = payload.to_string();

        // Use circuit breaker to protect against Redpanda failures
        let result = self.circuit_breaker.call(async {
            let record = FutureRecord::to(&topic)
                .key(&key)
                .payload(&payload);

            self.producer
                .send(record, rdkafka::util::Timeout::After(std::time::Duration::from_secs(5)))
                .await
                .map_err(|(e, _)| anyhow::anyhow!("Kafka send error: {}", e))?;

            Ok::<(), anyhow::Error>(())
        }).await;

        match result {
            Ok(_) => {
                tracing::info!(
                    topic = %topic,
                    key = %key,
                    "Published to Redpanda"
                );
                Ok(())
            }
            Err(CircuitBreakerError::CircuitOpen) => {
                tracing::error!(
                    topic = %topic,
                    "Circuit breaker open - Redpanda unavailable"
                );
                Err(anyhow::anyhow!("Circuit breaker open for Redpanda"))
            }
            Err(CircuitBreakerError::OperationFailed(e)) => {
                tracing::error!(
                    error = %e,
                    topic = %topic,
                    "Failed to publish to Redpanda"
                );
                Err(e)
            }
        }
    }

    pub async fn get_circuit_breaker_state(&self) -> crate::utils::CircuitState {
        self.circuit_breaker.get_state().await
    }

    pub async fn reset_circuit_breaker(&self) {
        self.circuit_breaker.reset().await;
    }
}