# AGENTS.md

Guidance for coding agents (Claude Code, Codex, …) in this repository. `CLAUDE.md` links here.


## What this repo is

`dictate`: local dictation, written in Rust. A trigger (key or mouse button) records from the
microphone, Parakeet Ultra transcribes on the device (on the Mac's Neural Engine via FluidAudio in
the Swift helper `dictate-ane`; other models on the CPU via `parakeet-rs`/ONNX Runtime), and the text
is pasted at the cursor. macOS app; Windows and Linux as untested betas. Public, MIT
(`LICENSE`, `NOTICE.md`). See `README.md`, `docs/features.md`, `docs/architecture.md`.

## Workspace: what needs a Mac and what does not

A Cargo workspace of two crates (`docs/architecture.md`):

- **`dictate-core`** (`core/`): the platform-free part: models and `Asr` on the CPU (parakeet-rs/ONNX
  Runtime), engines, VAD, the dictionary and the profile (`profile.rs`, `profiles/`), replacements,
  learning, spoken commands, the layout check of "Formatieren", `config.toml`, triggers and the
  Windows hook logic, the Linux key mapping (`linux.rs`), the UI texts (`text.rs`), stats and the report, the WER eval, and a small CLI
  (`dictate-core eval | transcribe | profile <file> | model list | model download`). Builds, tests and
  evals on Linux. The Neural Engine engine is not in it: the app plugs it in with `engine::register_ane`.
- **`dictate`** (root, `src/`): the app and CLI: menu bar, overlay, window, microphone (cpal), paste,
  the Swift helper (`platform/macos/ane.rs`, Core ML), Gemma/llama.cpp ("Korrigieren"), Apple's model,
  voices, Souffleur. OS code in `src/platform/` (`macos/`, `windows/`, `linux/`). It re-exports the
  core's modules under their old names, so `crate::vocab::…` still works there. Builds on macOS, and
  for Windows and Linux as untested betas (below).

On Linux:

- `cargo test -p dictate-core` and `cargo clippy -p dictate-core --all-targets` (must be clean),
  `cargo fmt --check`. The app (`-p dictate`) needs the GTK/WebKitGTK `-dev` packages
  (`docs/linux.md`); without them `cargo check`/`clippy` still run with stub pkg-config files (same
  doc), linking and its tests need `scripts/check-linux.sh` (Docker). On Linux ONNX Runtime is loaded at run time (`ort` feature `load-dynamic`;
  `ORT_DYLIB_PATH`, else `dictate-core model download onnxruntime`'s copy in the data folder, else
  next to the executable: `core/src/onnxruntime.rs`), so the unit tests need no runtime at all.
- The eval: `scripts/eval-clips.sh --baseline eval/baseline-cpu.json` (below). It uses the
  downloaded ONNX Runtime 1.31 (else fetches it from its GitHub release) and the CPU model with `dictate-core model download
  parakeet-ultra` (2.5 GB, Hugging Face, into `~/.local/share/dictate/models/`,
  `$XDG_DATA_HOME/dictate/models/` when set; point `XDG_DATA_HOME` elsewhere where `$HOME` is
  read-only).
- `ui/` works anywhere (`npm ci`, `npm test`, `npm run check`, Playwright against the mock).
- Needs a Mac: anything in `src/platform/macos/` and macOS-only code in `src/`, `scripts/eval.sh` (macOS voices, Neural Engine), the Swift
  helper, Gemma with Metal, rendering new eval clips. A change to `src/` made elsewhere says so in its
  PR, so someone builds and tests it on a Mac.

## Commands

- Build: `cargo build --release` (both crates; `target/release/dictate` and `dictate-core`)
- Tests: `cargo test --release` (unit tests of both crates, no model needed); on Linux
  `cargo test --release -p dictate-core`
