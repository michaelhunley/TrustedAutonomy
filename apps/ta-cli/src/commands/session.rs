// session.rs — Interactive session management commands.
//
// `ta session list` shows active sessions across all channels.
// `ta session show <id>` displays session details and message history.

use clap::Subcommand;
use ta_changeset::InteractiveSessionStore;
use ta_mcp_gateway::GatewayConfig;
use uuid::Uuid;

#[derive(Subcommand)]
pub enum SessionCommands {
    /// List interactive sessions.
    List {
        /// Show all sessions (including completed/aborted). Default: alive only.
        #[arg(long)]
        all: bool,
    },
    /// Show details for a specific session.
    Show {
        /// Session ID (full or prefix).
        id: String,
    },
}

pub fn execute(cmd: &SessionCommands, config: &GatewayConfig) -> anyhow::Result<()> {
    let store = InteractiveSessionStore::new(config.interactive_sessions_dir.clone())?;

    match cmd {
        SessionCommands::List { all } => list_sessions(&store, *all),
        SessionCommands::Show { id } => show_session(&store, id),
    }
}

fn list_sessions(store: &InteractiveSessionStore, all: bool) -> anyhow::Result<()> {
    let sessions = if all {
        store.list()?
    } else {
        store.list_alive()?
    };

    if sessions.is_empty() {
        if all {
            println!("No interactive sessions found.");
        } else {
            println!("No active interactive sessions. Use --all to see completed sessions.");
        }
        return Ok(());
    }

    println!(
        "{:<38} {:<38} {:<12} {:<14} {:<10}",
        "SESSION ID", "GOAL ID", "AGENT", "STATE", "ELAPSED"
    );
    println!("{}", "-".repeat(112));

    for s in &sessions {
        println!(
            "{:<38} {:<38} {:<12} {:<14} {:<10}",
            s.session_id,
            s.goal_id,
            truncate(&s.agent_id, 10),
            s.state.to_string(),
            s.elapsed_display(),
        );
    }

    println!("\n{} session(s).", sessions.len());
    Ok(())
}

fn show_session(store: &InteractiveSessionStore, id: &str) -> anyhow::Result<()> {
    // Try exact UUID parse first, then prefix match.
    let session = if let Ok(uuid) = Uuid::parse_str(id) {
        store.load(uuid)?
    } else {
        let all = store.list()?;
        let matches: Vec<_> = all
            .into_iter()
            .filter(|s| s.session_id.to_string().starts_with(id))
            .collect();
        match matches.len() {
            0 => anyhow::bail!("No session found matching '{}'", id),
            1 => matches.into_iter().next().unwrap(),
            n => anyhow::bail!("Ambiguous prefix '{}' matches {} sessions", id, n),
        }
    };

    println!("Session:   {}", session.session_id);
    println!("Goal:      {}", session.goal_id);
    println!("Channel:   {}", session.channel_id);
    println!("Agent:     {}", session.agent_id);
    println!("State:     {}", session.state);
    println!("Created:   {}", session.created_at.to_rfc3339());
    println!("Updated:   {}", session.updated_at.to_rfc3339());
    println!("Elapsed:   {}", session.elapsed_display());

    if !session.draft_ids.is_empty() {
        println!("Drafts:    {}", session.draft_ids.len());
        for draft_id in &session.draft_ids {
            println!("  - {}", draft_id);
        }
    }

    if !session.messages.is_empty() {
        println!("\nMessage log ({} messages):", session.messages.len());
        println!("{}", "-".repeat(60));
        for msg in &session.messages {
            let time = msg.timestamp.format("%H:%M:%S");
            let preview = truncate(&msg.content, 70);
            println!("  [{}] {}: {}", time, msg.sender, preview);
        }
    }

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max - 3])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ta_changeset::InteractiveSession;
    use tempfile::TempDir;

    #[test]
    fn list_empty_sessions() {
        let temp = TempDir::new().unwrap();
        let config = GatewayConfig::for_project(temp.path());
        let store = InteractiveSessionStore::new(config.interactive_sessions_dir.clone()).unwrap();

        // Should not error on empty list.
        let sessions = store.list().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn show_session_by_prefix() {
        let temp = TempDir::new().unwrap();
        let config = GatewayConfig::for_project(temp.path());
        let store = InteractiveSessionStore::new(config.interactive_sessions_dir.clone()).unwrap();

        let session = InteractiveSession::new(
            Uuid::new_v4(),
            "cli:tty0".to_string(),
            "claude-code".to_string(),
        );
        let id_prefix = session.session_id.to_string()[..8].to_string();
        store.save(&session).unwrap();

        // Should find by prefix.
        let all = store.list().unwrap();
        let matches: Vec<_> = all
            .into_iter()
            .filter(|s| s.session_id.to_string().starts_with(&id_prefix))
            .collect();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].session_id, session.session_id);
    }

    #[test]
    fn session_store_integrates_with_gateway_config() {
        let temp = TempDir::new().unwrap();
        let config = GatewayConfig::for_project(temp.path());

        // The interactive_sessions_dir should be under .ta/
        assert!(config
            .interactive_sessions_dir
            .to_str()
            .unwrap()
            .contains(".ta"));

        // Store creation should work.
        let store = InteractiveSessionStore::new(config.interactive_sessions_dir.clone()).unwrap();

        let session = InteractiveSession::new(
            Uuid::new_v4(),
            "cli:tty0".to_string(),
            "test-agent".to_string(),
        );
        store.save(&session).unwrap();

        let loaded = store.load(session.session_id).unwrap();
        assert_eq!(loaded.session_id, session.session_id);
    }
}
