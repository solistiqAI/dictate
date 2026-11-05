# Agent plugins: who answers in "Talk it over"

"Talk it over" (menu "Darüber diskutieren", `discuss_key`) is a spoken conversation with an agent:
you speak, the agent answers aloud, you speak again. Claude Code on your machine is built in. Any
other agent comes in as a **plugin**: a small executable that Dictate calls for each step. The
repository ships one, [`agents/fleet`](../agents/fleet): it talks to a session of agent-fleet
(SOLISTIQ's service for coding-agent sessions) through its `fleet` CLI. It is a 40-line shell
script; use it as the template for your own.

## Choosing one

In the profile (`docs/teams.md`):

```toml
[talk]
agent = "fleet"     # "claude" (the default) or a plugin's name
```

The plugin is the file of that name in the data folder's `agents/`
(`~/Library/Application Support/dictate/agents/` on a Mac). `scripts/install.sh` copies the
repository's `agents/` there; put your own next to them. On Windows the file is
`<name>.exe`, `.cmd` or `.bat`.

## The protocol

Three calls, one process each. Whatever the plugin prints on stderr becomes the error the user
sees when it exits with a status other than 0.

| Call | stdin | stdout |
|---|---|---|
| `<plugin> start` | the brief (answer short and speakable) and the first thing said | the conversation's number (a positive integer) on the first line, or `{"id": n}` |
| `<plugin> ask <id>` | what the user said; empty = the answer to the start | the agent's answer, once its turn is over |
| `<plugin> close <id>` | nothing | nothing |

Environment: `DICTATE_AGENT_PROFILE` (the setting `discuss_profile` in `config.toml`, for a
plugin that knows several profiles or models) and a `PATH` that includes `~/.local/bin` and
Homebrew's folders.

Each turn takes as long as the agent needs (a few seconds for a whole agent turn), so this is a
walk-and-talk, not a phone call. Markdown in the answer is cleaned up before it is spoken.

## Sessions that started elsewhere

A web page may hand an existing conversation over: `POST http://127.0.0.1:47619/talk?session=<id>`
(its origin listed in the profile's `web_origins`) starts listening and sends what you say to
`<plugin> ask <id>`, the profile's plugin; `POST /talk/stop` ends it. The first message then carries
a one-line reminder that the answer is read aloud. Such a conversation is not closed at the end.

## Try it without the microphone

```sh
dictate say --ask "Wie spät ist es in Tokio?"   # starts, asks, speaks the answer, closes
echo "Hallo" | ~/Library/Application\ Support/dictate/agents/fleet start
```
