//! ta-messaging-outlook — Outlook / Microsoft Graph messaging adapter.
//!
//! Implements the TA messaging plugin protocol (JSON-over-stdio, version 1).
//! Reads one JSON request line from stdin, writes one JSON response line to
//! stdout, then exits.
//!
//! ## Operations
//!
//! - `fetch`         — List messages since a watermark via Graph API
//! - `create_draft`  — Create a draft via `POST /messages` with `isDraft:true`
//! - `draft_status`  — Poll via `GET /messages/{id}`
//! - `health`        — Check connectivity and return connected address
//! - `capabilities`  — Return the op list above
//!
//! ## Credentials
//!
//! OAuth2 access token is read from the environment variable
//! `TA_SECRET_TA_MESSAGING_OUTLOOK_<ADDRESS>` or from
//! `~/.config/ta/secrets/ta-messaging_outlook_<address>`.
//!
//! Set up credentials with: `ta adapter setup messaging/outlook`
//!
//! ## Safety
//!
//! There is no `send` operation. TA only creates drafts; the user sends
//! from their Outlook client. This is enforced at the type level.

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
// Microsoft Graph API client
// ---------------------------------------------------------------------------

struct GraphClient {
    access_token: String,
    client: reqwest::blocking::Client,
}

const GRAPH_BASE: &str = "https://graph.microsoft.com/v1.0";

impl GraphClient {
    fn new(access_token: String) -> Self {
        Self {
            access_token,
            client: reqwest::blocking::Client::new(),
        }
    }

