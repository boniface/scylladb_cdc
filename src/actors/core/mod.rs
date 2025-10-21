// ============================================================================
// Core Actor Abstractions
// ============================================================================
//
// Generic, reusable actor traits and types.
// Infrastructure actors implement these traits.
//
// ============================================================================

mod health;
mod supervised;

// Re-export core types
pub use health::*;
pub use supervised::*;
