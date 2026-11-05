# Dictate on Windows (beta)

**Status (2026-11-03): compiles, has never run.** The app crate builds for `x86_64-pc-windows-msvc`
(checked from a Mac with `scripts/check-windows.sh`: `cargo xwin check` and `clippy`, no linking).
Nobody has started it on a Windows PC yet. Expect the first run on real hardware to find bugs in the
hooks, the paste, the overlay and the build itself.

## What is there in code

The platform layer (`src/platform/mod.rs`) gives the app one set of names; `src/platform/macos/` is
the Mac app as before, `src/platform/windows/` the Windows side:

| Part | Windows | Where |
|---|---|---|
| Triggers | `WH_KEYBOARD_LL` / `WH_MOUSE_LL` hooks on the main thread, same rules as the Mac (hold, toggle, hybrid, another key within 1 s discards, Esc cancels). Windows keys are translated to the Mac key codes the triggers use, so `config.toml` is the same. Unit tested on any OS. | `platform/windows/mod.rs`, `core/src/hook.rs` |
| Default triggers | `right-alt` and `f13`, hybrid (no Fn key on Windows). A right-Alt or Windows-key trigger sends an unassigned key (0xE8) when pressed, so letting go does not open a menu or Start. | `core/src/config.rs` |
| Modifier names | `alt`/`option` = Alt, `cmd`/`command` = Windows key, `ctrl`, `shift`; `right-alt`, `right-ctrl`, `right-shift`, `right-cmd` alone | `core/src/hook.rs` |
| Mouse | middle = `mouse2`, back (XBUTTON1) = `mouse3`, forward (XBUTTON2) = `mouse4` | |
| Paste | clipboard + `SendInput` Ctrl+V, old clipboard put back; waits ≤ 1 s for Alt/Win/Shift to be released | |
| Focus check | UI Automation's focused element: lists, buttons, menus etc. = "no text field" (text stays in the overlay), Edit = paste, anything else pastes too | |
| Recognition | Parakeet Ultra on the CPU (ONNX Runtime, as `dictate-core`); default model `parakeet-ultra` (2.5 GB, `dictate model download`). Features `cuda` / `directml` put it on the GPU (`DICTATE_CPU=1` compares with the CPU). | `core/src/asr.rs` |
| Korrigieren / Formatieren | Gemma 4 E4B through llama.cpp on the CPU; `vulkan` or `cuda` feature for the GPU | `Cargo.toml` |
| Sounds | the synthesised cues via `PlaySound`, scaled to the volume; "mute while recording" via the default output's `IAudioEndpointVolume` | |
| Start at login | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value `Dictate` | |
| Overlay | a plain dark topmost, non-activating layered window at the bottom of the primary screen (GDI text, level bars, clickable button labels) | `platform/windows/overlay.rs` |
| Tray, window, settings page | tao + tray-icon + wry (WebView2; the page is served as `http://dictate.localhost/`) | `app.rs`, `window.rs` |
| Log | started from Explorer/Run key, the console window is closed and the log goes to `%APPDATA%\dictate\dictate.log` | `main.rs` |
| Souffleur "Andere" | WASAPI loopback of the default output device (cpal opens an input stream on it), no permission | `platform/windows/system_audio.rs`, `audio.rs` |
| Claude Code, agent plugins | `%USERPROFILE%\.local\bin\claude.exe`, npm's `%APPDATA%\npm\claude.cmd`, then the PATH with PATHEXT (`.exe`/`.cmd`/`.bat`/`.com`, never npm's extensionless shell script); `DICTATE_CLAUDE` overrides. Agent plugins: `agents\<name>.exe`, `.cmd` or `.bat` in the data folder (the shell scripts in `agents/` do not run on Windows). The places are a pure function, unit tested on any OS | `core/src/locate.rs` |
| Model list | `dictate model list`, the settings' model choice: the Neural Engine model only on macOS; `dictate model download parakeet-ultra-coreml` says it needs a Mac | `core/src/asr.rs` (`available`) |
| Unchanged and portable | dictionary, replacements, history, local server (`127.0.0.1:47619`), the Souffleur's agent part, report text | |

## What is missing

- **Never run.** No hook, paste, overlay or WebView2 behaviour has been seen on a real PC.
- Apple's language model (macOS only; the Neural Engine model is no longer offered off macOS).
- Reading aloud and the spoken conversation: no system voice yet (SAPI / `System.Speech`); PocketTTS
  is macOS only.
