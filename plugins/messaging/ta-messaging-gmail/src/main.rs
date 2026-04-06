//! ta-messaging-gmail — Gmail messaging adapter plugin for Trusted Autonomy.
//!
//! Implements the TA messaging plugin protocol (JSON-over-stdio, version 1).
//! Reads one JSON request line from stdin, writes one JSON response line to
//! stdout, then exits. Spawned fresh per call by ExternalMessagingAdapter.
//!
//! ## Operations
//!
//! - `fetch`         — List messages since a watermark via Gmail REST API
//! - `create_draft`  — Create a draft in Gmail Drafts via `drafts.create`
//! - `draft_status`  — Poll a draft via `drafts.get`
//! - `health`        — Check connectivity and return connected address
//! - `capabilities`  — Return the op list above
//!
//! ## Credentials
//!
//! OAuth2 access token is read from the environment variable
//! `TA_SECRET_TA_MESSAGING_GMAIL_<ADDRESS>` (set by `ta adapter credentials get`).
//! Alternatively, the plugin falls back to reading the access token from
//! `~/.config/ta/secrets/ta-messaging_gmail_<sanitized-address>`.
//!
//! Set up credentials with: `ta adapter setup messaging/gmail`
//!
//! ## Safety
//!
//! There is no `send` operation. TA only creates drafts; the user sends
//! from their Gmail client. This is enforced at the type level.

use std::io::{BufRead, Write};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Protocol types (inline to avoid workspace dependency)
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
// Gmail API client
// ---------------------------------------------------------------------------

struct GmailClient {
    access_token: String,
    client: reqwest::blocking::Client,
}

impl GmailClient {
    fn new(access_token: String) -> Self {
        Self {
            access_token,
            client: reqwest::blocking::Client::new(),
        }
    }

