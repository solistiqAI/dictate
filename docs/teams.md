# Dictate for your team

The app has no team in its code. A team docks in with a small **team repo** of its own, kept
private, next to a checkout of this one:

```
your-org/dictate-team/        private
  profile.toml                the team's profile: words, report target, Souffleur, web pages
  brand.env                   bundle id and signing identity of your build
  AppIcon.icns, app-icon.svg  your icon (optional)
  MenuBarIcon.png             your mark in the menu bar instead of the voice bars (optional)
  install.sh                  builds and installs your build from the app checkout
  AGENTS.md                   how your people and agents deploy and read the reports (optional)
```

Nothing of it has to be in this repository, and nothing in this repository has to change for it.

## 1. The profile

Start from [`profiles/example.toml`](../profiles/example.toml); every key is described in
[`core/src/profile.rs`](../core/src/profile.rs). Point `update_from` at the file in the team repo:

```toml
name = "Example"
update_from = "gh:your-org/dictate-team/profile.toml"
dictionary = ["Example GmbH", "Pull Request"]
```

`gh:` reads it with the GitHub CLI, so the repo stays private and every machine where `gh` is
signed in can read it. An `https://` URL or a path (a shared drive) works too. The app fetches it
once a day: a new word or tool reaches everyone after a push, without a new build. A fetch that
fails keeps the installed profile. Check a profile before pushing it:
`dictate-core profile profile.toml`.

## 2. Usage reports (optional)

Without `[report]` nothing is sent. With it, each machine whose user has the switch on sends a
daily anonymous summary: counts, times, microphone levels, problem messages, learned corrections as
word pairs; never dictated text (`core/src/stats.rs`, `core/src/report.rs`;
`dictate report` prints one). Two kinds of target:

```toml
# A standing issue in a private repo, one comment per report (`gh` signed in on each machine).
# Dictate checks before every report that the repo is private and sends nothing otherwise.
[report]
repo = "your-org/dictate-team"
issue = 1
```

```toml
# Or a URL of your own: a JSON POST {"meta": …, "summary": …, "markdown": …} with curl.
# The UI shows only its host, so a token in the URL stays out of sight.
[report]
url = "https://reports.example.com/dictate"
```

Whoever looks after Dictate in the team reads them: problems that repeat, slow transcriptions,
quiet microphones, learned words that belong in the profile's dictionary.

## 3. Your build

`brand.env`:

```sh
DICTATE_BUNDLE_ID=com.example.dictate
DICTATE_SIGN_IDENTITY="Example Code Signing"   # a codesigning identity in each Mac's keychain
```

macOS ties Accessibility and the microphone to bundle id and identity: keep both stable, or every
Mac grants them again. Icon: `scripts/icon.sh app-icon.svg AppIcon.icns` in the app checkout.
Menu bar: `MenuBarIcon.png`, black on transparent and 72 px high (the menu bar shows it 18 pt
high and tints it like the icons beside it; only its opacity counts). Without it the menu bar
shows the voice bars.

`install.sh`:

```sh
#!/bin/sh
# Builds and installs our Dictate from the app checkout next to this repo.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
src=${DICTATE_SRC:-$here/../dictate}
DICTATE_BRAND=$here exec "$src/scripts/install.sh" "$@"
```

With both repos cloned side by side, `./install.sh` in the team repo builds the app, installs it
into `/Applications`, installs `profile.toml` and starts it. To update: pull both, run it again.
Profile-only changes need no install at all.
