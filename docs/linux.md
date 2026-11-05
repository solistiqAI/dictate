# Dictate on Linux (beta)

**Status (2026-11-04): compiles, has never run on a Linux desktop.** The app crate builds for
Linux (checked on an x86_64 runner: `cargo check` and `clippy -D warnings` of both crates; the
unit tests of `dictate-core`, including the Linux key mapping). Its own unit tests and a linked
binary need the GTK/WebKitGTK packages: `scripts/check-linux.sh` builds and tests both crates in a
container that has them. Nobody has started the app on a real X11 or Wayland desktop yet. Expect
the first run to find bugs in the input, the paste, the overlay and the tray.

## What is there in code

The platform layer (`src/platform/mod.rs`) gives the app one set of names; `src/platform/linux/`
is the Linux side, `core/src/linux.rs` its platform-free part (unit tested on any OS).

| Part | Linux | Where |
|---|---|---|
| Triggers, X11 | XInput2 raw key and button events on the root window (every app, no permission), fed into the same `Hook` the Windows build uses (hold, toggle, hybrid, another key within 1 s discards, Esc cancels). Key and mouse triggers (F13, combos, side buttons) are grabbed with `XGrabKey`/`XGrabButton`, so they do not reach the app; Esc is grabbed only while a recording or a voice runs. A lone modifier (right Alt) is never grabbed: it passes, as on the Mac. libX11/libXi/libXtst are loaded at run time (`x11-dl`). | `platform/linux/x11.rs`, `core/src/linux.rs`, `core/src/hook.rs` |
| Triggers, Wayland | `/dev/input/event*` read directly (keyboards and mice as the kernel sends them; new devices are picked up every 3 s). Needs the user in the `input` group. Nothing is swallowed: F13 or a side button also reaches the focused app. | `platform/linux/evdev.rs` |
| Which one | `XDG_SESSION_TYPE` (`x11` / `wayland`), else `WAYLAND_DISPLAY` / `DISPLAY`; `DICTATE_INPUT=x11` or `evdev` overrides | `core/src/linux.rs` (`session`) |
| Default triggers | `right-alt` and `f13`, hybrid (no Fn key), as on Windows | `core/src/config.rs` |
| Key names | evdev codes (X11: keycode − 8) → the same names as everywhere: `alt`/`option` = Alt, `cmd`/`command` = Super, `ctrl`, `shift`; `right-alt` (AltGr), `right-ctrl`, `right-shift`, `right-cmd` alone; letters by position (US names, as on the Mac) | `core/src/linux.rs` |
| Mouse | middle = `mouse2`, back (X11 button 8, `BTN_SIDE`) = `mouse3`, forward (9, `BTN_EXTRA`) = `mouse4` | |
| Paste | the clipboard (arboard: X11, or Wayland's data-control protocol where the compositor has it), then Ctrl+V: XTest on X11 (keycodes from the layout's keysyms); on Wayland `ydotool key` (any compositor, needs `ydotoold` running and access to `/dev/uinput`), else `wtype` (wlroots compositors such as Sway or Hyprland; KDE partly; not GNOME). Waits ≤ 1 s for Alt/Super/Shift to be let go. The old clipboard is put back. | `platform/linux/mod.rs` |
| No way to paste | the text stays on the clipboard and the card keeps it with "Kopieren" / "Schließen" (every platform now shows the card when a paste fails) | `app.rs` (`deliver`) |
| Focus check | none (no AT-SPI yet): always pastes | |
| Selected text (read aloud, talk it over) | the PRIMARY selection (what a middle click pastes): no Ctrl+C needed | |
| Recognition | Parakeet Ultra on the CPU through ONNX Runtime, loaded at run time: `ORT_DYLIB_PATH` when set, else `onnxruntime/lib/libonnxruntime.so` in the data folder (`dictate model download onnxruntime`), else `libonnxruntime.so` next to the executable, else the system's library path (as `dictate-core`); default model `parakeet-ultra` (2.5 GB, `dictate model download`). `--features cuda` with ONNX Runtime's GPU build puts it on an NVIDIA GPU. No Neural Engine model is offered. | `core/src/asr.rs`, `core/src/onnxruntime.rs` |
| Korrigieren / Formatieren | Gemma 4 E4B through llama.cpp on the CPU; `--features vulkan` or `cuda` for the GPU | `Cargo.toml` |
| Sounds | `paplay` at the volume (PulseAudio, PipeWire's pulse server), else `pw-play`, else `aplay`; mute while recording: `pactl set-sink-mute @DEFAULT_SINK@` | |
| Pausing media | `playerctl` (MPRIS) when installed | `platform/linux/media.rs` |
| Start at login | `~/.config/autostart/dictate.desktop` (XDG autostart); `ORT_DYLIB_PATH` goes into its `Exec` line when set (not needed for the runtime in the data folder) | `core/src/linux.rs` (`autostart_entry`) |
| Overlay | a small dark GTK window at the bottom of the primary monitor: level bars or a status, the text (struck-through words included), the buttons. X11: a popup (override-redirect), always on top, never takes the focus. Wayland: an undecorated window that asks to stay above and not take the focus; the compositor decides where it goes (often the middle) and may ignore both wishes. | `platform/linux/overlay.rs` |
| Tray, window, settings page | tao + tray-icon (libayatana-appindicator) + wry (WebKitGTK, in the window's GTK box so it works on X11 and Wayland) | `app.rs`, `window.rs` |
| No tray on the desktop | at start Dictate asks the session bus (GIO, for up to 5 s, as the panel may start after it at login) whether `org.kde.StatusNotifierWatcher` is there with `IsStatusNotifierHostRegistered`, else (X11) whether an XEmbed tray owns `_NET_SYSTEM_TRAY_S<n>`. Neither: the settings window opens, a banner on top of its pages (`PageState.no_tray`) and the log say the tray is missing, and closing the window quits Dictate (it asks again first: an extension turned on meanwhile keeps it running in the tray) | `core/src/linux.rs` (`Tray`), `platform/linux/mod.rs` (`tray`), `app.rs` |
| Log | started without a terminal (autostart), stderr goes to `$XDG_DATA_HOME/dictate/dictate.log` | `main.rs` |
| Souffleur "Andere" | the default sink's monitor through `parec --device=@DEFAULT_MONITOR@` (PulseAudio and PipeWire), else `pw-record` capturing the sink; no permission | `platform/linux/system_audio.rs` |
| Reading aloud | `espeak-ng` (or `espeak`) in the UI language; the Mac's voice names are ignored | `platform/linux/mod.rs` (`say`) |
| Claude Code, agent plugins | `~/.local/bin/claude`, the PATH (as on the Mac); plugins in the data folder's `agents/` | `core/src/locate.rs` |
| Data | `$XDG_DATA_HOME/dictate` (`~/.local/share/dictate`): models, history, log, `config.toml` | `core/src/asr.rs` |
| OS in the usage report | `PRETTY_NAME` from `/etc/os-release` and the kernel release | `core/src/linux.rs` (`os_name`) |
| UI language | `LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, `LANG` (as gettext reads them) | `core/src/linux.rs` |

## What is missing

- **Never run.** No grab, raw event, paste, overlay, tray or WebKitGTK window has been seen on a
  real desktop.
- Learning from corrections (`Field::focused` is `None`; AT-SPI's Text interface would do it) and
  the focus check ("Kein Textfeld"; AT-SPI's roles would do it).
- Apple's language model, PocketTTS, the bar at the bottom, the shortcuts beside the pill.
- The page and messages still say ⌥/⌘ and "Bedienungshilfen" (the menu's status line says
  "Kein Zugriff auf die Tastatur (Gruppe input)" on Linux); "open privacy settings" only logs what
  to do.
- The Souffleur's panel opens where the window manager puts it (no "screen with the mouse").
- No portal support yet (GlobalShortcuts, RemoteDesktop), so on Wayland Dictate needs the `input`
  group (and `ydotool` for pasting): see below.
- Packages (AppImage, Flatpak, .deb), an icon in the menus, CI on a Linux runner.
- On X11, while a grabbed trigger key (F13) is held, other keys go to Dictate, not the app (X11's
  passive grab): a combo pressed meanwhile is not typed.

## Wayland: why `/dev/input` and `ydotool`

Wayland gives a normal app no global keys and no synthetic input, by design. The ways around it,
weighed for a first cut:

| Option | For the trigger | For pasting | Catch |
|---|---|---|---|
| **`/dev/input` (chosen)** | any key or button, every compositor, lone modifiers too (right Alt) | – | the user joins the `input` group, which lets every program they run read every key (a keylogger's rights); nothing can be swallowed |
| XDG GlobalShortcuts portal | GNOME 48+, KDE 5.27+: shortcuts the user confirms in a system dialog | – | key combos only (no lone right Alt, no mouse buttons), press and release not on every desktop, D-Bus code (zbus) and a dialog flow; the cleaner long-term way |
| `ydotool` (chosen, first) | – | Ctrl+V through `/dev/uinput`, every compositor | `ydotoold` must run (systemd user service) with access to `/dev/uinput` |
| `wtype` (chosen, second) | – | virtual-keyboard protocol, no daemon | wlroots compositors (Sway, Hyprland); not GNOME |
| RemoteDesktop portal | – | any compositor that has it, through a consent dialog | a dialog per session (or a restore token), D-Bus code; the cleaner long-term way |
| copy + note (fallback) | – | always | the user presses Ctrl+V |

So the first cut works everywhere once the user is in `input` and has `ydotool`, and degrades to
"copied, the card keeps it" without them. Next steps once someone has a Wayland desktop to try on:
the GlobalShortcuts portal for combos (no `input` group) and the RemoteDesktop portal for pasting.

## Build on a Linux PC

1. Packages (Debian/Ubuntu names): `sudo apt install build-essential pkg-config cmake clang
   libclang-dev libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev libasound2-dev
   libxdo-dev libdbus-1-dev` (Fedora: `gtk3-devel webkit2gtk4.1-devel libayatana-appindicator-gtk3-devel
   alsa-lib-devel libxdo-devel dbus-devel cmake clang-devel`). Rust via `rustup`, Node.js for the
   page. At run time: `libxtst6` and `libxi6` (X11), `pulseaudio-utils` (`paplay`, `pactl`, `parec`),
   optionally `playerctl`, `espeak-ng`, `ydotool`/`wtype` (Wayland).
2. `cd ui && npm ci && npm run build && cd ..` (the page is embedded at build time).
3. `cargo build --release` → `target/release/dictate`.
4. ONNX Runtime 1.31 (not linked at build time): `dictate model download onnxruntime` fetches
   the release for your CPU (x64 or aarch64, 11 MB, sha256-checked) from GitHub into the data
   folder (`~/.local/share/dictate/onnxruntime/`), where the app and `dictate-core` find it, also
   when started at login. Your own build instead (e.g. the `-gpu-` one for `--features cuda`, with
   CUDA 12 and cuDNN 9 installed): `export ORT_DYLIB_PATH=…/lib/libonnxruntime.so` (in
   `~/.profile` too, for the session), or put `libonnxruntime.so` next to `dictate`.
5. `dictate model download` (Parakeet Ultra) and `dictate model download gemma-4-e4b`.
6. `dictate mic` checks the microphone, `dictate transcribe x.wav` the model, then `dictate` starts
   the tray app.
7. Wayland only: `sudo usermod -aG input $USER` and log in again; `ydotool` with its daemon running
   (`systemctl --user enable --now ydotool` where the package ships the unit), or `wtype` on wlroots.
8. GNOME shows tray icons only with the AppIndicator extension (Ubuntu has it; elsewhere "AppIndicator
   and KStatusNotifierItem Support"). Without a tray Dictate opens its settings window at start
   instead (settings, history and dictionary are its pages): closing that window quits Dictate,
   `dictate` starts it again.

## What to try first on a real desktop

On X11 (e.g. Xubuntu, or GNOME/KDE on Xorg):

1. `cargo build --release` at all (packages, cmake, llama.cpp), then `--features vulkan`.
2. `dictate mic`; `dictate transcribe` on a German WAV; the time per audio second on the CPU.
3. `dictate correct "wir treffen uns um drei, nein, um vier"` with Gemma on the CPU vs. Vulkan.
4. Live mode: right Alt hold and tap; AltGr+Q (@ on a German keyboard) types @ and does not leave a
   recording; F13 or a mouse side button (the browser must not go back); Esc cancels and does not
   reach the app; `shift+ctrl+5` as a combo.
5. Paste into gedit/Kate, a browser text field, VS Code, a terminal (Ctrl+V does not paste in most
   terminals: expected to fail there; Ctrl+Shift+V would), LibreOffice; with a German and a Dvorak
   layout.
6. The overlay: bottom centre, on top, never takes the focus, "Kopieren" / "Schließen" clickable.
7. Tray menu, the settings window (WebKitGTK), "Aufnehmen" of a new trigger, start at login
   (`~/.config/autostart/dictate.desktop`), mute while recording, media pausing with `playerctl`.
8. Souffleur with a video playing: "Andere" lines appear (`parec`), the level moves.

On Wayland (GNOME, KDE, Sway):

1. Without the `input` group: the tray says "Kein Zugriff auf die Tastatur (Gruppe input)"; with it,
   right Alt and F13 work (F13 also reaches the app).
2. Paste with `ydotool` (daemon running), with `wtype` on Sway, and with neither: the text must stay
   on the clipboard and in the card.
3. Where the overlay appears and whether it stays on top and leaves the focus alone (per compositor).
4. The settings window and the tray under GNOME (extension) and KDE; GNOME without the extension:
   the window opens by itself with the banner on top, closing it quits (`pgrep dictate` is empty).

## Checking without a desktop (agents)

- `scripts/check-linux.sh` (Docker): `cargo check`, `clippy -D warnings` and `cargo test` of both
  crates in `rust:1.99` with the packages above. Runs on a Mac with Docker (arm64 Linux).
- On a Linux runner without the `-dev` packages, `cargo check` / `clippy` (no linking) still work
  with empty pkg-config files for the GTK, WebKitGTK, ALSA and D-Bus libraries
  (`PKG_CONFIG_PATH` at a folder of `.pc` files with `Version: 99` and no `Libs`), plus `cmake`
  (`uv tool install cmake`) and, where bindgen finds libclang but not its headers,
  `BINDGEN_EXTRA_CLANG_ARGS=-I/usr/lib/gcc/x86_64-linux-gnu/<n>/include`. The app's tests need
  the real libraries to link.

## Proposed CI (not added: `.github/workflows` is the owner's)

```yaml
# .github/workflows/linux.yml
name: Linux build (beta)
on:
  push:
    branches: [main]
  pull_request:
jobs:
  build:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - run: >
          sudo apt-get update && sudo apt-get install -y libgtk-3-dev libwebkit2gtk-4.1-dev
          libayatana-appindicator3-dev libasound2-dev libxdo-dev libdbus-1-dev cmake clang libclang-dev
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
      - run: cargo build --release
      - run: cargo test --release
      - run: cargo clippy --release --all-targets -- -D warnings
      - uses: actions/upload-artifact@v4
        with:
          name: dictate-linux-x64
          path: target/release/dictate
```
