//! ta-messaging-imap — IMAP messaging adapter plugin for Trusted Autonomy.
//!
//! Implements the TA messaging plugin protocol (JSON-over-stdio, version 1).
//! Reads one JSON request line from stdin, writes one JSON response line to
//! stdout, then exits.
//!
//! ## Operations
//!
//! - `fetch`         — IMAP SEARCH SINCE + FETCH for new messages
//! - `create_draft`  — IMAP APPEND to Drafts mailbox
//! - `draft_status`  — Check if UID still in Drafts (best-effort)
//! - `health`        — IMAP NOOP + return connected address
//! - `capabilities`  — Return the op list above
//!
//! ## Credentials
//!
//! IMAP config (host, port, username, password, drafts_folder) is read from a
//! JSON blob stored in the OS keychain under `ta-messaging:imap:<address>`.
//! The plugin reads it via the environment variable
//! `TA_SECRET_TA_MESSAGING_IMAP_<ADDRESS>` or from the fallback file store.
//!
//! Set up credentials with: `ta adapter setup messaging/imap`
//!
//! ## Safety
//!
//! There is no `send` operation. TA only APPENDs to the Drafts mailbox;
//! the user sends from their email client.

use std::io::{BufRead, Write};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Protocol types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Fetch {
        since: String,
        account: Option<String>,
        limit: Option<u32>,
    },
    CreateDraft {
        draft: DraftEnvelope,
    },
    DraftStatus {
        draft_id: String,
    },
    Health {},
    Capabilities {},
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct DraftEnvelope {
    to: String,
    subject: String,
    body_html: String,
    in_reply_to: Option<String>,
    thread_id: Option<String>,
    body_text: Option<String>,
}

#[derive(Debug, Serialize)]
struct Response {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    messages: Option<Vec<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    draft_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capabilities: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct Message {
    id: String,
    from: String,
    to: String,
    subject: String,
    body_text: String,
    body_html: String,
    thread_id: String,
    received_at: String,
}

impl Response {
    fn ok_empty() -> Self {
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

    fn err(msg: impl Into<String>) -> Self {
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
// IMAP config (from keychain)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ImapConfig {
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    username: String,
    password: String,
    #[serde(default = "default_drafts_folder")]
    drafts_folder: String,
    #[serde(default = "default_tls")]
    tls: bool,
}

fn default_port() -> u16 { 993 }
fn default_drafts_folder() -> String { "Drafts".to_string() }
fn default_tls() -> bool { true }

fn load_imap_config(account: Option<&str>) -> Result<ImapConfig, String> {
    let key_suffix = account
        .map(|a| {
            a.chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }
                })
                .collect::<String>()
        })
        .unwrap_or_else(|| "default".to_string());

    let env_key = format!(
        "TA_SECRET_TA_MESSAGING_IMAP_{}",
        key_suffix.to_uppercase()
    );

    let json_str = if let Ok(val) = std::env::var(&env_key) {
        val
    } else {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| "Cannot determine home directory".to_string())?;

        let path = std::path::PathBuf::from(home)
            .join(".config")
            .join("ta")
            .join("secrets")
            .join(format!("ta-messaging_imap_{}", key_suffix.to_lowercase()));

        if !path.exists() {
            return Err(format!(
                "No IMAP config found for key '{}'. Run: ta adapter setup messaging/imap",
                env_key
            ));
        }

        std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read IMAP config: {}", e))?
    };

    serde_json::from_str(&json_str)
        .map_err(|e| format!("Invalid IMAP config JSON: {}", e))
}

// ---------------------------------------------------------------------------
// IMAP operations
// ---------------------------------------------------------------------------

