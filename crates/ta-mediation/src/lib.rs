// ta-mediation — Resource mediation trait (Layer 1).
//
// Generalizes the staging pattern from files to any resource. Every state-changing
// action an agent proposes is staged before it touches the real world.
//
// This crate defines:
// - `ResourceMediator` trait — the core abstraction
// - `FsMediator` — built-in filesystem mediator (wraps existing staging workspace)
// - `MediatorRegistry` — routes URIs to the correct mediator
// - Core types: ProposedAction, StagedMutation, MutationPreview, ActionClassification

pub mod api_mediator;
pub mod error;
pub mod fs_mediator;
pub mod mediator;
pub mod registry;

// Re-export primary types for convenience.
pub use api_mediator::ApiMediator;
pub use error::MediationError;
pub use fs_mediator::FsMediator;
pub use mediator::{
    ActionClassification, ApplyResult, MutationPreview, ProposedAction, ResourceMediator,
    StagedMutation,
};
pub use registry::MediatorRegistry;