    fn get_profile(&self) -> Result<serde_json::Value, String> {
        let resp = self
            .client
            .get("https://gmail.googleapis.com/gmail/v1/users/me/profile")
            .bearer_auth(&self.access_token)
            .send()
            .map_err(|e| format!("Gmail API request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(format!("Gmail API error {}: {}", status, body));
        }

        resp.json::<serde_json::Value>()
            .map_err(|e| format!("Failed to parse Gmail profile response: {}", e))
    }

    fn list_messages(&self, since_rfc3339: &str, limit: u32) -> Result<Vec<Message>, String> {
        // Convert ISO-8601 to Gmail query format: after:<epoch_seconds>
        let since_epoch = rfc3339_to_epoch(since_rfc3339).unwrap_or(0);
        let query = format!("after:{}", since_epoch);

        let url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages?q={}&maxResults={}",
            urlencoded(&query),
            limit.min(500)
        );

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .map_err(|e| format!("Gmail list messages request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(format!("Gmail API error {}: {}", status, body));
        }

        let list: serde_json::Value = resp
            .json()
            .map_err(|e| format!("Failed to parse message list: {}", e))?;

        let message_ids: Vec<String> = list["messages"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m["id"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        let mut messages = Vec::new();
        for id in &message_ids {
            match self.get_message(id) {
                Ok(msg) => messages.push(msg),
                Err(e) => {
                    eprintln!("Warning: failed to fetch message {}: {}", id, e);
                }
            }
        }

        Ok(messages)
    }

    fn get_message(&self, message_id: &str) -> Result<Message, String> {
        let url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=full",
            message_id
        );

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .map_err(|e| format!("Gmail get message request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(format!("Gmail API error {}: {}", status, body));
        }

        let raw: serde_json::Value = resp
            .json()
            .map_err(|e| format!("Failed to parse message: {}", e))?;

        let headers = raw["payload"]["headers"].as_array().cloned().unwrap_or_default();
        let from = header_value(&headers, "From").unwrap_or_default();
        let to = header_value(&headers, "To").unwrap_or_default();
        let subject = header_value(&headers, "Subject").unwrap_or_default();
        let date = header_value(&headers, "Date").unwrap_or_default();
        let thread_id = raw["threadId"].as_str().unwrap_or("").to_string();

        Ok(Message {
            id: raw["id"].as_str().unwrap_or("").to_string(),
            from,
            to,
            subject,
            body_text: extract_body_text(&raw["payload"]),
            body_html: extract_body_html(&raw["payload"]),
            thread_id,
            received_at: date,
        })
    }

    fn create_draft(&self, draft: &DraftEnvelope) -> Result<String, String> {
        // Build RFC 2822 message.
        let body_text = draft
            .body_text
            .clone()
            .unwrap_or_else(|| html_to_text(&draft.body_html));

        let mut raw_msg = String::new();
        raw_msg.push_str(&format!("To: {}\r\n", draft.to));
        raw_msg.push_str(&format!("Subject: {}\r\n", draft.subject));
        if let Some(ref in_reply_to) = draft.in_reply_to {
            raw_msg.push_str(&format!("In-Reply-To: {}\r\n", in_reply_to));
        }
        raw_msg.push_str("MIME-Version: 1.0\r\n");
        raw_msg.push_str("Content-Type: multipart/alternative; boundary=\"boundary_ta\"\r\n");
        raw_msg.push_str("\r\n");
        raw_msg.push_str("--boundary_ta\r\n");
        raw_msg.push_str("Content-Type: text/plain; charset=UTF-8\r\n\r\n");
        raw_msg.push_str(&body_text);
        raw_msg.push_str("\r\n--boundary_ta\r\n");
        raw_msg.push_str("Content-Type: text/html; charset=UTF-8\r\n\r\n");
        raw_msg.push_str(&draft.body_html);
        raw_msg.push_str("\r\n--boundary_ta--\r\n");

        let encoded = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            raw_msg.as_bytes(),
        );

        let body = serde_json::json!({
            "message": {
                "raw": encoded,
                "threadId": draft.thread_id,
            }
        });

        let resp = self
            .client
            .post("https://gmail.googleapis.com/gmail/v1/users/me/drafts")
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .map_err(|e| format!("Gmail create draft request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(format!("Gmail API error {}: {}", status, body));
        }

        let result: serde_json::Value = resp
            .json()
            .map_err(|e| format!("Failed to parse draft response: {}", e))?;

        result["id"]
            .as_str()
            .map(|id| format!("gmail-draft-{}", id))
            .ok_or_else(|| "Gmail did not return a draft ID".to_string())
    }

    fn get_draft_status(&self, draft_id: &str) -> Result<String, String> {
        // Strip our "gmail-draft-" prefix to get the raw Gmail draft ID.
        let raw_id = draft_id
            .strip_prefix("gmail-draft-")
            .unwrap_or(draft_id);

        let url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/drafts/{}",
            raw_id
        );

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .map_err(|e| format!("Gmail get draft request failed: {}", e))?;

        if resp.status().as_u16() == 404 {
            // Draft no longer exists — likely sent or discarded.
            return Ok("unknown".to_string());
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(format!("Gmail API error {}: {}", status, body));
        }

        let result: serde_json::Value = resp
            .json()
            .map_err(|e| format!("Failed to parse draft status: {}", e))?;

        // Check if the draft's message has the SENT label.
        let labels = result["message"]["labelIds"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if labels.contains(&"SENT") {
            Ok("sent".to_string())
        } else if labels.contains(&"DRAFT") {
            Ok("drafted".to_string())
        } else {
            Ok("unknown".to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn header_value(headers: &[serde_json::Value], name: &str) -> Option<String> {
    headers.iter().find_map(|h| {
        if h["name"].as_str().map(|n| n.eq_ignore_ascii_case(name)).unwrap_or(false) {
            h["value"].as_str().map(str::to_string)
        } else {
            None
        }
    })
}

fn extract_body_text(payload: &serde_json::Value) -> String {
    extract_part(payload, "text/plain").unwrap_or_default()
}

fn extract_body_html(payload: &serde_json::Value) -> String {
    extract_part(payload, "text/html").unwrap_or_default()
}

fn extract_part(payload: &serde_json::Value, mime: &str) -> Option<String> {
    if payload["mimeType"].as_str() == Some(mime) {
        let data = payload["body"]["data"].as_str()?;
        let decoded = base64::Engine::decode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            data,
        )
        .ok()?;
        return String::from_utf8(decoded).ok();
    }

    if let Some(parts) = payload["parts"].as_array() {
        for part in parts {
            if let Some(text) = extract_part(part, mime) {
                return Some(text);
            }
        }
    }

    None
}

fn html_to_text(html: &str) -> String {
    // Minimal HTML → plain text: strip tags, decode common entities.
    let no_tags = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n\n")
        .replace("</div>", "\n");

    let stripped: String = {
        let mut in_tag = false;
        let mut out = String::new();
        for ch in no_tags.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => out.push(ch),
                _ => {}
            }
        }
        out
    };

    stripped
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&nbsp;", " ")
}

fn rfc3339_to_epoch(s: &str) -> Option<i64> {
    // Parse just the date/time components (best-effort).
    // Format: YYYY-MM-DDTHH:MM:SSZ or similar.
    // For Gmail `after:` queries, just the date portion is sufficient.
    if s == "1970-01-01T00:00:00Z" || s.is_empty() {
        return Some(0);
    }
    // Simple extraction of date components.
    let parts: Vec<&str> = s.split('T').collect();
    if parts.is_empty() {
        return None;
    }
    let date_parts: Vec<&str> = parts[0].split('-').collect();
    if date_parts.len() < 3 {
        return None;
    }
    let year: i64 = date_parts[0].parse().ok()?;
    let month: i64 = date_parts[1].parse().ok()?;
    let day: i64 = date_parts[2].parse().ok()?;

    // Rough epoch conversion (days since 1970-01-01).
    // Accurate enough for Gmail query purposes.
    let days_since_epoch =
        (year - 1970) * 365 + (year - 1970) / 4 + day_of_year(month, day) - 1;
    Some(days_since_epoch * 86400)
}

fn day_of_year(month: i64, day: i64) -> i64 {
    let days_per_month = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let m = (month.max(1).min(12) - 1) as usize;
    days_per_month[m] + day
}

fn urlencoded(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "+".to_string(),
            other => format!("%{:02X}", other as u32),
        })
        .collect()
}

/// Read the OAuth2 access token from environment or fallback file store.
fn load_access_token(account: Option<&str>) -> Result<String, String> {
    // Plugins retrieve credentials via:
    // 1. Environment variable override (CI / test)
    // 2. Fallback file store at ~/.config/ta/secrets/

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
        "TA_SECRET_TA_MESSAGING_GMAIL_{}",
        key_suffix.to_uppercase()
    );

    if let Ok(token) = std::env::var(&env_key) {
        return Ok(token);
    }

    // Fallback: read from ~/.config/ta/secrets/
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Cannot determine home directory for credential lookup".to_string())?;

    let secrets_dir = std::path::PathBuf::from(home)
        .join(".config")
        .join("ta")
        .join("secrets");

    let file_name = format!(
        "ta-messaging_gmail_{}",
        key_suffix.to_lowercase()
    );
    let path = secrets_dir.join(&file_name);

    if path.exists() {
        let token = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read credential file: {}", e))?;
        return Ok(token.trim().to_string());
    }

    Err(format!(
        "No Gmail access token found. Set {} or run: ta adapter setup messaging/gmail",
        env_key
    ))
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
            let token = match load_access_token(None) {
                Ok(t) => t,
                Err(e) => {
                    respond(Response::err(e));
                    return;
                }
            };
            let client = GmailClient::new(token);
            match client.get_profile() {
                Ok(profile) => {
                    let address = profile["emailAddress"]
                        .as_str()
                        .unwrap_or("<unknown>")
                        .to_string();
                    let mut resp = Response::ok_empty();
                    resp.address = Some(address);
                    resp.provider = Some("gmail".to_string());
                    respond(resp);
                }
                Err(e) => respond(Response::err(format!("Gmail health check failed: {}", e))),
            }
        }