- Learning from corrections (`Field::focused` is `None`; UI Automation's TextPattern would do it).
- Pausing media (system media transport controls), the bar at the bottom, the overlay's
  struck-through words and the shortcuts beside the pill, rounded corners, per-monitor DPI.
- The UI and messages still say ⌥/⌘ and "Bedienungshilfen"; the console window appears briefly when
  started from Explorer (a `windows_subsystem = "windows"` build would need its own CLI story).
- Installer (MSI/NSIS), Authenticode signing, an app icon (`.ico`), CI on a Windows runner.
- Pasting into apps that run as administrator (Windows blocks input from a normal process into them).

## Build on a Windows PC

1. Visual Studio 2022 Build Tools with "Desktop development with C++" (MSVC, Windows SDK), CMake
   (comes with it, or `winget install Kitware.CMake`), Git, Rust (`rustup`, target
   `x86_64-pc-windows-msvc`), Node.js for the page.
2. `cd ui && npm ci && npm run build && cd ..` (the page is embedded at build time).
3. CPU build: `cargo build --release` → `target\release\dictate.exe` (ONNX Runtime is downloaded by
   the `ort` crate at build time; keep `onnxruntime*.dll` next to the exe if the build puts one there).
4. GPU, NVIDIA:
   - Gemma on Vulkan (any vendor, no CUDA install): install the Vulkan SDK (LunarG), then
     `cargo build --release --features vulkan`.
   - Gemma and Parakeet on CUDA: CUDA Toolkit 12.x (nvcc for llama.cpp) and cuDNN 9 (ONNX Runtime's
     CUDA build needs its DLLs on the `PATH` at run time), then `cargo build --release --features cuda`.
   - Parakeet on DirectML (any DirectX 12 GPU): `--features directml`.
   Features combine, e.g. `--features vulkan,directml`.
5. `dictate model download` (Parakeet Ultra, CPU/GPU build) and `dictate model download gemma-4-e4b`.
6. `dictate mic` checks the microphone, `dictate transcribe x.wav` the model, then `dictate` starts
   the tray app.

## What to try first on a real PC (with an NVIDIA GPU)

1. `cargo build --release` at all (MSVC, cmake, ONNX Runtime download, linking), then `--features vulkan`
   and `--features cuda`.
2. `dictate mic`, `dictate transcribe` on a German WAV; the time per audio second on CPU vs.
   `--features cuda` / `directml` (and `DICTATE_CPU=1` to compare in the same build).
3. `dictate correct "wir treffen uns um drei, nein, um vier"` with Gemma on CPU vs. Vulkan vs. CUDA.
4. Live mode: right Alt hold and tap, F13 or a mouse side button; AltGr+Q (@) on a German keyboard
   must type @ and not leave a recording; Esc cancels; releasing right Alt opens no menu.
5. Paste into Notepad, Word, a browser text field, VS Code, Windows Terminal, an admin console
   (expected to fail); the "Kein Textfeld" overlay on the desktop or in Explorer's file list.
6. The overlay: on top, never takes the focus, "Kopieren" / "Schließen" clickable; high-DPI screens.
7. Settings window (WebView2), "Aufnehmen" of a new trigger, start at login, mute while recording.
8. Souffleur with a call or a video playing: "Andere" lines appear (loopback), the level moves; with
   headphones and with speakers.

## Checking from a Mac (agents)

`scripts/check-windows.sh` (`--features directml` works too). Once per Mac: `rustup target add
x86_64-pc-windows-msvc`, `cargo install --locked cargo-xwin`, LLVM with `clang-cl` (`brew install
llvm`), `uv tool install ninja` (cmake as for the Mac build). The first run fetches the MSVC CRT
and Windows SDK (~1 GB, cached) and ONNX Runtime for Windows. `vulkan` and `cuda` cannot be checked
from a Mac (llama.cpp's build needs the Vulkan SDK / nvcc).

## Proposed CI (not added: `.github/workflows` is the owner's)

```yaml
# .github/workflows/windows.yml
name: Windows build (beta)
on:
  push:
    branches: [main]
  pull_request:
jobs:
  build:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: actions/setup-node@v4
        with:
          node-version: 22
      - uses: Swatinem/rust-cache@v2
      - name: Page
        working-directory: ui
        run: npm ci && npm run build
      - name: Build (CPU)
        run: cargo build --release
      - name: Tests
        run: cargo test --release
      - name: Clippy
        run: cargo clippy --release --all-targets -- -D warnings
      - uses: actions/upload-artifact@v4
        with:
          name: dictate-windows-x64
          path: target/release/dictate.exe
```

A Vulkan job would add the Vulkan SDK (e.g. `humbletim/install-vulkan-sdk@v1`) and
`--features vulkan`; CUDA (`Jimver/cuda-toolkit`) is slow to install and better left to a release job.
