// conversation.rs — Persistent conversation log for interactive mode.
//
// Each goal's interactive exchanges are logged as JSONL (one JSON object per
// line) for streaming reads and incremental appends. The log serves three
// purposes: audit trail, UI display (shell can render prior turns), and
// context injection (agent gets conversation_so_far in each response).

use std::fs;
use std::io::{BufRead, Write};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single turn in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub turn: u32,
    pub role: TurnRole,
    pub interaction_id: Uuid,
    pub text: String,
    pub timestamp: DateTime<Utc>,
    /// Who responded (only set for human turns).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responder_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnRole {
    Agent,
    Human,
}

/// Manages conversation logs for interactive goals.
///
/// Logs are stored as JSONL files at `<base_dir>/<goal_id>.jsonl`. Each line
/// is a serialised [`ConversationTurn`]. The append-only format supports
/// streaming reads and is safe for concurrent single-writer use.
pub struct ConversationStore {
    base_dir: PathBuf,
}

impl ConversationStore {
    /// Create a store rooted at the given directory (e.g., `.ta/conversations/`).
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// Path to the conversation log for a goal.
    fn log_path(&self, goal_id: Uuid) -> PathBuf {
        self.base_dir.join(format!("{}.jsonl", goal_id))
    }

    /// Append an agent question to the conversation log.
    pub fn append_question(
        &self,
        goal_id: Uuid,
        interaction_id: Uuid,
        turn: u32,
        question: &str,
    ) -> anyhow::Result<()> {
        let entry = ConversationTurn {
            turn,
            role: TurnRole::Agent,
            interaction_id,
            text: question.to_string(),
            timestamp: Utc::now(),
            responder_id: None,
        };
        self.append(goal_id, &entry)
    }

    /// Append a human answer to the conversation log.
    pub fn append_answer(
        &self,
        goal_id: Uuid,
        interaction_id: Uuid,
        turn: u32,
        answer: &str,
        responder_id: &str,
    ) -> anyhow::Result<()> {
        let entry = ConversationTurn {
            turn,
            role: TurnRole::Human,
            interaction_id,
            text: answer.to_string(),
            timestamp: Utc::now(),
            responder_id: Some(responder_id.to_string()),
        };
        self.append(goal_id, &entry)
    }

