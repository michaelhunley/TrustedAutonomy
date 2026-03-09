//! Email channel plugin for Trusted Autonomy.
//!
//! External JSON-over-stdio plugin that sends agent questions as formatted
//! emails via SMTP and optionally polls IMAP for reply-based responses.
//!
//! ## Protocol
//!
//! - Reads one JSON line from stdin: a `ChannelQuestion` object
//! - Sends the question as an HTML+text email via SMTP
//! - Writes one JSON line to stdout: a `DeliveryResult` object
//! - If IMAP is configured, spawns a background poller that watches for reply
//!   emails and POSTs parsed responses to the daemon's respond endpoint
//!
//! ## Environment Variables
//!
//! - `TA_EMAIL_SMTP_HOST` (required): SMTP server hostname (e.g., `smtp.gmail.com`)
//! - `TA_EMAIL_SMTP_PORT` (optional): SMTP port (default: 587 for STARTTLS)
//! - `TA_EMAIL_USER` (required): SMTP username / sender email address
//! - `TA_EMAIL_PASSWORD` (required): SMTP password or app password
//! - `TA_EMAIL_REVIEWER` (required): Comma-separated reviewer email addresses (first to reply wins)
//! - `TA_EMAIL_FROM_NAME` (optional): Display name for sender (default: "TA Agent")
//! - `TA_EMAIL_SUBJECT_PREFIX` (optional): Subject line prefix (default: "[TA Review]")
//!
//! ## Installation
//!
//! 1. Build: `cargo build --release` (or `ta plugin build email`)
//! 2. Plugin is auto-installed to `.ta/plugins/channels/email/`
//! 3. Set the required environment variables
//! 4. For Gmail: use an App Password (no OAuth needed)

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};

mod email_body;
pub mod reply_parser;

/// Input: question from TA daemon via stdin.
#[derive(Debug, Deserialize)]
pub struct ChannelQuestion {
    pub interaction_id: String,
    #[allow(dead_code)]
    pub goal_id: String,
    pub question: String,
    pub context: Option<String>,
    pub response_hint: String,
    #[serde(default)]
    pub choices: Vec<String>,
    #[serde(default = "default_turn")]
    pub turn: u32,
    pub callback_url: String,
}

fn default_turn() -> u32 {
    1
}

/// Output: delivery result written to stdout.
#[derive(Debug, Serialize)]
struct DeliveryResult {
    channel: String,
    delivery_id: String,
    success: bool,
    error: Option<String>,
}

impl DeliveryResult {
    fn success(delivery_id: String) -> Self {
        Self {
            channel: "email".into(),
            delivery_id,
            success: true,
            error: None,
        }
    }

    fn error(msg: String) -> Self {
        Self {
            channel: "email".into(),
            delivery_id: String::new(),
            success: false,
            error: Some(msg),
        }
    }
}

fn write_result(result: &DeliveryResult) {
    let json = serde_json::to_string(result).unwrap_or_else(|e| {
        format!(
            r#"{{"channel":"email","delivery_id":"","success":false,"error":"serialization error: {}"}}"#,
            e
        )
    });
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(out, "{}", json);
    let _ = out.flush();
}

/// Read a required environment variable with an actionable error message.
fn require_env(name: &str, help: &str) -> Result<String, String> {
    match std::env::var(name) {
        Ok(val) if !val.is_empty() => Ok(val),
        _ => Err(format!("Environment variable '{}' not set. {}", name, help)),
    }
}

/// SMTP configuration parsed from environment variables.
struct SmtpConfig {
    host: String,
    port: u16,
    user: String,
    password: String,
    from_name: String,
    subject_prefix: String,
    reviewers: Vec<String>,
}

impl SmtpConfig {
    fn from_env() -> Result<Self, String> {
        let host = require_env(
            "TA_EMAIL_SMTP_HOST",
            "Set it to your SMTP server (e.g., smtp.gmail.com).",
        )?;
        let port = std::env::var("TA_EMAIL_SMTP_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(587);
        let user = require_env(
            "TA_EMAIL_USER",
            "Set it to your email address / SMTP username.",
        )?;
        let password = require_env(
            "TA_EMAIL_PASSWORD",
            "Set it to your SMTP password or app password. \
             For Gmail, generate an App Password at https://myaccount.google.com/apppasswords",
        )?;
        let reviewer_str = require_env(
            "TA_EMAIL_REVIEWER",
            "Set it to a comma-separated list of reviewer email addresses.",
        )?;
        let reviewers: Vec<String> = reviewer_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if reviewers.is_empty() {
            return Err(
                "TA_EMAIL_REVIEWER is set but contains no valid email addresses. \
                 Provide at least one reviewer (e.g., reviewer@company.com)."
                    .into(),
            );
        }
        let from_name = std::env::var("TA_EMAIL_FROM_NAME").unwrap_or_else(|_| "TA Agent".into());
        let subject_prefix =
            std::env::var("TA_EMAIL_SUBJECT_PREFIX").unwrap_or_else(|_| "[TA Review]".into());

        Ok(Self {
            host,
            port,
            user,
            password,
            from_name,
            subject_prefix,
            reviewers,
        })
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Read config from environment.
    let config = match SmtpConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            write_result(&DeliveryResult::error(e));
            std::process::exit(1);
        }
    };