fn connect_imap(
    cfg: &ImapConfig,
) -> Result<imap::Session<Box<dyn imap::extensions::idle::SetReadTimeout + Send>>, String> {
    if cfg.tls {
        let tls = rustls_connector::RustlsConnector::new_with_native_certs()
            .map_err(|e| format!("TLS init failed: {}", e))?;

        let client = imap::connect(
            (cfg.host.as_str(), cfg.port),
            &cfg.host,
            &tls,
        )
        .map_err(|e| format!("IMAP connect failed: {}", e))?;

        let session = client
            .login(&cfg.username, &cfg.password)
            .map_err(|(e, _)| format!("IMAP login failed: {}", e))?;

        // Box the session as a dyn trait to unify the type.
        Ok(Box::new(session) as Box<dyn imap::extensions::idle::SetReadTimeout + Send>)
    } else {
        // STARTTLS path.
        let client = imap::connect_starttls(
            (cfg.host.as_str(), cfg.port),
            &cfg.host,
            &rustls_connector::RustlsConnector::new_with_native_certs()
                .map_err(|e| format!("TLS init failed: {}", e))?,
        )
        .map_err(|e| format!("IMAP STARTTLS connect failed: {}", e))?;

        let session = client
            .login(&cfg.username, &cfg.password)
            .map_err(|(e, _)| format!("IMAP login failed: {}", e))?;

        Ok(Box::new(session) as Box<dyn imap::extensions::idle::SetReadTimeout + Send>)
    }
}

/// Fetch messages from INBOX since the given date.
fn imap_fetch(cfg: &ImapConfig, since_rfc3339: &str, limit: u32) -> Result<Vec<Message>, String> {
    use imap::types::Seq;

    // Connect and select INBOX.
    let tls = rustls_connector::RustlsConnector::new_with_native_certs()
        .map_err(|e| format!("TLS init failed: {}", e))?;

    let client = imap::connect((cfg.host.as_str(), cfg.port), &cfg.host, &tls)
        .map_err(|e| format!("IMAP connect failed for fetch: {}", e))?;

    let mut session = client
        .login(&cfg.username, &cfg.password)
        .map_err(|(e, _)| format!("IMAP login failed: {}", e))?;

    session
        .select("INBOX")
        .map_err(|e| format!("IMAP SELECT INBOX failed: {}", e))?;

    // Build SEARCH SINCE <date> criterion. IMAP SINCE takes DD-Mon-YYYY.
    let since_imap = rfc3339_to_imap_date(since_rfc3339).unwrap_or_else(|| "1-Jan-1970".to_string());
    let uids = session
        .search(format!("SINCE \"{}\"", since_imap))
        .map_err(|e| format!("IMAP SEARCH failed: {}", e))?;

    let uids: Vec<Seq> = uids.into_iter().rev().take(limit as usize).collect();

    if uids.is_empty() {
        let _ = session.logout();
        return Ok(vec![]);
    }

    let uid_set = uids
        .iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let messages = session
        .fetch(&uid_set, "RFC822 INTERNALDATE UID")
        .map_err(|e| format!("IMAP FETCH failed: {}", e))?;

    let mut result = Vec::new();
    for msg in messages.iter() {
        if let Some(body) = msg.body() {
            let raw = String::from_utf8_lossy(body);
            let parsed = parse_rfc822(&raw, msg.uid.unwrap_or(0));
            result.push(parsed);
        }
    }

    let _ = session.logout();
    Ok(result)
}

/// Append a draft to the Drafts mailbox via IMAP APPEND.
fn imap_append_draft(cfg: &ImapConfig, draft: &DraftEnvelope) -> Result<String, String> {
    let tls = rustls_connector::RustlsConnector::new_with_native_certs()
        .map_err(|e| format!("TLS init failed: {}", e))?;

    let client = imap::connect((cfg.host.as_str(), cfg.port), &cfg.host, &tls)
        .map_err(|e| format!("IMAP connect failed for append: {}", e))?;

    let mut session = client
        .login(&cfg.username, &cfg.password)
        .map_err(|(e, _)| format!("IMAP login failed: {}", e))?;

    let body_text = draft
        .body_text
        .clone()
        .unwrap_or_else(|| strip_html(&draft.body_html));

    // Build RFC 2822 message.
    let mut rfc822 = String::new();
    rfc822.push_str(&format!("To: {}\r\n", draft.to));
    rfc822.push_str(&format!("Subject: {}\r\n", draft.subject));
    if let Some(ref in_reply_to) = draft.in_reply_to {
        rfc822.push_str(&format!("In-Reply-To: {}\r\n", in_reply_to));
    }
    rfc822.push_str("MIME-Version: 1.0\r\n");
    rfc822.push_str("Content-Type: multipart/alternative; boundary=\"ta_imap_boundary\"\r\n");
    rfc822.push_str("\r\n");
    rfc822.push_str("--ta_imap_boundary\r\n");
    rfc822.push_str("Content-Type: text/plain; charset=UTF-8\r\n\r\n");
    rfc822.push_str(&body_text);
    rfc822.push_str("\r\n--ta_imap_boundary\r\n");
    rfc822.push_str("Content-Type: text/html; charset=UTF-8\r\n\r\n");
    rfc822.push_str(&draft.body_html);
    rfc822.push_str("\r\n--ta_imap_boundary--\r\n");

    // APPEND to Drafts mailbox with \Draft flag.
    session
        .append_with_flags(
            &cfg.drafts_folder,
            &imap::types::Flag::Draft,
            None,
            rfc822.as_bytes(),
        )
        .map_err(|e| format!("IMAP APPEND to Drafts failed: {}", e))?;

    // The UID of the appended message is returned in the APPENDUID response,
    // but not all servers support it. We use a timestamp-based ID as a fallback.
    let draft_id = format!(
        "imap-draft-{}-{}",
        cfg.username.replace('@', "_at_").replace('.', "_"),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );

    let _ = session.logout();
    Ok(draft_id)
}