    /// Load all turns for a goal in append order.
    ///
    /// Returns an empty `Vec` if no log exists for `goal_id` yet.
    pub fn load(&self, goal_id: Uuid) -> anyhow::Result<Vec<ConversationTurn>> {
        let path = self.log_path(goal_id);
        if !path.exists() {
            return Ok(vec![]);
        }
        let file = fs::File::open(&path)?;
        let reader = std::io::BufReader::new(file);
        let mut turns = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let turn: ConversationTurn = serde_json::from_str(&line)?;
            turns.push(turn);
        }
        Ok(turns)
    }

    /// Get the next turn number for a goal.
    ///
    /// Returns `1` if the conversation is empty or the log does not yet exist.
    pub fn next_turn(&self, goal_id: Uuid) -> u32 {
        match self.load(goal_id) {
            Ok(turns) => turns.last().map(|t| t.turn + 1).unwrap_or(1),
            Err(_) => 1,
        }
    }

    /// Build a structured summary of prior turns for injection into an agent response.
    ///
    /// Adjacent agent/human turns with the same `turn` number are grouped into a
    /// single `{"turn": N, "question": "...", "answer": "..."}` object. Agent
    /// turns without a matching human answer produce `{"turn": N, "question": "..."}`.
    pub fn conversation_so_far(&self, goal_id: Uuid) -> anyhow::Result<Vec<serde_json::Value>> {
        let turns = self.load(goal_id)?;
        let mut result = Vec::new();
        let mut i = 0;
        while i < turns.len() {
            let agent_turn = &turns[i];
            if agent_turn.role == TurnRole::Agent {
                let mut entry = serde_json::json!({
                    "turn": agent_turn.turn,
                    "question": agent_turn.text,
                });
                // Pair with the human answer for this turn number, if present.
                if i + 1 < turns.len()
                    && turns[i + 1].turn == agent_turn.turn
                    && turns[i + 1].role == TurnRole::Human
                {
                    entry["answer"] = serde_json::Value::String(turns[i + 1].text.clone());
                    i += 1;
                }
                result.push(entry);
            }
            i += 1;
        }
        Ok(result)
    }

    fn append(&self, goal_id: Uuid, entry: &ConversationTurn) -> anyhow::Result<()> {
        fs::create_dir_all(&self.base_dir)?;
        let path = self.log_path(goal_id);
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let json = serde_json::to_string(entry)?;
        writeln!(file, "{}", json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Convenience: build a store inside a fresh temp directory.
    fn make_store() -> (ConversationStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let store = ConversationStore::new(dir.path().join("conversations"));
        (store, dir)
    }

    #[test]
    fn empty_load_returns_empty_vec() {
        let (store, _dir) = make_store();
        let goal_id = Uuid::new_v4();
        let turns = store.load(goal_id).unwrap();
        assert!(turns.is_empty(), "expected empty vec for unknown goal");
    }

    #[test]
    fn append_and_load_round_trip() {
        let (store, _dir) = make_store();
        let goal_id = Uuid::new_v4();
        let interaction_id = Uuid::new_v4();

        store
            .append_question(goal_id, interaction_id, 1, "What is the scope?")
            .unwrap();
        store
            .append_answer(goal_id, interaction_id, 1, "Just the auth module.", "alice")
            .unwrap();

        let turns = store.load(goal_id).unwrap();
        assert_eq!(turns.len(), 2);

        let q = &turns[0];
        assert_eq!(q.turn, 1);
        assert_eq!(q.role, TurnRole::Agent);
        assert_eq!(q.interaction_id, interaction_id);
        assert_eq!(q.text, "What is the scope?");
        assert!(q.responder_id.is_none());

        let a = &turns[1];
        assert_eq!(a.turn, 1);
        assert_eq!(a.role, TurnRole::Human);
        assert_eq!(a.interaction_id, interaction_id);
        assert_eq!(a.text, "Just the auth module.");
        assert_eq!(a.responder_id.as_deref(), Some("alice"));
    }

    #[test]
    fn next_turn_starts_at_one_and_increments() {
        let (store, _dir) = make_store();
        let goal_id = Uuid::new_v4();
        let iid = Uuid::new_v4();

        // No turns yet — expect 1.
        assert_eq!(store.next_turn(goal_id), 1);

        store.append_question(goal_id, iid, 1, "Q1").unwrap();
        // After an agent turn at turn=1, next should be 2.
        assert_eq!(store.next_turn(goal_id), 2);

        store.append_answer(goal_id, iid, 1, "A1", "bob").unwrap();
        // Human answer also has turn=1; last turn is still 1, so next is 2.
        assert_eq!(store.next_turn(goal_id), 2);

        store.append_question(goal_id, iid, 2, "Q2").unwrap();
        assert_eq!(store.next_turn(goal_id), 3);
    }

    #[test]
    fn conversation_so_far_groups_turns_correctly() {
        let (store, _dir) = make_store();
        let goal_id = Uuid::new_v4();
        let iid1 = Uuid::new_v4();
        let iid2 = Uuid::new_v4();

        // Turn 1: agent question + human answer.
        store
            .append_question(goal_id, iid1, 1, "What is the deadline?")
            .unwrap();
        store
            .append_answer(goal_id, iid1, 1, "End of sprint.", "carol")
            .unwrap();

        // Turn 2: agent question only (no answer yet).
        store
            .append_question(goal_id, iid2, 2, "Any blockers?")
            .unwrap();

        let summary = store.conversation_so_far(goal_id).unwrap();
        assert_eq!(summary.len(), 2, "expected two grouped entries");

        // First entry — question + answer.
        let first = &summary[0];
        assert_eq!(first["turn"], 1);
        assert_eq!(first["question"], "What is the deadline?");
        assert_eq!(first["answer"], "End of sprint.");

        // Second entry — question only, no "answer" key.
        let second = &summary[1];
        assert_eq!(second["turn"], 2);
        assert_eq!(second["question"], "Any blockers?");
        assert!(
            second.get("answer").is_none() || second["answer"].is_null(),
            "unanswered turn should not have an answer field"
        );
    }

    #[test]
    fn separate_goals_have_independent_logs() {
        let (store, _dir) = make_store();
        let goal_a = Uuid::new_v4();
        let goal_b = Uuid::new_v4();
        let iid = Uuid::new_v4();

        store
            .append_question(goal_a, iid, 1, "Goal A question")
            .unwrap();

        // Goal B should still be empty.
        let turns_b = store.load(goal_b).unwrap();
        assert!(turns_b.is_empty());

        // Goal A should have exactly one turn.
        let turns_a = store.load(goal_a).unwrap();
        assert_eq!(turns_a.len(), 1);
    }

    #[test]
    fn serialization_round_trip() {
        let turn = ConversationTurn {
            turn: 3,
            role: TurnRole::Human,
            interaction_id: Uuid::new_v4(),
            text: "Looks good to me.".to_string(),
            timestamp: Utc::now(),
            responder_id: Some("dave".to_string()),
        };
        let json = serde_json::to_string(&turn).unwrap();
        let restored: ConversationTurn = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.turn, turn.turn);
        assert_eq!(restored.role, turn.role);
        assert_eq!(restored.interaction_id, turn.interaction_id);
        assert_eq!(restored.text, turn.text);
        assert_eq!(restored.responder_id, turn.responder_id);
    }
}