- Lint: `cargo clippy --release --all-targets` must be clean (Linux: see "Linux (beta)"); `cargo fmt`.
- App bundle only: `./scripts/bundle.sh` → `target/Dictate.app`.
- Neural Engine helper (`ane/`, Swift + FluidAudio, pinned by revision in `Package.swift`):
  `swift build -c release --package-path ane` (Command Line Tools suffice; `bundle.sh` does it and puts
  `dictate-ane` next to `dictate`). The first build fetches FluidAudio (~3 min).
- Page (main window, `ui/`, Svelte 5): `npm ci` once, `npm run build` before `cargo build` (build.rs embeds
  `ui/dist`; `bundle.sh` does both), `npm test`, `npm run check`; run the Svelte autofixer on changed
  components. Design checks: `npm run dev` serves the page against a mock app (`src/lib/mock.ts`), screenshot
  it with Playwright; the protocol samples in `ui/src/lib/protocol.fixtures.json` are checked by both sides.
- Overlay design check without a window on screen: `dictate overlay-preview out.png` (transparent PNG;
  composite on dark to look at it; `--strike "…"` shows words struck through as live "Korrigieren" does;
  `--shortcuts [12:03]` the Einstellungen/Souffleur capsules beside the pill while recording).
  The bar at the bottom: `dictate bar-preview out.png [--recording] [--meeting]`.
  English texts: `DICTATE_LANG=en` before either (the app follows `ui_language`; page: `?lang=en`).
  The page as on Windows or Linux (key names, permission hints, no Apple models): `?platform=windows|linux`
  (the app sends `platform` in its state; texts that differ there are `WINDOWS`/`LINUX` in `ui/src/lib/strings.ts`).
- Layout without a microphone: `./target/release/dictate format "…"` (spoken commands, then Gemma's
  layout, Apple's model via `dictate-ane format` without Gemma or with `--model apple`; the note says
  why a text stayed as it was). `dictate correct --format "…"` = the default shortcut (one Gemma pass).
- Self-corrections without a microphone: `./target/release/dictate correct "…"` (filler words, then
  Gemma 4 E4B via llama.cpp where a cue is, Apple's model via `dictate-ane correct` without it or with
  `--model apple`; deletions only; `--format` lays it out too; `-` reads one text per line and prints
  JSON lines, the model loaded once). Both models get `src/correct-prompt.txt` (a test keeps the Swift
  copy equal); try a prompt on Apple's without rebuilding: `DICTATE_ANE_INSTRUCTIONS=prompt.txt`, on
  Gemma's: `DICTATE_CORRECT_PROMPT=prompt.txt`; another GGUF: `DICTATE_LLM_FILE=…`; raw answers and
  tokens: `DICTATE_LLM_DEBUG=1`. Score with `dictate correct --eval eval/corrections.tsv`.
- llama.cpp (`llama-cpp-2`, `src/llm.rs`) needs `cmake` to build (not in Command Line Tools):
  `uv tool install cmake`. Metal kernels are embedded as source and compiled at load, so no Xcode.
  Text model: `dictate model download gemma-4-e4b` (5 GB, same models folder).
- "Talk it over" with another agent: an agent plugin in `agents/` (`docs/agent-plugins.md`: `start`,
  `ask <id>`, `close <id>`; `agents/fleet` is the example), installed into the data folder's
  `agents/` by `install.sh`, chosen by the profile's `[talk] agent`. Try one without the
  microphone: `./target/release/dictate say --ask "…"`.
- Souffleur (meeting mode) without a meeting: `./target/release/dictate souffleur x.wav` plays a
  recording as "Andere" through transcription and the real Claude Code agent (your login, read-only
  by prompt only) and prints lines, tool use and cards; the panel's design via `npm run dev`,
  `?view=panel&mock=meeting`. The system audio tap (`dictate-ane system-audio`) asks for
  "Systemaudioaufnahme" on the user's screen: not from an unattended agent. Saved meetings
  (transcript, cards, "Das Wichtigste" at the end; `meetings/` in the data folder):
  `dictate souffleur list`, `dictate souffleur show [latest|N] [--answers] [--path]`.
