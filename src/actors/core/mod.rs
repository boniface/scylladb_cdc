// ============================================================================
// Core Actor Abstractions
// ============================================================================
//
// Generic, reusable actor traits and types.
// Infrastructure actors implement these traits.
//
// ============================================================================

pub mod health;
pub mod supervised;

// Re-export core types
pub use health::*;
pub use supervised::*;
