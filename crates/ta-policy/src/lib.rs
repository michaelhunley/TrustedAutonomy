//! # ta-policy
//!
//! Capability-based policy engine for Trusted Autonomy.
//!
//! Implements the "default deny" security boundary: agents can only perform
//! actions explicitly granted by a [`CapabilityManifest`]. The [`PolicyEngine`]
//! evaluates each tool call request against the agent's grants and returns
//! Allow, Deny, or RequireApproval.
//!
//! ## Key invariants
//!
//! - **Default deny**: no manifest → denied. No matching grant → denied.
//! - **Side effects gated**: verbs like "apply", "commit", "send", "post"
//!   always return RequireApproval, even when granted.
//! - **Path traversal blocked**: URIs containing ".." are always denied.

pub mod capability;
pub mod engine;
pub mod error;

pub use capability::{CapabilityGrant, CapabilityManifest};
pub use engine::{EvaluationStep, EvaluationTrace, PolicyDecision, PolicyEngine, PolicyRequest};
pub use error::PolicyError;
