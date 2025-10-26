use kameo::Actor;
use kameo::message::{Context, Message};
use kameo::actor::ActorRef;
use kameo::error::Infallible;
use kameo::reply::{Reply, ReplyError};
use std::sync::Arc;
use std::collections::HashMap;
use chrono::Utc;
use crate::messaging::RedpandaClient;
use crate::utils::CircuitState;
use crate::actors::core::{HealthStatus, ComponentHealth};

// ============================================================================
// Health Monitor Actor - Monitors system health
// ============================================================================
//
// Responsibilities:
// - Track health status of all components
// - Provide health endpoints for monitoring
// - Detect and report degraded states
// - Aggregate system-wide health
//
// ============================================================================

// ============================================================================
// Messages
// ============================================================================

pub struct UpdateHealth {
    pub component: String,
    pub status: HealthStatus,
    pub details: Option<String>,
}

pub struct GetSystemHealth;

#[derive(Debug, Clone)]
pub struct SystemHealth {
    pub overall_status: HealthStatus,
    pub components: HashMap<String, ComponentHealth>,
    pub check_time: chrono::DateTime<Utc>,
}

// Implement Reply for SystemHealth to use it as a message reply type
impl Reply for SystemHealth {
    type Ok = Self;
    type Error = Infallible;
    type Value = Self;

    fn to_result(self) -> Result<Self, Infallible> {
        Ok(self)
    }

    fn into_any_err(self) -> Option<Box<dyn ReplyError>> {
        None
    }

    fn into_value(self) -> Self::Value {
        self
    }
}

// ============================================================================
// Health Monitor Actor
// ============================================================================

pub struct HealthMonitorActor {
    components: HashMap<String, ComponentHealth>,
    redpanda: Option<Arc<RedpandaClient>>,
}

impl HealthMonitorActor {
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
}

impl Actor for HealthMonitorActor {
    type Args = Self;
    type Error = Infallible;

    async fn on_start(
        state: Self::Args,
        actor_ref: ActorRef<Self>
    ) -> Result<Self, Self::Error> {
        tracing::info!("HealthMonitorActor started");

        // Clone what we need for the periodic task
        let redpanda = state.redpanda.clone();
        let actor_ref_clone = actor_ref.clone();

        // Schedule periodic health checks
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            loop {
                interval.tick().await;

                // Check Redpanda health periodically
                if let Some(ref rp) = redpanda {
                    let status = match rp.get_circuit_breaker_state().await {
                        CircuitState::Closed => HealthStatus::Healthy,
                        CircuitState::HalfOpen => {
                            HealthStatus::Degraded("Circuit breaker half-open".to_string())
                        }
                        CircuitState::Open => {
                            HealthStatus::Unhealthy("Circuit breaker open".to_string())
                        }
                    };

                    // Fire and forget - use tell
                    let _ = actor_ref_clone.tell(UpdateHealth {
                        component: "redpanda".to_string(),
                        status,
                        details: None,
                    }).send().await;
                }
            }
        });

        Ok(state)
    }
}

// ============================================================================
// Message Handlers
// ============================================================================

impl Message<UpdateHealth> for HealthMonitorActor {
    type Reply = ();

    async fn handle(&mut self, msg: UpdateHealth, _ctx: &mut Context<Self, Self::Reply>) -> Self::Reply {
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

impl Message<GetSystemHealth> for HealthMonitorActor {
    type Reply = SystemHealth;

    async fn handle(&mut self, _msg: GetSystemHealth, _ctx: &mut Context<Self, Self::Reply>) -> Self::Reply {
        let overall_status = self.compute_overall_status();

        SystemHealth {
            overall_status,
            components: self.components.clone(),
            check_time: Utc::now(),
        }
    }
}
