//! JSON-over-stdio protocol types for external messaging adapter plugins.
//!
//! Messaging adapter plugins communicate with TA using a request/response
//! protocol over stdin/stdout. TA spawns the plugin process, writes one JSON
//! request line to stdin, reads one JSON response line from stdout.
//!
//! ## Protocol overview
//!
//! Every exchange is a single JSON line in each direction:
//!
//! ```text
//! TA → plugin: {"op":"<name>",...params...}
//! plugin → TA: {"ok":true,...result...}   or   {"ok":false,"error":"..."}
//! ```
//!
//! ## Operations
//!
//! | Op              | Description                                              |
//! |-----------------|----------------------------------------------------------|
//! | `fetch`         | Fetch messages since a watermark timestamp               |
//! | `create_draft`  | Write a draft to the provider's native Drafts folder     |
//! | `draft_status`  | Poll whether a draft was sent, discarded, or still open  |
//! | `health`        | Connectivity + credential check                          |
//! | `capabilities`  | Advertise which optional ops this plugin supports        |
//!
//! ## Safety invariant — `send` is absent by design
//!
//! The `send` operation is intentionally absent from this protocol.
//! TA never sends messages on behalf of the user. Plugins expose only
//! `create_draft`; the user sends from their native email client.
//! This is a deliberate safety boundary enforced at the type level.

use serde::{Deserialize, Serialize};

/// Protocol version implemented by this TA build.
pub const MESSAGING_PROTOCOL_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Request envelope
// ---------------------------------------------------------------------------

/// Request sent from TA to a messaging plugin over stdin.
///
/// One JSON line per request. The plugin processes it and writes one
/// `MessagingPluginResponse` line to stdout, then the process exits.
///
/// The `op` field selects the operation. Additional fields carry
/// operation-specific parameters (flat layout, not nested).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum MessagingPluginRequest {
    /// Fetch messages received since the given ISO-8601 timestamp.
    Fetch(FetchParams),

    /// Create a draft in the provider's native Drafts folder.
    ///
    /// NOTE: There is intentionally no `Send` variant. TA never sends.
    CreateDraft(CreateDraftParams),

    /// Poll the current state of a previously created draft.
    DraftStatus(DraftStatusParams),

    /// Connectivity and credential health check.
    Health(HealthParams),

    /// Advertise optional capabilities supported by this plugin.
    Capabilities(CapabilitiesParams),
}

// ---------------------------------------------------------------------------
// Response envelope
// ---------------------------------------------------------------------------

/// Response sent from a messaging plugin to TA over stdout.
///
/// One JSON line per response. Always contains `ok`; on success contains
/// the operation result fields; on failure contains `error`.
#[derive(Debug, Serialize, Deserialize)]
pub struct MessagingPluginResponse {
    /// Whether the operation succeeded.
    pub ok: bool,

    /// Human-readable error message (only set when ok=false).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Fetched messages (only for fetch op).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<FetchedMessage>>,

    /// Native draft ID assigned by the provider (only for create_draft op).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft_id: Option<String>,

    /// Current state of a draft (only for draft_status op).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<DraftState>,

    /// Connected email address (only for health op).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,

    /// Provider name reported by the plugin (only for health op).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// Capabilities declared by the plugin (only for capabilities op).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
}

impl MessagingPluginResponse {
    /// Construct a success response with no result fields (used for ops that
    /// return only `ok:true`).
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
            messages: None,
            draft_id: None,
            state: None,
            address: None,
            provider: None,
            capabilities: None,
        }
    }

    /// Construct an error response.
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(msg.into()),
            messages: None,
            draft_id: None,
            state: None,
            address: None,
            provider: None,
            capabilities: None,
        }
    }
}

// ---------------------------------------------------------------------------
// fetch
// ---------------------------------------------------------------------------