    // Read question from stdin.
    let stdin = io::stdin();
    let line = match stdin.lock().lines().next() {
        Some(Ok(line)) if !line.trim().is_empty() => line,
        _ => {
            write_result(&DeliveryResult::error(
                "No input received on stdin. Expected a JSON-encoded ChannelQuestion.".into(),
            ));
            std::process::exit(1);
        }
    };

    let question: ChannelQuestion = match serde_json::from_str(&line) {
        Ok(q) => q,
        Err(e) => {
            write_result(&DeliveryResult::error(format!(
                "Invalid JSON input: {}. Got: '{}'",
                e,
                if line.len() > 200 {
                    &line[..200]
                } else {
                    &line
                }
            )));
            std::process::exit(1);
        }
    };

    // Build email content.
    let subject = email_body::build_subject(&question, &config.subject_prefix);
    let body_text = email_body::build_body_text(&question);
    let body_html = email_body::build_body_html(&question);

    // Build and send email via SMTP using lettre.
    let from_mailbox = match format!("{} <{}>", config.from_name, config.user).parse() {
        Ok(m) => m,
        Err(e) => {
            write_result(&DeliveryResult::error(format!(
                "Invalid sender address '{}': {}. Check TA_EMAIL_USER.",
                config.user, e
            )));
            std::process::exit(1);
        }
    };

    // Build the message with all reviewers in To.
    let to_addresses: Vec<lettre::message::Mailbox> = config
        .reviewers
        .iter()
        .filter_map(|addr| addr.parse().ok())
        .collect();

    if to_addresses.is_empty() {
        write_result(&DeliveryResult::error(format!(
            "No valid email addresses in TA_EMAIL_REVIEWER: '{}'. \
             Each address must be a valid email (e.g., user@example.com).",
            config.reviewers.join(", ")
        )));
        std::process::exit(1);
    }

    let mut message_builder = lettre::Message::builder()
        .from(from_mailbox)
        .subject(&subject)
        .header(lettre::message::header::ContentType::parse("multipart/alternative").unwrap())
        // Custom headers for threading and tracking.
        .header(RawHeader::new(
            "X-TA-Interaction-ID",
            &question.interaction_id,
        ))
        .header(RawHeader::new("X-TA-Goal-ID", &question.goal_id))
        .header(RawHeader::new("X-TA-Request-ID", &question.interaction_id));

    for to_addr in &to_addresses {
        message_builder = message_builder.to(to_addr.clone());
    }

    // Use Message-ID based on interaction_id for threading.
    let message_id = format!(
        "<ta-{}-{}@ta.local>",
        question.interaction_id, question.turn
    );
    message_builder = message_builder.message_id(Some(message_id.clone()));

    // If this is a follow-up turn, reference the original message for threading.
    if question.turn > 1 {
        let original_id = format!("<ta-{}-1@ta.local>", question.interaction_id);
        message_builder = message_builder.header(RawHeader::new("In-Reply-To", &original_id));
        message_builder = message_builder.header(RawHeader::new("References", &original_id));
    }

    let email = match message_builder.multipart(
        lettre::message::MultiPart::alternative()
            .singlepart(
                lettre::message::SinglePart::builder()
                    .header(lettre::message::header::ContentType::TEXT_PLAIN)
                    .body(body_text),
            )
            .singlepart(
                lettre::message::SinglePart::builder()
                    .header(lettre::message::header::ContentType::TEXT_HTML)
                    .body(body_html),
            ),
    ) {
        Ok(m) => m,
        Err(e) => {
            write_result(&DeliveryResult::error(format!(
                "Failed to build email message: {}",
                e
            )));
            std::process::exit(1);
        }
    };