/// Best-effort draft status check: check if a UID still exists in the Drafts folder.
fn imap_draft_status(cfg: &ImapConfig, draft_id: &str) -> Result<String, String> {
    // For timestamp-based IDs we can't do a precise UID lookup.
    // Return "drafted" unless the draft_id contains metadata we can use.
    // This is documented as best-effort in the protocol.

    // If the ID contains the username, attempt a rough check.
    let username_part = cfg
        .username
        .replace('@', "_at_")
        .replace('.', "_");

    if !draft_id.contains(&username_part) {
        // Different account or legacy ID — can't check.
        return Ok("unknown".to_string());
    }

    let tls = rustls_connector::RustlsConnector::new_with_native_certs()
        .map_err(|e| format!("TLS init failed: {}", e))?;

    let client = imap::connect((cfg.host.as_str(), cfg.port), &cfg.host, &tls)
        .map_err(|e| format!("IMAP connect failed for draft status: {}", e))?;

    let mut session = client
        .login(&cfg.username, &cfg.password)
        .map_err(|(e, _)| format!("IMAP login failed: {}", e))?;

    // SELECT the Drafts mailbox and check whether it has messages.
    match session.select(&cfg.drafts_folder) {
        Ok(mbox) => {
            let _ = session.logout();
            if mbox.exists > 0 {
                Ok("drafted".to_string())
            } else {
                Ok("unknown".to_string())
            }
        }
        Err(_) => {
            let _ = session.logout();
            Ok("unknown".to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_rfc822(raw: &str, uid: u32) -> Message {
    let mut from = String::new();
    let mut to = String::new();
    let mut subject = String::new();
    let mut date = String::new();
    let mut message_id = String::new();

    // Parse headers (very minimal — sufficient for the TA protocol).
    for line in raw.lines() {
        if line.is_empty() {
            break; // End of headers.
        }
        let lower = line.to_lowercase();
        if lower.starts_with("from:") {
            from = line[5..].trim().to_string();
        } else if lower.starts_with("to:") {
            to = line[3..].trim().to_string();
        } else if lower.starts_with("subject:") {
            subject = line[8..].trim().to_string();
        } else if lower.starts_with("date:") {
            date = line[5..].trim().to_string();
        } else if lower.starts_with("message-id:") {
            message_id = line[11..].trim().to_string();
        }
    }

    // Extract body (everything after the blank line).
    let body = raw
        .find("\r\n\r\n")
        .or_else(|| raw.find("\n\n"))
        .map(|pos| &raw[pos..])
        .unwrap_or("")
        .trim()
        .to_string();

    let id = if !message_id.is_empty() {
        message_id
    } else {
        format!("imap-uid-{}", uid)
    };

    Message {
        id,
        from,
        to,
        subject,
        body_text: body.clone(),
        body_html: String::new(),
        thread_id: String::new(),
        received_at: date,
    }
}

fn strip_html(html: &str) -> String {
    let mut in_tag = false;
    let mut out = String::new();
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

/// Convert an RFC3339 timestamp to an IMAP SINCE date string ("DD-Mon-YYYY").
fn rfc3339_to_imap_date(s: &str) -> Option<String> {
    if s.is_empty() || s == "1970-01-01T00:00:00Z" {
        return Some("1-Jan-1970".to_string());
    }

    let parts: Vec<&str> = s.split('T').collect();
    let date_parts: Vec<&str> = parts.first()?.split('-').collect();
    if date_parts.len() < 3 {
        return None;
    }

    let year = date_parts[0];
    let month: u32 = date_parts[1].parse().ok()?;
    let day: u32 = date_parts[2].parse().ok()?;

    let month_name = [
        "", "Jan", "Feb", "Mar", "Apr", "May", "Jun",
        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .get(month as usize)?;

    Some(format!("{}-{}-{}", day, month_name, year))
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let stdin = std::io::stdin();
    let mut line = String::new();
    if let Err(e) = stdin.lock().read_line(&mut line) {
        respond(Response::err(format!("Failed to read stdin: {}", e)));
        return;
    }

    let request: Request = match serde_json::from_str(line.trim()) {
        Ok(r) => r,
        Err(e) => {
            respond(Response::err(format!(
                "Invalid request JSON: {}. Got: '{}'",
                e,
                line.trim()
            )));
            return;
        }
    };

    match request {
        Request::Capabilities {} => {
            let mut resp = Response::ok_empty();
            resp.capabilities = Some(vec![
                "fetch".to_string(),
                "create_draft".to_string(),
                "draft_status".to_string(),
                "health".to_string(),
                "capabilities".to_string(),
            ]);
            respond(resp);
        }

        Request::Health { .. } => {
            let cfg = match load_imap_config(None) {
                Ok(c) => c,
                Err(e) => {
                    respond(Response::err(e));
                    return;
                }
            };

            // Connect and run NOOP as health check.
            let tls = match rustls_connector::RustlsConnector::new_with_native_certs() {
                Ok(t) => t,
                Err(e) => {
                    respond(Response::err(format!("TLS init failed: {}", e)));
                    return;
                }
            };

            match imap::connect((cfg.host.as_str(), cfg.port), &cfg.host, &tls) {
                Ok(client) => {
                    match client.login(&cfg.username, &cfg.password) {
                        Ok(mut session) => {
                            let _ = session.noop();
                            let _ = session.logout();
                            let mut resp = Response::ok_empty();
                            resp.address = Some(cfg.username.clone());
                            resp.provider = Some("imap".to_string());
                            respond(resp);
                        }
                        Err((e, _)) => {
                            respond(Response::err(format!("IMAP login failed: {}", e)));
                        }
                    }
                }
                Err(e) => {
                    respond(Response::err(format!(
                        "IMAP connect to {}:{} failed: {}",
                        cfg.host, cfg.port, e
                    )));
                }
            }
        }

        Request::Fetch { since, account, limit } => {
            let cfg = match load_imap_config(account.as_deref()) {
                Ok(c) => c,
                Err(e) => {
                    respond(Response::err(e));
                    return;
                }
            };
            match imap_fetch(&cfg, &since, limit.unwrap_or(50)) {
                Ok(msgs) => {
                    let mut resp = Response::ok_empty();
                    resp.messages = Some(msgs);
                    respond(resp);
                }
                Err(e) => respond(Response::err(format!("IMAP fetch failed: {}", e))),
            }
        }

        Request::CreateDraft { draft } => {
            let cfg = match load_imap_config(None) {
                Ok(c) => c,
                Err(e) => {
                    respond(Response::err(e));
                    return;
                }
            };
            match imap_append_draft(&cfg, &draft) {
                Ok(draft_id) => {
                    let mut resp = Response::ok_empty();
                    resp.draft_id = Some(draft_id);
                    respond(resp);
                }
                Err(e) => respond(Response::err(format!("IMAP create_draft failed: {}", e))),
            }
        }

        Request::DraftStatus { draft_id } => {
            let cfg = match load_imap_config(None) {
                Ok(c) => c,
                Err(e) => {
                    respond(Response::err(e));
                    return;
                }
            };
            match imap_draft_status(&cfg, &draft_id) {
                Ok(state) => {
                    let mut resp = Response::ok_empty();
                    resp.state = Some(state);
                    respond(resp);
                }
                Err(e) => respond(Response::err(format!("IMAP draft_status failed: {}", e))),
            }
        }
    }
}

fn respond(resp: Response) {
    let json = serde_json::to_string(&resp).unwrap_or_else(|e| {
        format!(r#"{{"ok":false,"error":"Failed to serialize response: {}"}}"#, e)
    });
    println!("{}", json);
    let _ = std::io::stdout().flush();
}
