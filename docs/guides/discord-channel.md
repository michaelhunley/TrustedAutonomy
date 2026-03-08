# Setting Up a Discord Channel Plugin for TA

This guide covers how to connect Trusted Autonomy to a Discord server so that
agent questions, draft reviews, and notifications appear as Discord messages.

---

## Architecture Overview

```
Agent needs human input
        │
        ▼
  TA Daemon (AgentNeedsInput event)
        │  builds ChannelQuestion
        ▼
  ChannelDispatcher
        │  routes to "discord" adapter
        ▼
  Discord Plugin (ta-channel-discord)
        │  reads ChannelQuestion JSON from stdin
        │  posts embed with buttons to Discord
        │  writes DeliveryResult JSON to stdout
        ▼
  Human clicks button in Discord
        │
        ▼
  POST /api/interactions/{id}/respond
        │  (via Discord bot interaction handler)
        ▼
  Agent receives response
```

Discord is an **external channel plugin** that speaks the JSON-over-stdio
protocol. TA spawns the plugin process, sends the question, and the plugin
posts it to Discord.

---

## Part 1: Discord Setup

### 1.1 Create a Discord Application

1. Go to https://discord.com/developers/applications
2. Click **New Application**, name it `TA Review Bot` (or similar)
3. Go to the **Bot** tab
4. Click **Reset Token** and copy the bot token — you'll need this later
5. Under **Privileged Gateway Intents**, enable:
   - **Message Content Intent** (needed to read approval commands)
6. Click **Save Changes**

### 1.2 Set Bot Permissions

Go to **OAuth2 → URL Generator**:

1. Under **Scopes**, select `bot` and `applications.commands`
2. Under **Bot Permissions**, select:
   - Send Messages
   - Embed Links
   - Use Slash Commands
   - Read Message History
   - Add Reactions
3. Copy the generated URL and open it in your browser
4. Select your server and authorize the bot

### 1.3 Create a Review Channel

In your Discord server:

1. Create a channel called `#ta-reviews` (or whatever you prefer)
2. Restrict the channel so only you and the bot can post
3. Copy the **channel ID**: right-click the channel → Copy Channel ID
   (enable Developer Mode in Discord Settings → Advanced if you don't see this)

---

## Part 2: Plugin Installation

### Option A: Install from Pre-Built Plugin

If you have the `ta-channel-discord` binary available:

```bash
# Create the plugin directory
mkdir -p .ta/plugins/channels/discord

# Copy the binary and manifest
cp /path/to/ta-channel-discord .ta/plugins/channels/discord/
cp /path/to/channel.toml .ta/plugins/channels/discord/
```

### Option B: Build from Source

```bash
# From the TA project root
cd plugins/ta-channel-discord
cargo build --release

# Install the plugin
mkdir -p ../../.ta/plugins/channels/discord
cp target/release/ta-channel-discord ../../.ta/plugins/channels/discord/
cp channel.toml ../../.ta/plugins/channels/discord/
```

### Option C: Configure via daemon.toml (Inline)

Instead of installing the plugin directory, you can configure Discord
directly in `.ta/daemon.toml`:

```toml
[[channels.external]]
name = "discord"
command = "/path/to/ta-channel-discord"
protocol = "json-stdio"
timeout_secs = 30

[channels]
default_channels = ["discord"]
```

---

## Part 3: Configuration

### 3.1 Set Environment Variables

```bash
export TA_DISCORD_TOKEN="your-bot-token-here"
export TA_DISCORD_CHANNEL_ID="your-channel-id-here"
```

> **Security**: Never commit bot tokens. Add them to your shell profile or
> use a `.env` file that's in `.gitignore`.

### 3.2 Configure Default Channels

Edit `.ta/daemon.toml` to route questions to Discord:

```toml
[channels]
default_channels = ["discord"]
```

### 3.3 Plugin Discovery

TA automatically discovers plugins from two locations:

- **Project-local**: `.ta/plugins/channels/discord/channel.toml`
- **User-global**: `~/.config/ta/plugins/channels/discord/channel.toml`

The plugin directory must contain:
- `channel.toml` — plugin manifest
- `ta-channel-discord` — the plugin binary (or whatever `command` points to)

### 3.4 Verify Installation

```bash
# Start the daemon
ta daemon start

# Check that Discord is registered
ta config channels
```

You should see `discord` in the registered channel types list.

---

## Part 4: Usage

Once configured, any agent question will automatically be delivered to Discord:

```bash
# Start a goal — questions route to Discord
ta run "implement feature X"
```

Questions appear as Discord embeds with:
- **Yes/No** questions: Green Yes and Red No buttons
- **Choice** questions: Blurple buttons for each option (max 5)
- **Freeform** questions: Instructions to reply in a thread

Responses come back to TA via the daemon's HTTP API:
```
POST http://localhost:7700/api/interactions/{interaction_id}/respond
```

---

## Migration from Native Discord Channel

If you previously used the built-in `ta-channel-discord` crate (v0.10.1),
follow these migration steps:

1. **Install the plugin** (see Part 2 above)

2. **Update `.ta/config.yaml`**: If you had `type: discord` in your review
   channel config, switch to `type: terminal` or `type: web` for the review
   channel. Discord now works through the daemon's channel delivery system,
   not the MCP gateway's review channel.

3. **Update `.ta/daemon.toml`**: Replace `[channels.discord]` with a plugin
   configuration:

   Before:
   ```toml
   [channels.discord]
   bot_token = "..."
   channel_id = "..."
   ```

   After:
   ```toml
   [channels]
   default_channels = ["discord"]
   ```
   (Set `TA_DISCORD_TOKEN` and `TA_DISCORD_CHANNEL_ID` as environment variables)

4. **Remove old config**: The `[channels.discord]` section in daemon.toml is
   no longer used and will produce a warning.

---

## Troubleshooting

| Problem | Solution |
|---|---|
| Bot doesn't post | Check `TA_DISCORD_TOKEN` and `TA_DISCORD_CHANNEL_ID`. Verify bot has Send Messages permission in the channel. |
| Plugin not found | Run `ta config channels` to see registered types. Verify `channel.toml` exists in `.ta/plugins/channels/discord/`. |
| "Failed to spawn plugin" | Ensure the `ta-channel-discord` binary is in the plugin directory and is executable (`chmod +x`). |
| Button clicks do nothing | Set up an interaction handler (Discord bot or webhook) that POSTs responses to `{callback_url}/api/interactions/{id}/respond`. |
| Timeout errors | Increase `timeout_secs` in `channel.toml` (default: 30s). |

---

## Environment Variables Reference

| Variable | Required | Default | Description |
|---|---|---|---|
| `TA_DISCORD_TOKEN` | Yes | — | Discord bot token |
| `TA_DISCORD_CHANNEL_ID` | Yes | — | Discord channel snowflake ID |
| `TA_DISCORD_TOKEN_ENV` | No | `TA_DISCORD_TOKEN` | Override the token env var name |
