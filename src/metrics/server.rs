use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use prometheus::{Encoder, Registry, TextEncoder};
use std::sync::Arc;

/// Start the metrics HTTP server
/// This should be called in a separate thread/runtime to avoid conflicts
pub async fn start_metrics_server(registry: Arc<Registry>, port: u16) -> std::io::Result<()> {
    tracing::info!("📊 Starting metrics server on http://0.0.0.0:{}/metrics", port);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(registry.clone()))
            .route("/metrics", web::get().to(metrics_handler))
            .route("/health", web::get().to(health_handler))
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}

async fn metrics_handler(registry: web::Data<Arc<Registry>>) -> impl Responder {
    let encoder = TextEncoder::new();
    let metric_families = registry.gather();

    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();

    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(buffer)
}

async fn health_handler() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "service": "scylladb-cdc-outbox"
    }))
}
