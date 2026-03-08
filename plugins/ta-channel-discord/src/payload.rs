//! Discord embed and component payload builders for the channel plugin.
//!
//! Builds rich embeds with button components for different question types.
//! This is a standalone implementation that does not depend on internal TA crates.

use crate::ChannelQuestion;

/// Discord embed color constants.
const COLOR_BLURPLE: u32 = 0x5865F2; // Discord blurple — default
const COLOR_GREEN: u32 = 0x57F287; // Green — yes/no
const _COLOR_RED: u32 = 0xED4245; // Red — reserved for urgent/escalation

/// Build a Discord message payload with embed and button components.
///
/// Adapts the question type (yes/no, choice, freeform) to the appropriate
/// Discord UI components.
pub fn build_payload(question: &ChannelQuestion) -> serde_json::Value {
    let color = match question.response_hint.as_str() {
        "yes_no" => COLOR_GREEN,
        _ => COLOR_BLURPLE,
    };

    let mut embed = serde_json::json!({
        "title": format!("Agent Question (turn {})", question.turn),
        "description": &question.question,
        "color": color,
    });

    // Add context field if present.
    if let Some(ctx) = &question.context {
        embed["fields"] = serde_json::json!([{
            "name": "Context",
            "value": truncate(ctx, 1024),
            "inline": false,
        }]);
    }

    let mut payload = serde_json::json!({
        "embeds": [embed],
    });

    // Add button components based on response hint.
    match question.response_hint.as_str() {
        "yes_no" => {
            payload["components"] = serde_json::json!([{
                "type": 1, // ACTION_ROW
                "components": [
                    {
                        "type": 2, // BUTTON
                        "style": 3, // SUCCESS (green)
                        "label": "Yes",
                        "custom_id": format!("ta_{}_yes", question.interaction_id),
                    },
                    {
                        "type": 2,
                        "style": 4, // DANGER (red)
                        "label": "No",
                        "custom_id": format!("ta_{}_no", question.interaction_id),
                    }
                ]
            }]);
        }
        "choice" if !question.choices.is_empty() => {
            let buttons: Vec<serde_json::Value> = question
                .choices
                .iter()
                .enumerate()
                .take(5) // Discord limit: 5 buttons per row
                .map(|(i, choice)| {
                    serde_json::json!({
                        "type": 2,
                        "style": 1, // PRIMARY (blurple)
                        "label": truncate(choice, 80),
                        "custom_id": format!("ta_{}_choice_{}", question.interaction_id, i),
                    })
                })
                .collect();

            payload["components"] = serde_json::json!([{
                "type": 1,
                "components": buttons,
            }]);
        }
        _ => {
            // Freeform: add a footer prompting thread reply.
            if let Some(embeds) = payload["embeds"].as_array_mut() {
                if let Some(embed) = embeds.first_mut() {
                    embed["footer"] = serde_json::json!({
                        "text": format!(
                            "Reply in this thread to answer. Respond via: POST {}/api/interactions/{}/respond",
                            question.callback_url, question.interaction_id
                        )
                    });
                }
            }
        }
    }

    payload
}

/// Truncate a string to the given max length, appending "..." if truncated.
fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
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
    fn payload_choice_buttons() {
        let q = test_question("choice", vec!["PostgreSQL".into(), "SQLite".into()]);
        let payload = build_payload(&q);
        assert!(payload.get("embeds").is_some());
        assert!(payload.get("components").is_some());
        let buttons = payload["components"][0]["components"].as_array().unwrap();
        assert_eq!(buttons.len(), 2);
        assert_eq!(buttons[0]["label"], "PostgreSQL");
        assert_eq!(buttons[1]["label"], "SQLite");
    }

    #[test]
    fn payload_yes_no_buttons() {
        let q = test_question("yes_no", vec![]);
        let payload = build_payload(&q);
        let buttons = payload["components"][0]["components"].as_array().unwrap();
        assert_eq!(buttons.len(), 2);
        assert_eq!(buttons[0]["label"], "Yes");
        assert_eq!(buttons[1]["label"], "No");
    }

    #[test]
    fn payload_freeform_footer() {
        let q = test_question("freeform", vec![]);
        let payload = build_payload(&q);
        assert!(payload.get("components").is_none());
        let footer = payload["embeds"][0]["footer"]["text"].as_str().unwrap();
        assert!(footer.contains("Reply in this thread"));
        assert!(footer.contains("test-id-123"));
    }

    #[test]
    fn payload_has_context_field() {
        let q = test_question("freeform", vec![]);
        let payload = build_payload(&q);
        let fields = payload["embeds"][0]["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0]["name"], "Context");
        assert_eq!(fields[0]["value"], "Setting up backend");
    }

    #[test]
    fn payload_no_context() {
        let mut q = test_question("freeform", vec![]);
        q.context = None;
        let payload = build_payload(&q);
        assert!(payload["embeds"][0].get("fields").is_none());
    }

    #[test]
    fn payload_max_five_choice_buttons() {
        let choices: Vec<String> = (0..10).map(|i| format!("Option {}", i)).collect();
        let q = test_question("choice", choices);
        let payload = build_payload(&q);
        let buttons = payload["components"][0]["components"].as_array().unwrap();
        assert_eq!(buttons.len(), 5); // Discord limit
    }

    #[test]
    fn button_custom_ids_contain_interaction_id() {
        let q = test_question("choice", vec!["A".into(), "B".into()]);
        let payload = build_payload(&q);
        let buttons = payload["components"][0]["components"].as_array().unwrap();
        for button in buttons {
            let custom_id = button["custom_id"].as_str().unwrap();
            assert!(custom_id.contains("test-id-123"));
        }
    }

    #[test]
    fn truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long() {
        let long = "a".repeat(100);
        assert_eq!(truncate(&long, 50).len(), 50);
    }
}