        Request::Fetch { since, account, limit } => {
            let token = match load_access_token(account.as_deref()) {
                Ok(t) => t,
                Err(e) => {
                    respond(Response::err(e));
                    return;
                }
            };
            let client = GmailClient::new(token);
            let max = limit.unwrap_or(50);
            match client.list_messages(&since, max) {
                Ok(msgs) => {
                    let mut resp = Response::ok_empty();
                    resp.messages = Some(msgs);
                    respond(resp);
                }
                Err(e) => respond(Response::err(format!("Gmail fetch failed: {}", e))),
            }
        }

        Request::CreateDraft { draft } => {
            let token = match load_access_token(None) {
                Ok(t) => t,
                Err(e) => {
                    respond(Response::err(e));
                    return;
                }
            };
            let client = GmailClient::new(token);
            match client.create_draft(&draft) {
                Ok(draft_id) => {
                    let mut resp = Response::ok_empty();
                    resp.draft_id = Some(draft_id);
                    respond(resp);
                }
                Err(e) => respond(Response::err(format!("Gmail create_draft failed: {}", e))),
            }
        }

        Request::DraftStatus { draft_id } => {
            let token = match load_access_token(None) {
                Ok(t) => t,
                Err(e) => {
                    respond(Response::err(e));
                    return;
                }
            };
            let client = GmailClient::new(token);
            match client.get_draft_status(&draft_id) {
                Ok(state) => {
                    let mut resp = Response::ok_empty();
                    resp.state = Some(state);
                    respond(resp);
                }
                Err(e) => respond(Response::err(format!("Gmail draft_status failed: {}", e))),
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
