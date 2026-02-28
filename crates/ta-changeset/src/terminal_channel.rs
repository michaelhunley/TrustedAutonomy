// terminal_channel.rs — Terminal-based ReviewChannel adapter.
//
// The default ReviewChannel implementation for v0.4.1.1. Renders interaction
// requests to stdout with formatting, collects responses from stdin.
// Supports mock I/O for testing.

use std::io::{BufRead, BufReader, Read, Write};
use std::sync::Mutex;

use crate::interaction::{
    ChannelCapabilities, Decision, InteractionKind, InteractionRequest, InteractionResponse,
    Notification, NotificationLevel,
};
use crate::review_channel::{ReviewChannel, ReviewChannelError};

/// A ReviewChannel that uses stdin/stdout for human interaction.
///
/// Renders interaction requests as formatted text, prompts for input,
/// and parses responses into InteractionResponse values.
pub struct TerminalChannel {
    reader: Mutex<BufReader<Box<dyn Read + Send>>>,
    writer: Mutex<Box<dyn Write + Send>>,
    channel_id: String,
}

impl TerminalChannel {
    /// Create a TerminalChannel from raw reader/writer.
    /// Use `TerminalChannel::stdio()` for real terminal, or pass mock I/O for tests.
    pub fn new(
        reader: Box<dyn Read + Send>,
        writer: Box<dyn Write + Send>,
        channel_id: impl Into<String>,
    ) -> Self {
        Self {
            reader: Mutex::new(BufReader::new(reader)),
            writer: Mutex::new(writer),
            channel_id: channel_id.into(),
        }
    }

    /// Create a TerminalChannel that reads/writes to real stdin/stdout.
    pub fn stdio() -> Self {
        Self::new(
            Box::new(std::io::stdin()),
            Box::new(std::io::stdout()),
            "terminal:stdio",
        )
    }

    /// Render an interaction request as formatted text.
    fn render_request(&self, request: &InteractionRequest) -> String {
        let mut out = String::new();
        out.push('\n');
        out.push_str(&"=".repeat(60));
        out.push('\n');

        match &request.kind {
            InteractionKind::DraftReview => {
                out.push_str("  DRAFT REVIEW REQUIRED\n");
                out.push_str(&"-".repeat(60));
                out.push('\n');
                if let Some(summary) = request.context.get("summary").and_then(|v| v.as_str()) {
                    out.push_str(&format!("  Summary: {}\n", summary));
                }
                if let Some(count) = request
                    .context
                    .get("artifact_count")
                    .and_then(|v| v.as_u64())
                {
                    out.push_str(&format!("  Artifacts: {}\n", count));
                }
                if let Some(draft_id) = request.context.get("draft_id").and_then(|v| v.as_str()) {
                    out.push_str(&format!("  Draft ID: {}\n", draft_id));
                }
            }
            InteractionKind::PlanNegotiation => {
                out.push_str("  PLAN UPDATE PROPOSED\n");
                out.push_str(&"-".repeat(60));
                out.push('\n');
                if let Some(phase) = request.context.get("phase").and_then(|v| v.as_str()) {
                    out.push_str(&format!("  Phase: {}\n", phase));
                }
                if let Some(status) = request
                    .context
                    .get("proposed_status")
                    .and_then(|v| v.as_str())
                {
                    out.push_str(&format!("  Proposed status: {}\n", status));
                }
            }
            InteractionKind::ApprovalDiscussion => {
                out.push_str("  APPROVAL REQUIRED\n");
                out.push_str(&"-".repeat(60));
                out.push('\n');
                if let Some(msg) = request.context.as_str() {
                    out.push_str(&format!("  {}\n", msg));
                }
            }
            InteractionKind::Escalation => {
                out.push_str("  ESCALATION\n");
                out.push_str(&"-".repeat(60));
                out.push('\n');
                if let Some(reason) = request.context.get("reason").and_then(|v| v.as_str()) {
                    out.push_str(&format!("  Reason: {}\n", reason));
                }
            }
            InteractionKind::Custom(name) => {
                out.push_str(&format!("  INTERACTION: {}\n", name.to_uppercase()));
                out.push_str(&"-".repeat(60));
                out.push('\n');
            }
        }

        out.push_str(&"-".repeat(60));
        out.push('\n');
        out.push_str("  [a]pprove  [r]eject  [d]iscuss  [s]kip\n");
        out.push_str(&"=".repeat(60));
        out.push_str("\n> ");
        out
    }

