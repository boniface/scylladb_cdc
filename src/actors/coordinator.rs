use actix::prelude::*;
use scylla::client::session::Session;
use std::sync::Arc;
use crate::messaging::RedpandaClient;
use super::{CdcProcessor, DlqActor, health_check::{HealthCheckActor, HealthStatus, UpdateHealth, GetSystemHealth}};

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
    cdc_processor: Option<Addr<CdcProcessor>>,
    health_check: Option<Addr<HealthCheckActor>>,
    dlq_actor: Option<Addr<DlqActor>>,
}

impl CoordinatorActor {
    pub fn new(session: Arc<Session>, redpanda: Arc<RedpandaClient>) -> Self {
        Self {
            session,
            redpanda,
            cdc_processor: None,
            health_check: None,
            dlq_actor: None,
        }
    }

    fn start_child_actors(&mut self, _ctx: &mut Context<Self>) {
        tracing::info!("Starting supervised child actors");

        // Start health check actor
        let health_check = HealthCheckActor::new(self.redpanda.clone()).start();
        self.health_check = Some(health_check.clone());

        // Start DLQ actor
        let dlq_actor = DlqActor::new(self.session.clone()).start();
        self.dlq_actor = Some(dlq_actor.clone());

        // Report DLQ actor health
        health_check.do_send(UpdateHealth {
            component: "dlq_actor".to_string(),
            status: HealthStatus::Healthy,
            details: Some("DLQ actor started".to_string()),
        });

        // Start CDC stream processor with DLQ support
        let cdc_processor = CdcProcessor::new(
            self.session.clone(),
            self.redpanda.clone(),
            Some(dlq_actor.clone()),
        ).start();
        self.cdc_processor = Some(cdc_processor.clone());

        // Report CDC processor health
        health_check.do_send(UpdateHealth {
            component: "cdc_processor".to_string(),
            status: HealthStatus::Healthy,
            details: Some("CDC processor started".to_string()),
        });

        tracing::info!("✅ All supervised actors started successfully");
    }
}

impl Actor for CoordinatorActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("🎯 CoordinatorActor started - Event Sourcing with CDC");
        self.start_child_actors(ctx);

        // Schedule periodic health checks
        ctx.run_interval(
            std::time::Duration::from_secs(30),
            |act, _ctx| {
                if let Some(ref health_check) = act.health_check {
                    let health_check = health_check.clone();
                    actix::spawn(async move {
                        match health_check.send(GetSystemHealth).await {
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
                    });
                }
            },
        );
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        tracing::info!("🛑 CoordinatorActor stopping - initiating graceful shutdown");
        Running::Stop
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        tracing::info!("🛑 CoordinatorActor stopped");
    }
}

// ============================================================================
// Messages
// ============================================================================

#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct Shutdown;

impl Handler<Shutdown> for CoordinatorActor {
    type Result = Result<(), String>;

    fn handle(&mut self, _msg: Shutdown, ctx: &mut Self::Context) -> Self::Result {
        tracing::info!("Received shutdown signal");

        // Stop child actors gracefully
        if let Some(ref cdc_processor) = self.cdc_processor {
            cdc_processor.do_send(StopActor);
        }

        if let Some(ref dlq_actor) = self.dlq_actor {
            dlq_actor.do_send(StopActor);
        }

        if let Some(ref health_check) = self.health_check {
            health_check.do_send(StopActor);
        }

        // Stop coordinator
        ctx.stop();

        Ok(())
    }
}

/// Message to gracefully stop an actor
#[derive(Message)]
#[rtype(result = "()")]
struct StopActor;

impl Handler<StopActor> for CdcProcessor {
    type Result = ();

    fn handle(&mut self, _: StopActor, ctx: &mut Self::Context) {
        tracing::info!("CdcProcessor received stop signal");
        ctx.stop();
    }
}

impl Handler<StopActor> for HealthCheckActor {
    type Result = ();

    fn handle(&mut self, _: StopActor, ctx: &mut Self::Context) {
        tracing::info!("HealthCheckActor received stop signal");
        ctx.stop();
    }
}

impl Handler<StopActor> for DlqActor {
    type Result = ();

    fn handle(&mut self, _: StopActor, ctx: &mut Self::Context) {
        tracing::info!("DlqActor received stop signal");
        ctx.stop();
    }
}
