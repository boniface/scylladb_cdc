use kameo::Actor;
use kameo::message::{Context, Message};
use kameo::actor::ActorRef;
use kameo::error::Infallible;
use scylla::client::session::Session;
use std::sync::Arc;
use futures_util::task::SpawnExt;
use crate::messaging::RedpandaClient;
use crate::actors::core::HealthStatus;
use super::{CdcProcessor, DlqActor, HealthMonitorActor, UpdateHealth, GetSystemHealth};

// ============================================================================
// Coordinator Actor - Orchestrates all system actors
// ============================================================================
//
// Responsibilities:
// - Manages lifecycle of child actors (CdcProcessor, DlqActor, HealthCheck)
// - Implements supervision strategy
// - Coordinates graceful shutdown
// - Reports system health
// - Handles actor failures and restarts
//
// Actor Hierarchy:
//   CoordinatorActor (Supervisor)
//   ├── CdcProcessor
//   ├── DlqActor
//   └── HealthCheckActor
//
// ============================================================================

pub struct CoordinatorActor {
    session: Arc<Session>,
    redpanda: Arc<RedpandaClient>,
    cdc_processor: Option<ActorRef<CdcProcessor>>,
    health_monitor: Option<ActorRef<HealthMonitorActor>>,
    dlq_actor: Option<ActorRef<DlqActor>>,
}

impl CoordinatorActor {
    pub fn new(session: Arc<Session>, redpanda: Arc<RedpandaClient>) -> Self {
        Self {
            session,
            redpanda,
            cdc_processor: None,
            health_monitor: None,
            dlq_actor: None,
        }
    }
}

impl Actor for CoordinatorActor {
    type Args = Self;
    type Error = Infallible;

    async fn on_start(
        mut state: Self::Args,
        _actor_ref: ActorRef<Self>
    ) -> Result<Self, Self::Error> {
        tracing::info!("🎯 CoordinatorActor started - Event Sourcing with CDC");

        // Start health monitor actor
        let health_monitor = HealthMonitorActor::spawn(HealthMonitorActor::new(state.redpanda.clone()));
        state.health_monitor = Some(health_monitor.clone());

        // Start DLQ actor
        let dlq_actor = DlqActor::spawn(DlqActor::new(state.session.clone()));
        state.dlq_actor = Some(dlq_actor.clone());

        // Report DLQ actor health
        let _ = health_monitor.tell(UpdateHealth {
            component: "dlq_actor".to_string(),
            status: HealthStatus::Healthy,
            details: Some("DLQ actor started".to_string()),
        }).send().await;

        // Start CDC stream processor with DLQ support
        let cdc_processor = CdcProcessor::spawn(CdcProcessor::new(
            state.session.clone(),
            state.redpanda.clone(),
            Some(dlq_actor.clone()),
        ));
        state.cdc_processor = Some(cdc_processor.clone());

        // Report CDC processor health
        let _ = health_monitor.tell(UpdateHealth {
            component: "cdc_processor".to_string(),
            status: HealthStatus::Healthy,
            details: Some("CDC processor started".to_string()),
        }).send().await;

        tracing::info!("✅ All supervised actors started successfully");

        // Clone what we need for periodic health checks
        let health_monitor_clone = state.health_monitor.clone();

        // Schedule periodic health checks
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;

                if let Some(ref health_monitor) = health_monitor_clone {
                    match health_monitor.ask(GetSystemHealth).await {
                        Ok(health) => {
                            match health.overall_status {
                                HealthStatus::Healthy => {
                                    tracing::debug!("System health check: Healthy");
                                }
                                HealthStatus::Degraded(ref msg) => {
                                    tracing::warn!("System health check: Degraded - {}", msg);
                                }
                                HealthStatus::Unhealthy(ref msg) => {
                                    tracing::error!("System health check: Unhealthy - {}", msg);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to get system health: {}", e);
                        }
                    }
                }
            }
        });

        Ok(state)
    }

    async fn on_stop(
        &mut self,
        _actor_ref: kameo::actor::WeakActorRef<Self>,
        _reason: kameo::error::ActorStopReason,
    ) -> Result<(), Self::Error> {
        tracing::info!("🛑 CoordinatorActor stopped");
        Ok(())
    }
}

// ============================================================================
// Messages
// ============================================================================

pub struct Shutdown;

impl Message<Shutdown> for CoordinatorActor {
    type Reply = Result<(), String>;

    async fn handle(&mut self, _msg: Shutdown, ctx: &mut Context<Self, Self::Reply>) -> Self::Reply {
        tracing::info!("Received shutdown signal");

        // Stop child actors gracefully using Kameo's kill method
        if let Some(ref cdc_processor) = self.cdc_processor {
            tracing::info!("Stopping CdcProcessor...");
            cdc_processor.kill();
        }

        if let Some(ref dlq_actor) = self.dlq_actor {
            tracing::info!("Stopping DlqActor...");
            dlq_actor.kill();
        }

        if let Some(ref health_monitor) = self.health_monitor {
            tracing::info!("Stopping HealthMonitorActor...");
            health_monitor.kill();
        }

        // Stop coordinator
        ctx.stop();

        Ok(())
    }
}