    /// Parse a user's text response into a Decision.
    fn parse_decision(input: &str) -> Result<Decision, ReviewChannelError> {
        let trimmed = input.trim().to_lowercase();
        match trimmed.as_str() {
            "a" | "approve" | "y" | "yes" => Ok(Decision::Approve),
            "d" | "discuss" => Ok(Decision::Discuss),
            "s" | "skip" => Ok(Decision::SkipForNow),
            _ if trimmed.starts_with("r") || trimmed.starts_with("n") => {
                // "r", "reject", "n", "no" — optionally followed by a reason
                let reason = if trimmed.len() > 1 {
                    // "reject: reason" or "r reason" or "r: reason"
                    let rest = trimmed
                        .trim_start_matches("reject")
                        .trim_start_matches("no")
                        .trim_start_matches('r')
                        .trim_start_matches('n')
                        .trim_start_matches(':')
                        .trim();
                    if rest.is_empty() {
                        "rejected by reviewer".to_string()
                    } else {
                        rest.to_string()
                    }
                } else {
                    "rejected by reviewer".to_string()
                };
                Ok(Decision::Reject { reason })
            }
            "" => Err(ReviewChannelError::InvalidResponse("empty response".into())),
            _ => Err(ReviewChannelError::InvalidResponse(format!(
                "unrecognized input: '{}'",
                trimmed
            ))),
        }
    }

    /// Render a notification as formatted text.
    fn render_notification(notification: &Notification) -> String {
        let prefix = match notification.level {
            NotificationLevel::Debug => "[DEBUG]",
            NotificationLevel::Info => "[INFO]",
            NotificationLevel::Warning => "[WARN]",
            NotificationLevel::Error => "[ERROR]",
        };
        format!("{} {}\n", prefix, notification.message)
    }
}

impl ReviewChannel for TerminalChannel {
    fn request_interaction(
        &self,
        request: &InteractionRequest,
    ) -> Result<InteractionResponse, ReviewChannelError> {
        let rendered = self.render_request(request);

        // Write the rendered request to output.
        {
            let mut writer = self
                .writer
                .lock()
                .map_err(|e| ReviewChannelError::Other(format!("writer lock poisoned: {}", e)))?;
            writer.write_all(rendered.as_bytes())?;
            writer.flush()?;
        }

        // Read the response from input.
        let mut line = String::new();
        {
            let mut reader = self
                .reader
                .lock()
                .map_err(|e| ReviewChannelError::Other(format!("reader lock poisoned: {}", e)))?;
            let bytes = reader.read_line(&mut line)?;
            if bytes == 0 {
                return Err(ReviewChannelError::ChannelClosed);
            }
        }

        let decision = Self::parse_decision(&line)?;

        Ok(InteractionResponse::new(request.interaction_id, decision)
            .with_responder(&self.channel_id))
    }

    fn notify(&self, notification: &Notification) -> Result<(), ReviewChannelError> {
        let rendered = Self::render_notification(notification);
        let mut writer = self
            .writer
            .lock()
            .map_err(|e| ReviewChannelError::Other(format!("writer lock poisoned: {}", e)))?;
        writer.write_all(rendered.as_bytes())?;
        writer.flush()?;
        Ok(())
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_async: false,
            supports_rich_media: false,
            supports_threads: false,
        }
    }

    fn channel_id(&self) -> &str {
        &self.channel_id
    }
}

