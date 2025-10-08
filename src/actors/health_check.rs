use actix::prelude::*;
use std::sync::Arc;
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use crate::messaging::RedpandaClient;
use crate::utils::CircuitState;

// ============================================================================
// Health Check Actor - Monitors system health
// ============================================================================
//
// Responsibilities:
// - Track health status of all components
// - Provide health endpoints for monitoring
// - Detect and report degraded states
// - Aggregate system-wide health
//
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded(String),
    Unhealthy(String),
}

#[derive(Debug, Clone)]
pub struct ComponentHealth {
    pub name: String,
    pub status: HealthStatus,
    pub last_check: DateTime<Utc>,
    pub details: Option<String>,
}

// ============================================================================
// Messages
// ============================================================================

#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateHealth {
    pub component: String,
    pub status: HealthStatus,
    pub details: Option<String>,
}

#[derive(Message)]
#[rtype(result = "SystemHealth")]
pub struct GetSystemHealth;

#[derive(Debug, Clone)]
pub struct SystemHealth {
    pub overall_status: HealthStatus,
    pub components: HashMap<String, ComponentHealth>,
    pub check_time: DateTime<Utc>,
}

// ============================================================================
// Health Check Actor
// ============================================================================

pub struct HealthCheckActor {
    components: HashMap<String, ComponentHealth>,
    redpanda: Option<Arc<RedpandaClient>>,
}

impl HealthCheckActor {
    pub fn new(redpanda: Arc<RedpandaClient>) -> Self {
        Self {
            components: HashMap::new(),
            redpanda: Some(redpanda),
        }
    }

    fn compute_overall_status(&self) -> HealthStatus {
        let mut has_degraded = false;
        let mut unhealthy_components = Vec::new();

        for (name, health) in &self.components {
            match &health.status {
                HealthStatus::Unhealthy(msg) => {
                    unhealthy_components.push(format!("{}: {}", name, msg));
                }
                HealthStatus::Degraded(_) => {
                    has_degraded = true;
                }
                HealthStatus::Healthy => {}
            }
        }

        if !unhealthy_components.is_empty() {
            HealthStatus::Unhealthy(unhealthy_components.join(", "))
        } else if has_degraded {
            HealthStatus::Degraded("Some components degraded".to_string())
        } else {
            HealthStatus::Healthy
        }
    }

    async fn check_redpanda_health(&self) -> HealthStatus {
        if let Some(ref redpanda) = self.redpanda {
            let cb_state = redpanda.get_circuit_breaker_state().await;
            match cb_state {
                CircuitState::Closed => HealthStatus::Healthy,
                CircuitState::HalfOpen => {
                    HealthStatus::Degraded("Circuit breaker half-open".to_string())
                }
                CircuitState::Open => {
                    HealthStatus::Unhealthy("Circuit breaker open".to_string())
                }
            }
        } else {
            HealthStatus::Unhealthy("Redpanda client not initialized".to_string())
        }
    }
}

impl Actor for HealthCheckActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("HealthCheckActor started");

        // Get address before borrowing ctx
        let addr = ctx.address();

        // Schedule periodic health checks
        ctx.run_interval(
            std::time::Duration::from_secs(10),
            move |act, _ctx| {
                // Check Redpanda health periodically
                let redpanda = act.redpanda.clone();
                let addr = addr.clone();

                actix::spawn(async move {
                    if let Some(rp) = redpanda {
                        let status = match rp.get_circuit_breaker_state().await {
                            CircuitState::Closed => HealthStatus::Healthy,
                            CircuitState::HalfOpen => {
                                HealthStatus::Degraded("Circuit breaker half-open".to_string())
                            }
                            CircuitState::Open => {
                                HealthStatus::Unhealthy("Circuit breaker open".to_string())
                            }
                        };

                        addr.do_send(UpdateHealth {
                            component: "redpanda".to_string(),
                            status,
                            details: None,
                        });
                    }
                });
            },
        );
    }
}

impl Handler<UpdateHealth> for HealthCheckActor {
    type Result = ();

    fn handle(&mut self, msg: UpdateHealth, _: &mut Self::Context) {
        let health = ComponentHealth {
            name: msg.component.clone(),
            status: msg.status.clone(),
            last_check: Utc::now(),
            details: msg.details,
        };

        tracing::debug!(
            component = %msg.component,
            status = ?msg.status,
            "Updated component health"
        );

        self.components.insert(msg.component, health);
    }
}

impl Handler<GetSystemHealth> for HealthCheckActor {
    type Result = MessageResult<GetSystemHealth>;

    fn handle(&mut self, _msg: GetSystemHealth, _: &mut Self::Context) -> Self::Result {
        let overall_status = self.compute_overall_status();

        MessageResult(SystemHealth {
            overall_status,
            components: self.components.clone(),
            check_time: Utc::now(),
        })
    }
}
