use chrono::{DateTime, Utc};

// ============================================================================
// Health Check Abstractions
// ============================================================================
//
// Core traits for health monitoring that any actor can implement.
// This allows the health monitoring system to work with any actor type.
//
// ============================================================================

/// Health status of a component
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded(String),
    Unhealthy(String),
}

impl HealthStatus {
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy)
    }

    pub fn is_degraded(&self) -> bool {
        matches!(self, HealthStatus::Degraded(_))
    }

    pub fn is_unhealthy(&self) -> bool {
        matches!(self, HealthStatus::Unhealthy(_))
    }
}

/// Health information for a component
#[derive(Debug, Clone)]
pub struct ComponentHealth {
    pub name: String,
    pub status: HealthStatus,
    pub last_check: DateTime<Utc>,
    pub details: Option<String>,
}

impl ComponentHealth {
    pub fn new(name: impl Into<String>, status: HealthStatus) -> Self {
        Self {
            name: name.into(),
            status,
            last_check: Utc::now(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

/// Trait for actors that can report their health status
pub trait HealthCheckable {
    /// Get the current health status
    fn check_health(&self) -> ComponentHealth;

    /// Get the component name
    fn component_name(&self) -> &str;
}