/// A no-op ReviewChannel that auto-approves all interactions.
/// Useful for non-interactive/batch mode and testing.
pub struct AutoApproveChannel {
    channel_id: String,
}

impl AutoApproveChannel {
    pub fn new() -> Self {
        Self {
            channel_id: "auto-approve".to_string(),
        }
    }
}

impl Default for AutoApproveChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl ReviewChannel for AutoApproveChannel {
    fn request_interaction(
        &self,
        request: &InteractionRequest,
    ) -> Result<InteractionResponse, ReviewChannelError> {
        Ok(
            InteractionResponse::new(request.interaction_id, Decision::Approve)
                .with_responder(&self.channel_id),
        )
    }

    fn notify(&self, _notification: &Notification) -> Result<(), ReviewChannelError> {
        Ok(())
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities::default()
    }

    fn channel_id(&self) -> &str {
        &self.channel_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::Notification;
    use std::io::Cursor;
    use uuid::Uuid;

    fn mock_channel(input: &str) -> (TerminalChannel, std::sync::Arc<Mutex<Vec<u8>>>) {
        let output_buf = std::sync::Arc::new(Mutex::new(Vec::new()));
        let output_writer = output_buf.clone();

        struct SharedWriter(std::sync::Arc<Mutex<Vec<u8>>>);
        impl Write for SharedWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().write(buf)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let reader = Box::new(Cursor::new(input.as_bytes().to_vec()));
        let writer = Box::new(SharedWriter(output_writer));
        let channel = TerminalChannel::new(reader, writer, "test:mock");
        (channel, output_buf)
    }

    #[test]
    fn approve_draft_review() {
        let (channel, _output) = mock_channel("a\n");
        let req = InteractionRequest::draft_review(Uuid::new_v4(), "Test draft", 3);
        let resp = channel.request_interaction(&req).unwrap();
        assert_eq!(resp.decision, Decision::Approve);
        assert_eq!(resp.interaction_id, req.interaction_id);
        assert_eq!(resp.responder_id.as_deref(), Some("test:mock"));
    }

    #[test]
    fn reject_with_reason() {
        let (channel, _output) = mock_channel("reject: needs more tests\n");
        let req = InteractionRequest::draft_review(Uuid::new_v4(), "Draft", 1);
        let resp = channel.request_interaction(&req).unwrap();
        assert_eq!(
            resp.decision,
            Decision::Reject {
                reason: "needs more tests".into()
            }
        );
    }

    #[test]
    fn reject_shorthand() {
        let (channel, _output) = mock_channel("r\n");
        let req = InteractionRequest::draft_review(Uuid::new_v4(), "Draft", 1);
        let resp = channel.request_interaction(&req).unwrap();
        assert!(matches!(resp.decision, Decision::Reject { .. }));
    }

    #[test]
    fn discuss_response() {
        let (channel, _output) = mock_channel("d\n");
        let req = InteractionRequest::draft_review(Uuid::new_v4(), "Draft", 1);
        let resp = channel.request_interaction(&req).unwrap();
        assert_eq!(resp.decision, Decision::Discuss);
    }

    #[test]
    fn skip_response() {
        let (channel, _output) = mock_channel("s\n");
        let req = InteractionRequest::draft_review(Uuid::new_v4(), "Draft", 1);
        let resp = channel.request_interaction(&req).unwrap();
        assert_eq!(resp.decision, Decision::SkipForNow);
    }

    #[test]
    fn yes_is_approve() {
        let (channel, _output) = mock_channel("yes\n");
        let req = InteractionRequest::draft_review(Uuid::new_v4(), "Draft", 1);
        let resp = channel.request_interaction(&req).unwrap();
        assert_eq!(resp.decision, Decision::Approve);
    }

    #[test]
    fn empty_input_is_error() {
        let (channel, _output) = mock_channel("\n");
        let req = InteractionRequest::draft_review(Uuid::new_v4(), "Draft", 1);
        let result = channel.request_interaction(&req);
        assert!(matches!(
            result,
            Err(ReviewChannelError::InvalidResponse(_))
        ));
    }

    #[test]
    fn eof_is_channel_closed() {
        let (channel, _output) = mock_channel("");
        let req = InteractionRequest::draft_review(Uuid::new_v4(), "Draft", 1);
        let result = channel.request_interaction(&req);
        assert!(matches!(result, Err(ReviewChannelError::ChannelClosed)));
    }

    #[test]
    fn renders_draft_review_output() {
        let (channel, output) = mock_channel("a\n");
        let req = InteractionRequest::draft_review(Uuid::new_v4(), "Add auth module", 5);
        channel.request_interaction(&req).unwrap();

        let rendered = String::from_utf8(output.lock().unwrap().clone()).unwrap();
        assert!(rendered.contains("DRAFT REVIEW REQUIRED"));
        assert!(rendered.contains("Add auth module"));
        assert!(rendered.contains("Artifacts: 5"));
        assert!(rendered.contains("[a]pprove"));
    }

    #[test]
    fn renders_plan_negotiation() {
        let (channel, output) = mock_channel("a\n");
        let req = InteractionRequest::plan_negotiation("v0.4.2", "done");
        channel.request_interaction(&req).unwrap();

        let rendered = String::from_utf8(output.lock().unwrap().clone()).unwrap();
        assert!(rendered.contains("PLAN UPDATE PROPOSED"));
        assert!(rendered.contains("v0.4.2"));
    }

    #[test]
    fn notify_renders_to_output() {
        let (channel, output) = mock_channel("");
        let notif = Notification::info("Sub-goal 2 of 5 complete");
        channel.notify(&notif).unwrap();

        let rendered = String::from_utf8(output.lock().unwrap().clone()).unwrap();
        assert!(rendered.contains("[INFO]"));
        assert!(rendered.contains("Sub-goal 2 of 5 complete"));
    }

    #[test]
    fn notify_warning_prefix() {
        let (channel, output) = mock_channel("");
        let notif = Notification::warning("Agent approaching token limit");
        channel.notify(&notif).unwrap();

        let rendered = String::from_utf8(output.lock().unwrap().clone()).unwrap();
        assert!(rendered.contains("[WARN]"));
    }

    #[test]
    fn channel_capabilities() {
        let (channel, _) = mock_channel("");
        let caps = channel.capabilities();
        assert!(!caps.supports_async);
        assert!(!caps.supports_rich_media);
        assert!(!caps.supports_threads);
    }

    #[test]
    fn channel_id_returns_configured_id() {
        let (channel, _) = mock_channel("");
        assert_eq!(channel.channel_id(), "test:mock");
    }

    #[test]
    fn auto_approve_channel_approves_all() {
        let channel = AutoApproveChannel::new();
        let req = InteractionRequest::draft_review(Uuid::new_v4(), "Any draft", 10);
        let resp = channel.request_interaction(&req).unwrap();
        assert_eq!(resp.decision, Decision::Approve);
        assert_eq!(resp.responder_id.as_deref(), Some("auto-approve"));
    }

    #[test]
    fn auto_approve_channel_notify_is_noop() {
        let channel = AutoApproveChannel::new();
        let notif = Notification::info("test");
        assert!(channel.notify(&notif).is_ok());
    }

    #[test]
    fn parse_decision_variants() {
        assert_eq!(
            TerminalChannel::parse_decision("approve").unwrap(),
            Decision::Approve
        );
        assert_eq!(
            TerminalChannel::parse_decision("y").unwrap(),
            Decision::Approve
        );
        assert_eq!(
            TerminalChannel::parse_decision("discuss").unwrap(),
            Decision::Discuss
        );
        assert_eq!(
            TerminalChannel::parse_decision("skip").unwrap(),
            Decision::SkipForNow
        );
        assert!(TerminalChannel::parse_decision("unknown").is_err());
    }
}
