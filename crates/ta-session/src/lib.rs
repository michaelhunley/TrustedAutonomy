// ta-session — Session & Human Control Plane (Layer 3).
//
// Placeholder — implementation coming in Phase 3.

pub mod error;
pub mod manager;
pub mod session;

pub use error::SessionError;
pub use manager::SessionManager;
pub use session::{ConversationTurn, SessionState, TaSession};
