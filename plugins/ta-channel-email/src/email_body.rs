//! Email body builders for the channel plugin.
//!
//! Builds both HTML and plain-text email bodies for different question types.
//! This is a standalone implementation that does not depend on internal TA crates.

use crate::ChannelQuestion;

/// Build the email subject line.
///
/// Format: `[TA Review] Agent question (turn N): <truncated question>`
/// Uses `X-TA-Request-ID` header for threading (set by caller).
pub fn build_subject(question: &ChannelQuestion, prefix: &str) -> String {
    let truncated = if question.question.len() > 60 {
        // Find char boundary at or before 57 to avoid splitting multi-byte chars.
        let mut end = 57;
        while end > 0 && !question.question.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &question.question[..end])
    } else {
        question.question.clone()
    };
    format!(
        "{} Agent question (turn {}): {}",
        prefix, question.turn, truncated
    )
}

/// Build plain text email body.
pub fn build_body_text(question: &ChannelQuestion) -> String {
    let mut body = format!(
        "An agent needs your input.\n\nQuestion: {}\n",
        question.question
    );

    if let Some(ctx) = &question.context {
        body.push_str(&format!("\nContext: {}\n", ctx));
    }

    match question.response_hint.as_str() {
        "yes_no" => {
            body.push_str(
                "\nReply to this email with APPROVE or DENY to answer.\n\
                 You can also reply YES/NO or simply type your response.\n",
            );
        }
        "choice" if !question.choices.is_empty() => {
            body.push_str("\nChoices:\n");
            for (i, choice) in question.choices.iter().enumerate() {
                body.push_str(&format!("  {}. {}\n", i + 1, choice));
            }
            body.push_str("\nReply to this email with your chosen option.\n");
        }
        _ => {
            body.push_str("\nReply to this email with your answer.\n");
        }
    }

    body.push_str(&format!(
        "\n---\nInteraction: {}\nGoal: {}\n\
         Respond via API: POST {}/api/interactions/{}/respond\n\
         Body: {{\"answer\": \"your response\"}}\n",
        question.interaction_id, question.goal_id, question.callback_url, question.interaction_id
    ));

    body
}

/// Build HTML email body with structured layout.
pub fn build_body_html(question: &ChannelQuestion) -> String {
    let mut html = String::from(
        "<div style=\"font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; \
         max-width: 600px; margin: 0 auto; color: #333;\">\n",
    );

    // Header
    html.push_str(&format!(
        "<div style=\"background: #2563eb; color: white; padding: 16px 20px; \
         border-radius: 8px 8px 0 0;\">\
         <h2 style=\"margin: 0; font-size: 18px;\">Agent Question (turn {})</h2>\
         </div>\n",
        question.turn
    ));

    // Question body
    html.push_str("<div style=\"padding: 20px; border: 1px solid #e5e7eb; border-top: none;\">\n");
    html.push_str(&format!(
        "<p style=\"font-size: 16px; line-height: 1.5; margin-top: 0;\">{}</p>\n",
        html_escape(&question.question)
    ));

    // Context
    if let Some(ctx) = &question.context {
        html.push_str(&format!(
            "<div style=\"background: #f9fafb; border-left: 3px solid #d1d5db; \
             padding: 12px 16px; margin: 16px 0; color: #6b7280;\">\
             <strong>Context:</strong><br>{}</div>\n",
            html_escape(ctx)
        ));
    }

    // Response guidance
    match question.response_hint.as_str() {
        "yes_no" => {
            html.push_str(
                "<div style=\"margin: 20px 0; padding: 16px; background: #f0fdf4; \
                 border: 1px solid #bbf7d0; border-radius: 6px;\">\
                 <p style=\"margin: 0;\">Reply to this email with \
                 <strong style=\"color: #16a34a;\">APPROVE</strong> or \
                 <strong style=\"color: #dc2626;\">DENY</strong> to answer.</p>\
                 </div>\n",
            );
        }
        "choice" if !question.choices.is_empty() => {
            html.push_str(
                "<div style=\"margin: 20px 0; padding: 16px; background: #eff6ff; \
                 border: 1px solid #bfdbfe; border-radius: 6px;\">\
                 <p style=\"margin: 0 0 8px;\"><strong>Options:</strong></p><ol style=\"margin: 0;\">\n",
            );
            for choice in &question.choices {
                html.push_str(&format!(
                    "<li style=\"margin: 4px 0;\">{}</li>\n",
                    html_escape(choice)
                ));
            }
            html.push_str(
                "</ol>\n<p style=\"margin: 8px 0 0; color: #6b7280;\">Reply with your chosen option.</p>\
                 </div>\n",
            );
        }
        _ => {
            html.push_str(
                "<div style=\"margin: 20px 0; padding: 16px; background: #eff6ff; \
                 border: 1px solid #bfdbfe; border-radius: 6px;\">\
                 <p style=\"margin: 0;\">Reply to this email with your answer.</p>\
                 </div>\n",
            );
        }
    }

    html.push_str("</div>\n"); // close body div

    // Footer
    html.push_str(&format!(
        "<div style=\"padding: 12px 20px; background: #f9fafb; \
         border: 1px solid #e5e7eb; border-top: none; border-radius: 0 0 8px 8px; \
         font-size: 12px; color: #9ca3af;\">\
         Interaction: <code>{}</code> &middot; Goal: <code>{}</code>\
         </div>\n",
        question.interaction_id, question.goal_id
    ));

    html.push_str("</div>\n");
    html
}