- End to end without a microphone: make speech with `say -v Anna -o x.aiff "…"`, convert with
  `afconvert -f WAVE -d LEI16@48000 x.aiff x.wav`, then `./target/release/dictate transcribe x.wav`
  (needs `dictate model download` once). Without a Mac: `cargo run --release -p dictate-core --
  transcribe eval/clips/s01-anna.flac` (CPU model, `model download parakeet-ultra` once).

## Eval

Two evals on `eval/sentences.tsv`, measuring WER and dictionary words (with the word list
`eval/profile.toml`, which the scripts pass, so a profile change does not move the baselines). Run
one before and after a change to the dictionary, boosting, replacements, filters or VAD with its
baseline; after an intended change `--update-baseline` and commit the file with the reason.

- **Anywhere:** `scripts/eval-clips.sh --baseline eval/baseline-cpu.json`: the pre-rendered clips in
  `eval/clips/` (four macOS voices × 32 sentences, 16 kHz mono FLAC, 8 MB, committed; synthetic speech
  only) through `dictate-core eval` with the CPU model `parakeet-ultra`. About a minute on a Mac or
  Linux; the same numbers on both so far (macOS arm64: WER 9.4 %, 65/132 dictionary words since
  the word list of 2026-11-05; Linux matched the Mac on the list before).
- **On a Mac:** `scripts/eval.sh --baseline eval/baseline.json` (macOS voices rendered on the fly,
  nine of them, the default Neural Engine model; `DICTATE_EVAL_VOICES="Anna"` for a quick run). The
  Neural Engine numbers are what Mac users get.

New mishearings become sentences in `eval/sentences.tsv` (keep the ids). Changing the list needs a
Mac: `scripts/render-clips.sh` renders the new clips, then new baselines for both evals.

## Build and install

No backend, no updater: Dictate is one binary in a signed `.app`, built and installed per Mac.

- **Install on this Mac:** `./scripts/install.sh`. It runs `bundle.sh`, quits the running app,
  replaces `/Applications/Dictate.app` (`DICTATE_INSTALL_DIR` for another folder), re-registers it
  with LaunchServices, starts it and links the CLI as `~/.local/bin/dictate` (`DICTATE_BIN_DIR`). It
  restarts the app on the user's screen: only when the user asked for it.
- **A team's build:** `DICTATE_BRAND=<folder>` gives bundle id, signing identity (`brand.env`), icon
  (`AppIcon.icns`) and profile (`profile.toml`) to `bundle.sh`/`install.sh`. Without one: bundle id
  `app.dictate`, `assets/AppIcon.icns`, `$DICTATE_SIGN_IDENTITY` or ad-hoc. An installed app with
  another bundle id keeps it (its permissions stay).
- **Signing:** macOS ties Accessibility and Microphone to bundle id + identity: keep both stable or
  the user has to grant both again (ad-hoc = every build).
- **Version:** `0.<minor>.<commits on HEAD>` (`+dirty` with uncommitted changes); `CFBundleVersion`
  adds a timestamp so macOS never serves a cached icon or bundle.
- **Model:** not part of the app; `dictate model download` once per Mac (into
  `~/Library/Application Support/dictate/models/`).
- **Check after install:** `pgrep -lf Dictate.app` and the end of
  `~/Library/Application Support/dictate/dictate.log` ("model loaded", "microphone: …", "Bereit"/"Ready").
- **Icon:** `assets/app-icon.svg` → `scripts/icon.sh` → `assets/AppIcon.icns` (commit both).

## Windows (beta)

The app also compiles for Windows (`src/platform/windows/`, `core/src/hook.rs`); it has **never run on
a real Windows PC**. What works in code, what is missing, how
to build it on Windows and what to try first: `docs/windows.md`.

