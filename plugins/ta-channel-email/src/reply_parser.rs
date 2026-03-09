//! Reply parsing for email responses.
//!
//! Strips quoted text and email signatures, then extracts the meaningful
//! response content. Recognizes APPROVE/DENY keywords for yes_no questions.

/// Parsed reply from an email response.
#[derive(Debug, PartialEq)]
pub enum ParsedReply {
    /// Explicit approval (matched APPROVE/YES keywords).
    Approve,
    /// Explicit denial (matched DENY/NO keywords).
    Deny,
    /// Freeform text response (after stripping quoted content).
    Text(String),
    /// Empty reply after stripping — no actionable content.
    Empty,
}

/// Parse an email reply body, stripping quoted text and extracting the response.
///
/// Strips:
/// - Lines starting with `>` (quoted text)
/// - "On ... wrote:" attribution lines and everything after
/// - Lines after `---` or `___` separators (signatures)
/// - Lines after common signature markers ("--", "Sent from my")
pub fn parse_reply(body: &str) -> ParsedReply {
    let cleaned = strip_quoted_text(body);
    let trimmed = cleaned.trim();

    if trimmed.is_empty() {
        return ParsedReply::Empty;
    }

    // Check for keyword responses (case-insensitive, whole-word).
    let upper = trimmed.to_uppercase();
    let first_word = upper.split_whitespace().next().unwrap_or("");

    match first_word {
        "APPROVE" | "APPROVED" | "YES" | "LGTM" | "ACK" => ParsedReply::Approve,
        "DENY" | "DENIED" | "NO" | "REJECT" | "REJECTED" | "NACK" => ParsedReply::Deny,
        _ => ParsedReply::Text(trimmed.to_string()),
    }
}

/// Strip quoted text, attribution lines, and signature blocks from an email body.
fn strip_quoted_text(body: &str) -> String {
    let mut result_lines = Vec::new();

    for line in body.lines() {
        let trimmed = line.trim();

        // Stop at "On ... wrote:" attribution line.
        if is_attribution_line(trimmed) {
            break;
        }

        // Stop at signature separators.
        if trimmed == "--" || trimmed == "---" || trimmed == "___" {
            break;
        }

        // Stop at common mobile signature markers.
        if trimmed.starts_with("Sent from my ") || trimmed.starts_with("Get Outlook for ") {
            break;
        }

        // Skip quoted lines.
        if trimmed.starts_with('>') {
            continue;
        }

        result_lines.push(line);
    }

    result_lines.join("\n")
}

/// Check if a line is an email attribution line like "On Mon, Jan 1, 2024, user wrote:".
fn is_attribution_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    // Common patterns: "On <date>, <email> wrote:" or "On <date> at <time>, <name> wrote:"
    (lower.starts_with("on ") && lower.contains("wrote:"))
        || (lower.starts_with("le ") && lower.contains("a écrit"))  // French
        || (lower.starts_with("am ") && lower.contains("schrieb")) // German
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_approve() {
        assert_eq!(parse_reply("APPROVE"), ParsedReply::Approve);
        assert_eq!(parse_reply("approve"), ParsedReply::Approve);
        assert_eq!(parse_reply("Approved"), ParsedReply::Approve);
        assert_eq!(parse_reply("yes"), ParsedReply::Approve);
        assert_eq!(parse_reply("YES"), ParsedReply::Approve);
        assert_eq!(parse_reply("LGTM"), ParsedReply::Approve);
        assert_eq!(parse_reply("ack"), ParsedReply::Approve);
    }

    #[test]
    fn parse_deny() {
        assert_eq!(parse_reply("DENY"), ParsedReply::Deny);
        assert_eq!(parse_reply("deny"), ParsedReply::Deny);
        assert_eq!(parse_reply("Denied"), ParsedReply::Deny);
        assert_eq!(parse_reply("no"), ParsedReply::Deny);
        assert_eq!(parse_reply("NO"), ParsedReply::Deny);
        assert_eq!(parse_reply("REJECT"), ParsedReply::Deny);
        assert_eq!(parse_reply("nack"), ParsedReply::Deny);
    }

    #[test]
    fn parse_freeform_text() {
        assert_eq!(
            parse_reply("Use PostgreSQL for this project"),
            ParsedReply::Text("Use PostgreSQL for this project".into())
        );
    }

    #[test]
    fn parse_empty() {
        assert_eq!(parse_reply(""), ParsedReply::Empty);
        assert_eq!(parse_reply("   "), ParsedReply::Empty);
        assert_eq!(parse_reply("\n\n"), ParsedReply::Empty);
    }

    #[test]
    fn strip_quoted_lines() {
        let reply = "Looks good to me\n\n> Original question\n> More quoted text";
        assert_eq!(
            parse_reply(reply),
            ParsedReply::Text("Looks good to me".into())
        );
    }

    #[test]
    fn strip_attribution_block() {
        let reply = "APPROVE\n\nOn Mon, Jan 1, 2024, agent@example.com wrote:\n> Which database?";
        assert_eq!(parse_reply(reply), ParsedReply::Approve);
    }

    #[test]
    fn strip_signature() {
        let reply = "Use SQLite\n\n--\nJohn Doe\nSenior Engineer";
        assert_eq!(parse_reply(reply), ParsedReply::Text("Use SQLite".into()));
    }

    #[test]
    fn strip_mobile_signature() {
        let reply = "DENY\n\nSent from my iPhone";
        assert_eq!(parse_reply(reply), ParsedReply::Deny);
    }

    #[test]
    fn strip_outlook_signature() {
        let reply = "Yes\n\nGet Outlook for iOS";
        assert_eq!(parse_reply(reply), ParsedReply::Approve);
    }

    #[test]
    fn strip_triple_dash_separator() {
        let reply = "PostgreSQL\n\n---\nThis is a footer";
        assert_eq!(parse_reply(reply), ParsedReply::Text("PostgreSQL".into()));
    }

    #[test]
    fn only_quoted_text_is_empty() {
        let reply = "> Some quoted text\n> More quoted text";
        assert_eq!(parse_reply(reply), ParsedReply::Empty);
    }

    #[test]
    fn multiline_freeform() {
        let reply = "I think we should use PostgreSQL.\nIt has better JSON support.";
        assert_eq!(
            parse_reply(reply),
            ParsedReply::Text(
                "I think we should use PostgreSQL.\nIt has better JSON support.".into()
            )
        );
    }

    #[test]
    fn approve_with_trailing_text() {
        // First word is the keyword — rest is ignored for keyword detection
        assert_eq!(parse_reply("APPROVE this looks good"), ParsedReply::Approve);
    }

    #[test]
    fn french_attribution() {
        let reply = "D'accord\n\nLe 1 janvier 2024, agent@example.com a écrit :\n> Question";
        assert_eq!(parse_reply(reply), ParsedReply::Text("D'accord".into()));
    }

    #[test]
    fn german_attribution() {
        let reply = "APPROVE\n\nAm 1. Januar 2024 schrieb agent@example.com:\n> Frage";
        assert_eq!(parse_reply(reply), ParsedReply::Approve);
    }
}
