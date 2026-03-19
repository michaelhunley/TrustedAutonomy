# ta-channel-discord

Discord channel plugin for [Trusted Autonomy](https://github.com/Trusted-Autonomy/TrustedAutonomy).

Delivers agent questions as rich Discord embeds with interactive buttons. The bot posts `ta_ask_human` requests to a channel — reviewers click **Approve / Deny** (or choose from a list) and the response flows back to the TA daemon automatically.

## Features

- Rich embeds with colour-coded buttons (approve/deny, multiple-choice, freeform)
- **Listen mode** (`--listen`): persistent Gateway bot for slash commands (`/ta status`, `/ta goal list`) and button callbacks
- **Progress streaming**: live goal progress updates posted to Discord as agents work
- Slash command registration (`--register-commands`)

## Quick install (recommended)

Declare the plugin in your project's `.ta/project.toml`:

```toml
[plugins.discord]
type    = "channel"
version = ">=0.1.0"
source  = "registry:ta-channel-discord"
env_vars = ["TA_DISCORD_TOKEN", "TA_DISCORD_CHANNEL_ID"]
```

Then run:

```bash
ta setup resolve
```

TA downloads the pre-built binary for your platform, installs it to `.ta/plugins/channels/discord/`, and checks that the required env vars are set.

## Manual install

### 1. Create a Discord bot

1. Open the [Discord Developer Portal](https://discord.com/developers/applications) → **New Application**.
2. Go to **Bot** → **Add Bot**. Copy the **bot token** — you will need it as `TA_DISCORD_TOKEN`.
3. Under **OAuth2 → URL Generator**, select the `bot` scope and the `Send Messages` + `Embed Links` permissions.
4. Open the generated URL and invite the bot to your server.
5. In the target channel, right-click → **Copy Channel ID** (enable Developer Mode under User Settings → Advanced first). That is `TA_DISCORD_CHANNEL_ID`.

### 2. Set environment variables

```bash
export TA_DISCORD_TOKEN="your-bot-token-here"
export TA_DISCORD_CHANNEL_ID="123456789012345678"
```

Persist these in your shell profile or a `.env` file sourced before starting the daemon.

### 3. Download or build the binary

**Download** (pre-built):

```bash
# macOS (Apple Silicon)
curl -LO https://github.com/Trusted-Autonomy/ta-channel-discord/releases/latest/download/ta-channel-discord-0.1.0-aarch64-apple-darwin.tar.gz
tar xzf ta-channel-discord-0.1.0-aarch64-apple-darwin.tar.gz

# Linux x86_64
curl -LO https://github.com/Trusted-Autonomy/ta-channel-discord/releases/latest/download/ta-channel-discord-0.1.0-x86_64-unknown-linux-musl.tar.gz
tar xzf ta-channel-discord-0.1.0-x86_64-unknown-linux-musl.tar.gz
```

**Build from source**:

```bash
cargo build --release
```

### 4. Install the plugin

```bash
mkdir -p .ta/plugins/channels/discord
cp target/release/ta-channel-discord .ta/plugins/channels/discord/
cp channel.toml .ta/plugins/channels/discord/
```

Or use TA's plugin command:

```bash
ta plugin install .
```

### 5. Configure the daemon

```toml
# .ta/daemon.toml
[channels]
default_channels = ["discord"]
```

### 6. Verify

```bash
ta plugin validate
ta config channels
```

`ta config channels` should list `discord` with the `json-stdio` protocol.

## Optional: Slash commands

Register the `/ta` application command once:

```bash
export TA_DISCORD_APP_ID="your-application-id"

# Global (takes ~1 hour to propagate):
ta-channel-discord --register-commands

# Guild-only (instant, for testing):
export TA_DISCORD_GUILD_ID="your-guild-id"
ta-channel-discord --register-commands
```

Users can then run `/ta command:status`, `/ta command:goal list`, etc. in Discord.

## Listen mode (daemon-managed)

Enable the persistent Gateway listener in `daemon.toml`:

```toml
[channels.discord_listener]
enabled = true
```

The daemon auto-starts `ta-channel-discord --listen`, which:
- Handles button-click callbacks from embeds
- Forwards slash commands to the daemon API
- Streams live goal progress as agents work

## Environment variables

| Variable | Required | Description |
|---|---|---|
| `TA_DISCORD_TOKEN` | Yes | Discord bot token |
| `TA_DISCORD_CHANNEL_ID` | Yes | Target channel snowflake ID |
| `TA_DAEMON_URL` | Listen mode | TA daemon URL (default: `http://127.0.0.1:7700`) |
| `TA_DISCORD_PREFIX` | Listen mode | Message prefix for commands (default: `ta `) |
| `TA_DISCORD_APP_ID` | Slash commands | Discord Application ID |
| `TA_DISCORD_GUILD_ID` | Slash commands | Guild ID for guild-scoped registration |
| `TA_DISCORD_TOKEN_ENV` | No | Override the env var name for the token |

## daemon.toml snippet

```toml
[channels]
default_channels = ["discord"]

[channels.discord_listener]
enabled = true

[[channels.external]]
name    = "discord"
command = "/path/to/ta-channel-discord"
protocol = "json-stdio"
timeout_secs = 30
```

## Setting a bot avatar

1. Open the [Discord Developer Portal](https://discord.com/developers/applications) and select your bot application.
2. Click **General Information**.
3. Under **App Icon**, upload your avatar image.
4. Click **Save Changes**.

## Troubleshooting

**Bot doesn't post messages**: Check `TA_DISCORD_TOKEN` and `TA_DISCORD_CHANNEL_ID`. Ensure the bot has `Send Messages` and `Embed Links` permissions in the channel.

**`ta plugin validate` fails**: Run `ta plugin logs discord` to see stderr output from the plugin process.

**Button clicks do nothing**: Listen mode must be running. Set `[channels.discord_listener] enabled = true` and restart the daemon.

**Slash commands not appearing**: Global commands take ~1 hour. Use guild-scoped registration for instant testing.

## License

Apache-2.0 — see [LICENSE](https://github.com/Trusted-Autonomy/TrustedAutonomy/blob/main/LICENSE).