- Check from a Mac after touching `src/` or `core/`: `scripts/check-windows.sh` (`cargo xwin check`
  and `clippy -D warnings` for `x86_64-pc-windows-msvc`; no linking, nothing runs). Needs once:
  `rustup target add x86_64-pc-windows-msvc`, `cargo install --locked cargo-xwin`, Homebrew LLVM
  (`clang-cl`, keg-only, the script finds it), `uv tool install ninja`. Not checkable from a Mac:
  `--features vulkan` / `cuda` (llama.cpp needs the Vulkan SDK / nvcc).
- OS calls go through `src/platform/` with the same names on every side; a new one needs a macOS,
  a Windows and a Linux version (a stand-in that says "not supported" is fine). Windows-only features
  `vulkan`, `cuda`, `directml` (GPU); never turn them on for the Mac build.
- Say in the PR or commit that Windows changes are compile-checked only.

## Linux (beta)

The app also compiles for Linux (`src/platform/linux/`, `core/src/linux.rs`); it has **never run on
a real Linux desktop**. What works in code (XInput2 or `/dev/input` triggers, XTest/`ydotool`/`wtype`
paste, a GTK overlay, WebKitGTK window), what is missing, the Wayland trade-offs, how to build it and
what to try first: `docs/linux.md`.

- Check after touching `src/` or `core/`: `scripts/check-linux.sh` (Docker, `rust:1.99` plus the
  `-dev` packages: `cargo check`, `clippy -D warnings` and `cargo test` of both crates). On a Linux
  runner without the packages: `cargo check -p dictate` / `clippy` with stub pkg-config files and
  `cmake` (`docs/linux.md`, "Checking without a desktop").
- Platform-free Linux logic (key codes, grabs, the autostart entry, locale, os-release) goes into
  `core/src/linux.rs` with unit tests; `src/platform/linux/` only talks to the system.
- Features `vulkan` and `cuda` work for Linux too; `directml` is Windows only.
- Say in the PR or commit that Linux changes are compile-checked only, until someone has run them.

## Rules

- **This repository is public**: every commit, message, issue and PR is visible to everyone, for
  good. Nothing internal of any team goes here, the maintainers' own included: no company names
  in features, no internal hosts, tools, customers, people, words or plans. What a team needs
  goes into its profile (or brand folder), kept in its own private team repo (`docs/teams.md`); a
  new kind of setting becomes a profile key here with a neutral default, the team's value stays in
  its repo. When you work for such a team, its repo's `AGENTS.md` has the rest.
- **Changes come as pull requests**: work on a branch (in a fork without write access) and open a
  pull request against `main`; never commit or push to `main` directly. Only the maintainers push
  to `main`, and only when their own instructions say so (their team repo's `AGENTS.md`). Say in the
  pull request what you ran and what you could not run (README, "Contributing").
- Other agents and services plug in (`agents/`, the profile), they are not built into `src/`.
- Nothing team-specific in the code: names, hosts, tools, report targets, default words go into a
  profile (keys in `core/src/profile.rs`; the built-in `profiles/default.toml` stays neutral, a test
  checks), bundle id, identity and icon into a brand folder. No real customer, contact or deal data
  in tests, mocks or fixtures: invent names (`example.com`).
- Keep platform-specific code in `src/platform/` (`macos/`, `windows/`, one interface in
  `platform/mod.rs`); `core/` stays free of OS calls and macOS crates (`trigger.rs`, `hook.rs`
  included) so it builds and tests on Linux. New platform-free logic goes into the core; the app only
  re-exports it.
- Never commit model files or recordings of people; models live in the data folder. The synthetic
  eval clips in `eval/clips/` (macOS voices) are the one exception.
- Do not start the live mode (`dictate` without a subcommand) from an unattended agent: it asks for
  Accessibility and the microphone on the user's screen.
- `vendor/parakeet-rs` is crates.io parakeet-rs 0.4.4 plus one hook (`DICTATE-PATCH.md`); change it only
  there and keep that file current. Third-party code or a new model: add it to `NOTICE.md`.