    // Connect to SMTP and send.
    use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

    let transport = match AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host) {
        Ok(builder) => builder
            .port(config.port)
            .credentials(lettre::transport::smtp::authentication::Credentials::new(
                config.user.clone(),
                config.password.clone(),
            ))
            .build(),
        Err(e) => {
            write_result(&DeliveryResult::error(format!(
                "Failed to configure SMTP transport for '{}:{}': {}. \
                 Check TA_EMAIL_SMTP_HOST and TA_EMAIL_SMTP_PORT.",
                config.host, config.port, e
            )));
            std::process::exit(1);
        }
    };

    match transport.send(email).await {
        Ok(response) => {
            let delivery_id = message_id.clone();
            eprintln!(
                "[email] Delivered question {} to {} (message-id: {}, smtp: {})",
                question.interaction_id,
                config.reviewers.join(", "),
                delivery_id,
                response.code()
            );
            write_result(&DeliveryResult::success(delivery_id));
        }
        Err(e) => {
            write_result(&DeliveryResult::error(format!(
                "SMTP send failed for question {} to {}: {}. \
                 Check SMTP credentials (TA_EMAIL_USER, TA_EMAIL_PASSWORD) \
                 and server settings (TA_EMAIL_SMTP_HOST:{}). \
                 For Gmail, ensure you're using an App Password.",
                question.interaction_id,
                config.reviewers.join(", "),
                e,
                config.port
            )));
            std::process::exit(1);
        }
    }
}

/// A raw email header for custom X-TA-* headers.
///
/// lettre doesn't have built-in support for arbitrary headers, so we implement
/// the Header trait manually for our custom headers.
#[derive(Clone)]
struct RawHeader {
    name: lettre::message::header::HeaderName,
    value: String,
}

impl RawHeader {
    fn new(name: &str, value: &str) -> Self {
        Self {
            name: lettre::message::header::HeaderName::new_from_ascii(name.into())
                .expect("valid header name"),
            value: value.to_string(),
        }
    }
}

impl lettre::message::header::Header for RawHeader {
    fn name() -> lettre::message::header::HeaderName {
        // This is a placeholder — the actual name is instance-specific.
        // lettre uses the instance method get_name() at runtime.
        lettre::message::header::HeaderName::new_from_ascii("X-TA-Custom".into())
            .expect("valid header name")
    }

    fn parse(_: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Err("RawHeader does not support parsing".into())
    }

    fn display(&self) -> lettre::message::header::HeaderValue {
        lettre::message::header::HeaderValue::new(self.name.clone(), self.value.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_channel_question() {
        let json = r#"{
            "interaction_id": "550e8400-e29b-41d4-a716-446655440000",
            "goal_id": "660e8400-e29b-41d4-a716-446655440000",
            "question": "Which database?",
            "context": "Setting up backend",
            "response_hint": "choice",
            "choices": ["PostgreSQL", "SQLite"],
            "turn": 1,
            "callback_url": "http://localhost:7700"
        }"#;
        let q: ChannelQuestion = serde_json::from_str(json).unwrap();
        assert_eq!(q.question, "Which database?");
        assert_eq!(q.choices.len(), 2);
        assert_eq!(q.turn, 1);
    }

    #[test]
    fn deserialize_minimal_question() {
        let json = r#"{
            "interaction_id": "550e8400-e29b-41d4-a716-446655440000",
            "goal_id": "660e8400-e29b-41d4-a716-446655440000",
            "question": "Continue?",
            "response_hint": "yes_no",
            "callback_url": "http://localhost:7700"
        }"#;
        let q: ChannelQuestion = serde_json::from_str(json).unwrap();
        assert_eq!(q.question, "Continue?");
        assert!(q.context.is_none());
        assert!(q.choices.is_empty());
        assert_eq!(q.turn, 1);
    }

    #[test]
    fn serialize_delivery_result() {
        let result = DeliveryResult::success("<ta-123-1@ta.local>".into());
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"channel\":\"email\""));
        assert!(json.contains("ta-123-1@ta.local"));
    }

    #[test]
    fn serialize_error_result() {
        let result = DeliveryResult::error("SMTP auth failed".into());
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("SMTP auth failed"));
    }

    #[test]
    fn require_env_missing() {
        // Use a unique env var name that won't be set.
        let result = require_env("TA_TEST_NONEXISTENT_VAR_XYZ", "Help text here.");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("TA_TEST_NONEXISTENT_VAR_XYZ"));
        assert!(err.contains("Help text here"));
    }
}
