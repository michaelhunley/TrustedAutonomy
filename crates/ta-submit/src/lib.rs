//! Submit adapters for VCS integration
//!
//! This crate provides pluggable adapters for submitting changes through different
//! version control systems and workflows. The core abstraction is the `SubmitAdapter`
//! trait, with built-in implementations for Git and a "none" fallback.

pub mod adapter;
pub mod config;
pub mod git;
pub mod none;

pub use adapter::{CommitResult, PushResult, ReviewResult, SubmitAdapter};
pub use config::{DiffConfig, GitConfig, SubmitConfig, WorkflowConfig};
pub use git::GitAdapter;
pub use none::NoneAdapter;
