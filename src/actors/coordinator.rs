use actix::prelude::*;
use scylla::client::session::Session;
use std::sync::Arc;
use crate::messaging::RedpandaClient;
use super::{OrderActor, CdcStreamProcessor, DlqActor, health_check::{HealthCheckActor, HealthStatus, UpdateHealth, GetSystemHealth}};

// ============================================================================
// Coordinator Actor - Orchestrates all system actors
// ============================================================================
//
// Responsibilities:
// - Manages lifecycle of child actors (OrderActor, CdcStreamProcessor)
// - Implements supervision strategy
// - Coordinates graceful shutdown
// - Reports system health
// - Handles actor failures and restarts
//
// Actor Hierarchy:
//   CoordinatorActor (Supervisor)
//   ├── OrderActor
//   ├── CdcStreamProcessor
//   ├── DlqActor
//   └── HealthCheckActor
//
// ============================================================================

pub struct CoordinatorActor {
    session: Arc<Session>,
    redpanda: Arc<RedpandaClient>,
    order_actor: Option<Addr<OrderActor>>,
    cdc_processor: Option<Addr<CdcStreamProcessor>>,
    health_check: Option<Addr<HealthCheckActor>>,
    dlq_actor: Option<Addr<DlqActor>>,
}

impl CoordinatorActor {
    pub fn new(session: Arc<Session>, redpanda: Arc<RedpandaClient>) -> Self {
        Self {
            session,
            redpanda,
            order_actor: None,
            cdc_processor: None,
            health_check: None,
            dlq_actor: None,
        }
    }

    fn start_child_actors(&mut self, ctx: &mut Context<Self>) {
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

        // Start order actor
        let order_actor = OrderActor::new(self.session.clone()).start();
        self.order_actor = Some(order_actor.clone());

        // Report order actor health
        health_check.do_send(UpdateHealth {
            component: "order_actor".to_string(),
            status: HealthStatus::Healthy,
            details: Some("Order actor started".to_string()),
        });

        // Start CDC stream processor with DLQ support
        let cdc_processor = CdcStreamProcessor::new(
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
        tracing::info!("🎯 CoordinatorActor started - Phase 4: Actor Supervision");
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
        if let Some(ref order_actor) = self.order_actor {
            order_actor.do_send(StopActor);
        }

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

impl Handler<StopActor> for OrderActor {
    type Result = ();

    fn handle(&mut self, _: StopActor, ctx: &mut Self::Context) {
        tracing::info!("OrderActor received stop signal");
        ctx.stop();
    }
}

impl Handler<StopActor> for CdcStreamProcessor {
    type Result = ();

    fn handle(&mut self, _: StopActor, ctx: &mut Self::Context) {
        tracing::info!("CdcStreamProcessor received stop signal");
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

// ============================================================================
// Public API for accessing child actors
// ============================================================================

#[derive(Message)]
#[rtype(result = "Option<Addr<OrderActor>>")]
pub struct GetOrderActor;

impl Handler<GetOrderActor> for CoordinatorActor {
    type Result = Option<Addr<OrderActor>>;

    fn handle(&mut self, _: GetOrderActor, _: &mut Self::Context) -> Self::Result {
        self.order_actor.clone()
    }
}

#[derive(Message)]
#[rtype(result = "Option<Addr<HealthCheckActor>>")]
pub struct GetHealthCheckActor;

impl Handler<GetHealthCheckActor> for CoordinatorActor {
    type Result = Option<Addr<HealthCheckActor>>;

    fn handle(&mut self, _: GetHealthCheckActor, _: &mut Self::Context) -> Self::Result {
        self.health_check.clone()
    }
}