/// Minimal HTML escaping for user-provided text inserted into HTML bodies.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_question(hint: &str, choices: Vec<String>) -> ChannelQuestion {
        ChannelQuestion {
            interaction_id: "test-id-123".into(),
            goal_id: "goal-456".into(),
            question: "Which database?".into(),
            context: Some("Setting up backend".into()),
            response_hint: hint.into(),
            choices,
            turn: 1,
            callback_url: "http://localhost:7700".into(),
        }
    }

    #[test]
    fn subject_short_question() {
        let q = test_question("yes_no", vec![]);
        let subject = build_subject(&q, "[TA Review]");
        assert_eq!(
            subject,
            "[TA Review] Agent question (turn 1): Which database?"
        );
    }

    #[test]
    fn subject_long_question_truncates() {
        let mut q = test_question("yes_no", vec![]);
        q.question = "A".repeat(100);
        let subject = build_subject(&q, "[TA Review]");
        assert!(subject.contains("..."));
        assert!(subject.len() < 120);
    }

    #[test]
    fn subject_custom_prefix() {
        let q = test_question("yes_no", vec![]);
        let subject = build_subject(&q, "[Custom]");
        assert!(subject.starts_with("[Custom]"));
    }

    #[test]
    fn body_text_yes_no() {
        let q = test_question("yes_no", vec![]);
        let body = build_body_text(&q);
        assert!(body.contains("APPROVE"));
        assert!(body.contains("DENY"));
        assert!(body.contains("test-id-123"));
    }

    #[test]
    fn body_text_choice() {
        let q = test_question("choice", vec!["PostgreSQL".into(), "SQLite".into()]);
        let body = build_body_text(&q);
        assert!(body.contains("1. PostgreSQL"));
        assert!(body.contains("2. SQLite"));
    }

    #[test]
    fn body_text_freeform() {
        let q = test_question("freeform", vec![]);
        let body = build_body_text(&q);
        assert!(body.contains("Reply to this email with your answer"));
    }

    #[test]
    fn body_text_with_context() {
        let q = test_question("freeform", vec![]);
        let body = build_body_text(&q);
        assert!(body.contains("Setting up backend"));
    }

    #[test]
    fn body_text_without_context() {
        let mut q = test_question("freeform", vec![]);
        q.context = None;
        let body = build_body_text(&q);
        assert!(!body.contains("Context:"));
    }

    #[test]
    fn body_text_has_api_endpoint() {
        let q = test_question("freeform", vec![]);
        let body = build_body_text(&q);
        assert!(body.contains("/api/interactions/test-id-123/respond"));
    }

    #[test]
    fn body_html_yes_no() {
        let q = test_question("yes_no", vec![]);
        let html = build_body_html(&q);
        assert!(html.contains("APPROVE"));
        assert!(html.contains("DENY"));
        assert!(html.contains("<h2"));
    }

    #[test]
    fn body_html_choice() {
        let q = test_question("choice", vec!["PostgreSQL".into(), "SQLite".into()]);
        let html = build_body_html(&q);
        assert!(html.contains("PostgreSQL"));
        assert!(html.contains("SQLite"));
        assert!(html.contains("<ol"));
    }

    #[test]
    fn body_html_freeform() {
        let q = test_question("freeform", vec![]);
        let html = build_body_html(&q);
        assert!(html.contains("Reply to this email with your answer"));
    }

    #[test]
    fn body_html_escapes_special_chars() {
        let mut q = test_question("freeform", vec![]);
        q.question = "Is x < y && z > w?".into();
        let html = build_body_html(&q);
        assert!(html.contains("&lt;"));
        assert!(html.contains("&gt;"));
        assert!(html.contains("&amp;"));
    }

    #[test]
    fn body_html_has_footer() {
        let q = test_question("freeform", vec![]);
        let html = build_body_html(&q);
        assert!(html.contains("test-id-123"));
        assert!(html.contains("goal-456"));
    }

    #[test]
    fn body_html_has_context() {
        let q = test_question("freeform", vec![]);
        let html = build_body_html(&q);
        assert!(html.contains("Setting up backend"));
    }

    #[test]
    fn html_escape_works() {
        assert_eq!(
            html_escape("<script>alert('xss')</script>"),
            "&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;"
        );
    }
}