    fn get_me(&self) -> Result<serde_json::Value, String> {
        let resp = self
            .client
            .get(format!("{}/me", GRAPH_BASE))
            .bearer_auth(&self.access_token)
            .send()
            .map_err(|e| format!("Graph API request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(format!("Graph API error {}: {}", status, body));
        }

        resp.json::<serde_json::Value>()
            .map_err(|e| format!("Failed to parse Graph /me response: {}", e))
    }

    fn list_messages(&self, since_rfc3339: &str, limit: u32) -> Result<Vec<Message>, String> {
        // Graph uses OData filter: receivedDateTime gt <ISO 8601>
        let filter = format!("receivedDateTime gt {}", since_rfc3339);
        let url = format!(
            "{}/me/messages?$filter={}&$top={}&$select=id,from,toRecipients,subject,body,receivedDateTime,conversationId,isDraft",
            GRAPH_BASE,
            urlencoded(&filter),
            limit.min(250)
        );

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .header("Prefer", "outlook.body-content-type=\"html\"")
            .send()
            .map_err(|e| format!("Graph list messages request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(format!("Graph API error {}: {}", status, body));
        }

        let result: serde_json::Value = resp
            .json()
            .map_err(|e| format!("Failed to parse message list: {}", e))?;

        let items = result["value"].as_array().cloned().unwrap_or_default();
        let messages = items
            .iter()
            .filter(|m| !m["isDraft"].as_bool().unwrap_or(false))
            .map(graph_message_to_ta)
            .collect();

        Ok(messages)
    }

    fn create_draft(&self, draft: &DraftEnvelope) -> Result<String, String> {
        let body_content = draft.body_html.clone();
        let body_text = draft
            .body_text
            .clone()
            .unwrap_or_else(|| strip_html(&draft.body_html));

        let payload = serde_json::json!({
            "subject": draft.subject,
            "isDraft": true,
            "body": {
                "contentType": "HTML",
                "content": body_content,
            },
            "toRecipients": [{
                "emailAddress": { "address": draft.to }
            }],
            // Plain text alternative for clients that don't render HTML.
            "uniqueBody": {
                "contentType": "Text",
                "content": body_text,
            },
            // Associate with an existing conversation thread if provided.
            "conversationId": draft.thread_id,
            // If replying, set the In-Reply-To header via Graph's replyTo.
        });

        let resp = self
            .client
            .post(format!("{}/me/messages", GRAPH_BASE))
            .bearer_auth(&self.access_token)
            .json(&payload)
            .send()
            .map_err(|e| format!("Graph create draft request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(format!("Graph API error {}: {}", status, body));
        }

        let result: serde_json::Value = resp
            .json()
            .map_err(|e| format!("Failed to parse create message response: {}", e))?;

        result["id"]
            .as_str()
            .map(|id| format!("outlook-draft-{}", id))
            .ok_or_else(|| "Graph API did not return a message ID".to_string())
    }

    fn get_draft_status(&self, draft_id: &str) -> Result<String, String> {
        let raw_id = draft_id
            .strip_prefix("outlook-draft-")
            .unwrap_or(draft_id);

        let url = format!(
            "{}/me/messages/{}?$select=isDraft,isRead,receivedDateTime",
            GRAPH_BASE, raw_id
        );

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .map_err(|e| format!("Graph get message request failed: {}", e))?;

        if resp.status().as_u16() == 404 {
            // Message no longer exists — likely sent/deleted.
            return Ok("unknown".to_string());
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(format!("Graph API error {}: {}", status, body));
        }

        let result: serde_json::Value = resp
            .json()
            .map_err(|e| format!("Failed to parse message status: {}", e))?;

        if result["isDraft"].as_bool().unwrap_or(false) {
            Ok("drafted".to_string())
        } else {
            // Message exists but isDraft=false — it was sent.
            Ok("sent".to_string())
        }
    }
}

fn graph_message_to_ta(m: &serde_json::Value) -> Message {
    let from = m["from"]["emailAddress"]["address"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let to = m["toRecipients"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|r| r["emailAddress"]["address"].as_str())
        .unwrap_or("")
        .to_string();
    let subject = m["subject"].as_str().unwrap_or("").to_string();
    let body_content = m["body"]["content"].as_str().unwrap_or("").to_string();
    let body_type = m["body"]["contentType"].as_str().unwrap_or("text");
    let (body_html, body_text) = if body_type.eq_ignore_ascii_case("html") {
        let text = strip_html(&body_content);
        (body_content, text)
    } else {
        (String::new(), body_content)
    };

    Message {
        id: m["id"].as_str().unwrap_or("").to_string(),
        from,
        to,
        subject,
        body_html,
        body_text,
        thread_id: m["conversationId"].as_str().unwrap_or("").to_string(),
        received_at: m["receivedDateTime"].as_str().unwrap_or("").to_string(),
    }
}

fn strip_html(html: &str) -> String {
    let mut in_tag = false;
    let mut out = String::new();
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            '\n' | '\r' => {
                if !in_tag {
                    out.push(ch);
                }
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
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

fn load_access_token(account: Option<&str>) -> Result<String, String> {
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
        "TA_SECRET_TA_MESSAGING_OUTLOOK_{}",
        key_suffix.to_uppercase()
    );

    if let Ok(token) = std::env::var(&env_key) {
        return Ok(token);
    }

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Cannot determine home directory".to_string())?;

    let path = std::path::PathBuf::from(home)
        .join(".config")
        .join("ta")
        .join("secrets")
        .join(format!("ta-messaging_outlook_{}", key_suffix.to_lowercase()));

    if path.exists() {
        let token = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read credential file: {}", e))?;
        return Ok(token.trim().to_string());
    }

    Err(format!(
        "No Outlook access token found. Set {} or run: ta adapter setup messaging/outlook",
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
            let client = GraphClient::new(token);
            match client.get_me() {
                Ok(me) => {
                    let address = me["mail"]
                        .as_str()
                        .or_else(|| me["userPrincipalName"].as_str())
                        .unwrap_or("<unknown>")
                        .to_string();
                    let mut resp = Response::ok_empty();
                    resp.address = Some(address);
                    resp.provider = Some("outlook".to_string());
                    respond(resp);
                }
                Err(e) => respond(Response::err(format!("Outlook health check failed: {}", e))),
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
            let client = GraphClient::new(token);
            match client.list_messages(&since, limit.unwrap_or(50)) {
                Ok(msgs) => {
                    let mut resp = Response::ok_empty();
                    resp.messages = Some(msgs);
                    respond(resp);
                }
                Err(e) => respond(Response::err(format!("Outlook fetch failed: {}", e))),
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
            let client = GraphClient::new(token);
            match client.create_draft(&draft) {
                Ok(draft_id) => {
                    let mut resp = Response::ok_empty();
                    resp.draft_id = Some(draft_id);
                    respond(resp);
                }
                Err(e) => respond(Response::err(format!("Outlook create_draft failed: {}", e))),
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
            let client = GraphClient::new(token);
            match client.get_draft_status(&draft_id) {
                Ok(state) => {
                    let mut resp = Response::ok_empty();
                    resp.state = Some(state);
                    respond(resp);
                }
                Err(e) => respond(Response::err(format!("Outlook draft_status failed: {}", e))),
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