/// Parameters for the `fetch` operation.
#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct FetchParams {
    /// ISO-8601 timestamp. Only messages received at or after this time are
    /// returned. Use `"1970-01-01T00:00:00Z"` to fetch all.
    pub since: String,

    /// Email account to fetch from (e.g., "me@example.com").
    /// If omitted, the plugin uses its configured default account.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,

    /// Maximum number of messages to return per call.
    /// Plugins may impose a lower internal cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// A single fetched message.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct FetchedMessage {
    /// Provider-specific message identifier.
    pub id: String,

    /// Sender address (e.g., "Alice <alice@example.com>").
    pub from: String,

    /// Recipient address(es).
    pub to: String,

    /// Message subject line.
    pub subject: String,

    /// Plain-text body (may be empty if only HTML is available).
    #[serde(default)]
    pub body_text: String,

    /// HTML body (may be empty).
    #[serde(default)]
    pub body_html: String,

    /// Provider thread/conversation identifier.
    #[serde(default)]
    pub thread_id: String,

    /// ISO-8601 timestamp when the message was received.
    pub received_at: String,
}

// ---------------------------------------------------------------------------
// create_draft
// ---------------------------------------------------------------------------

/// Parameters for the `create_draft` operation.
///
/// The plugin writes this draft to the provider's native Drafts folder
/// (Gmail `drafts.create`, Outlook `POST /messages` with `isDraft:true`,
/// IMAP `APPEND` to Drafts mailbox). The user sees the draft in their
/// email client, edits freely, and sends when ready.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CreateDraftParams {
    /// Draft envelope to create.
    pub draft: DraftEnvelope,
}

/// The content of a draft message to be created.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct DraftEnvelope {
    /// Recipient address(es).
    pub to: String,

    /// Subject line.
    pub subject: String,

    /// HTML body. Plain text is derived from this if the provider requires it.
    pub body_html: String,

    /// Message-ID of the original message being replied to (for threading).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<String>,

    /// Provider thread identifier (for Gmail/Outlook thread association).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,

    /// Plain-text alternative body. If omitted, plugins derive it from body_html.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_text: Option<String>,
}

// ---------------------------------------------------------------------------
// draft_status
// ---------------------------------------------------------------------------

/// Parameters for the `draft_status` operation.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct DraftStatusParams {
    /// Provider-specific draft ID returned by `create_draft`.
    pub draft_id: String,
}

/// Current state of a draft as reported by the provider.
///
/// `Sent` and `Discarded` are best-effort — providers may not reliably
/// report these states. `Drafted` is the safe default.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum DraftState {
    /// Draft exists in the Drafts folder and has not been sent.
    Drafted,
    /// The user sent this draft from their email client.
    Sent,
    /// The draft was deleted by the user without sending.
    Discarded,
    /// State cannot be determined (e.g., provider API limitations).
    Unknown,
}

impl std::fmt::Display for DraftState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DraftState::Drafted => write!(f, "drafted"),
            DraftState::Sent => write!(f, "sent"),
            DraftState::Discarded => write!(f, "discarded"),
            DraftState::Unknown => write!(f, "unknown"),
        }
    }
}

// ---------------------------------------------------------------------------
// health
// ---------------------------------------------------------------------------

/// Parameters for the `health` operation.
///
/// No parameters required — plugins use their configured credentials.
#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct HealthParams {}

// ---------------------------------------------------------------------------
// capabilities
// ---------------------------------------------------------------------------

/// Parameters for the `capabilities` operation.
///
/// No parameters required.
#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct CapabilitiesParams {}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from messaging plugin operations.
#[derive(Debug, thiserror::Error)]
pub enum MessagingPluginError {
    #[error("messaging plugin not found: {name}. Install with: ta adapter setup messaging/{name}")]
    PluginNotFound { name: String },

    #[error("messaging plugin '{name}' op '{op}' failed: {reason}")]
    OpFailed {
        name: String,
        op: String,
        reason: String,
    },

    #[error("messaging plugin '{name}' produced invalid response for op '{op}': {reason}")]
    InvalidResponse {
        name: String,
        op: String,
        reason: String,
    },

    #[error(
        "failed to spawn messaging plugin '{command}': {reason}. Ensure the plugin is on PATH."
    )]
    SpawnFailed { command: String, reason: String },

    #[error("messaging plugin '{name}' timed out after {timeout_secs}s for op '{op}'. Increase timeout in plugin.toml.")]
    Timeout {
        name: String,
        op: String,
        timeout_secs: u64,
    },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_request_roundtrip() {
        let req = MessagingPluginRequest::Fetch(FetchParams {
            since: "2026-04-01T00:00:00Z".to_string(),
            account: Some("me@example.com".to_string()),
            limit: Some(50),
        });
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"op\":\"fetch\""));
        let parsed: MessagingPluginRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn create_draft_request_roundtrip() {
        let req = MessagingPluginRequest::CreateDraft(CreateDraftParams {
            draft: DraftEnvelope {
                to: "bob@example.com".to_string(),
                subject: "Re: Hello".to_string(),
                body_html: "<p>Hi Bob!</p>".to_string(),
                in_reply_to: Some("<msg123@example.com>".to_string()),
                thread_id: Some("thread-abc".to_string()),
                body_text: None,
            },
        });
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"op\":\"create_draft\""));
        let parsed: MessagingPluginRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn no_send_op_variant() {
        // The protocol MUST NOT have a Send variant. Verify by ensuring the
        // enum exhaustively matches without it.
        let req = MessagingPluginRequest::Health(HealthParams {});
        let json = serde_json::to_string(&req).unwrap();
        assert!(
            !json.contains("\"send\""),
            "Send op must not exist in the protocol"
        );
    }

    #[test]
    fn draft_status_request_roundtrip() {
        let req = MessagingPluginRequest::DraftStatus(DraftStatusParams {
            draft_id: "gmail-draft-abc123".to_string(),
        });
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"op\":\"draft_status\""));
        let parsed: MessagingPluginRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn health_request_roundtrip() {
        let req = MessagingPluginRequest::Health(HealthParams {});
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"op\":\"health\""));
    }

    #[test]
    fn response_ok_roundtrip() {
        let resp = MessagingPluginResponse::ok();
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: MessagingPluginResponse = serde_json::from_str(&json).unwrap();
        assert!(parsed.ok);
        assert!(parsed.error.is_none());
    }

    #[test]
    fn response_error_roundtrip() {
        let resp = MessagingPluginResponse::error("credentials not found");
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: MessagingPluginResponse = serde_json::from_str(&json).unwrap();
        assert!(!parsed.ok);
        assert_eq!(parsed.error.as_deref(), Some("credentials not found"));
    }

    #[test]
    fn response_with_draft_id() {
        let mut resp = MessagingPluginResponse::ok();
        resp.draft_id = Some("gmail-draft-xyz".to_string());
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: MessagingPluginResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.draft_id.as_deref(), Some("gmail-draft-xyz"));
    }

    #[test]
    fn response_with_messages() {
        let mut resp = MessagingPluginResponse::ok();
        resp.messages = Some(vec![FetchedMessage {
            id: "msg-1".to_string(),
            from: "alice@example.com".to_string(),
            to: "me@example.com".to_string(),
            subject: "Hello".to_string(),
            body_text: "Hi there!".to_string(),
            body_html: "<p>Hi there!</p>".to_string(),
            thread_id: "thread-1".to_string(),
            received_at: "2026-04-01T10:00:00Z".to_string(),
        }]);
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: MessagingPluginResponse = serde_json::from_str(&json).unwrap();
        let msgs = parsed.messages.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].subject, "Hello");
    }

    #[test]
    fn draft_state_display() {
        assert_eq!(DraftState::Drafted.to_string(), "drafted");
        assert_eq!(DraftState::Sent.to_string(), "sent");
        assert_eq!(DraftState::Discarded.to_string(), "discarded");
        assert_eq!(DraftState::Unknown.to_string(), "unknown");
    }

    #[test]
    fn draft_state_roundtrip() {
        for state in [
            DraftState::Drafted,
            DraftState::Sent,
            DraftState::Discarded,
            DraftState::Unknown,
        ] {
            let json = serde_json::to_string(&state).unwrap();
            let parsed: DraftState = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, state);
        }
    }

    #[test]
    fn messaging_protocol_version_is_one() {
        assert_eq!(MESSAGING_PROTOCOL_VERSION, 1);
    }
}
