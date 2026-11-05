# Architecture

A Cargo workspace of two crates. `dictate` (the root folder) is the macOS app and CLI (and the
Windows and Linux betas); `dictate-core`
(`core/`) is the platform-free part, re-exported by the app under the same module names
(`crate::vocab::…`), which builds and tests on Linux too.

`core/src/` (dictate-core, no macOS):

- `trigger.rs`: trigger parsing and the hold/toggle/hybrid state machine; `hook.rs`: the same
  triggers from Windows' low-level hooks; `linux.rs`: Linux key and button codes (evdev, X11) into
  the same hook, the X11 grabs, the autostart entry, locale and OS name.
- `audio.rs`: WAV/FLAC reading, resampling to 16 kHz (rubato).
- `asr.rs`: the known models, download (curl, resumable), `Asr` (loads whatever family a folder holds), `transcribe_file`.
- `onnxruntime.rs`: where the Linux build loads ONNX Runtime from (`ORT_DYLIB_PATH`, else the data folder, else next to the executable) and `model download onnxruntime` (the pinned GitHub release, sha256-checked).
- `engine.rs`: one engine per model family behind a trait (Parakeet TDT, CTC, Nemotron, Unified,
  Cohere; all parakeet-rs on ONNX Runtime, CPU); decoder dictionary and language are optional per
  family. The Neural Engine family is the app's and plugs in with `engine::register_ane`.
- `config.rs`: `config.toml`; `vad.rs`: silence trimming; `commands.rs`: spoken layout commands.
- `replace.rs`: word replacements; `vocab.rs`: the dictionary (sound-alike correction); `learn.rs`: learning from corrections.
- `boost.rs`: the dictionary as a token automaton for the decoder (word boosting).
- `format.rs`: carrying a model's layout onto the transcript (the model call is the app's `src/format.rs`).
- `stats.rs`, `report.rs`: the anonymous usage statistics and the report (to the profile's target: an issue in a private repo with `gh`, or a URL with curl).
- `profile.rs`: the profile (`profiles/`, see "Profiles"): default words, report target, the Souffleur's tools and context, the conversation's agent, allowed web pages; installed, built in, updated daily.
- `eval.rs` + `scripts/eval.sh` (macOS voices) / `scripts/eval-clips.sh` (pre-rendered clips in
  `eval/clips/`, any OS) + `eval/`: recognition measured on synthetic speech.
- `main.rs`: the `dictate-core` binary: `eval`, `transcribe`, `model list|download` without the app.

`src/` (dictate, macOS and the Windows and Linux betas):

- `main.rs`: CLI, the live loop (event tap thread → recorder/transcriber worker).
- `audio.rs`: microphone capture (cpal).
- `app.rs`: menu bar app (tao + tray-icon), worker thread (recording, live text, final text).
- `overlay.rs`, `bar.rs`: the platform-free part of the recorder overlay and the bottom bar.
- `window.rs`: the main window (tao + wry); `protocol.rs`: its messages; `ui/`: the page (Svelte 5, Vite).
- `platform/`: everything from the operating system behind one set of names (`platform/mod.rs`).
  - `macos/mod.rs`: CGEventTap for triggers, paste via ⌘V, focused-text-field check, Accessibility
    prompt, sounds; `macos/overlay.rs`, `macos/bar.rs`: the AppKit panels (objc2); `macos/ane.rs` +
    `ane/` (Swift package, FluidAudio): Parakeet on the Neural Engine in a helper process;
    `macos/pocket.rs`: PocketTTS; `macos/media.rs`: pause/resume "Now Playing" via
    `vendor/mediaremote-adapter` (see its DICTATE-VENDOR.md).
  - `windows/`: the Windows beta (see "Windows (beta)").
  - `linux/`: the Linux beta (`docs/linux.md`): XInput2 (`x11.rs`) or `/dev/input` (`evdev.rs`)
    for the triggers, XTest / `ydotool` / `wtype` for pasting, a GTK overlay.
- `correct.rs`, `llm.rs`: "Korrigieren" with Gemma (llama.cpp, Metal) or Apple's model.
- `voice.rs`, `talk.rs`: reading aloud (`say`) and the spoken conversation (Claude Code, or an agent plugin: `docs/agent-plugins.md`).
- `meeting.rs`: the Souffleur; `history.rs`: the dictation history (`history.jsonl`).

## Recognition: CPU or Neural Engine

The app is Rust (one language for the app, the CLI and the tools, and a path to Windows and Linux);
the Neural Engine is reached through a small Swift helper, since Core ML is Apple's.

CPU or Neural Engine (window → Erkennung → Modell): "Parakeet Ultra · CPU" runs on ONNX Runtime,
"Parakeet Ultra · Neural Engine" (the default since 2026-11-01) is FluidAudio's Core ML build of the same model (int8 encoder,
0.6 GB) in the Swift helper `dictate-ane` (`ane/`, `src/platform/macos/ane.rs`), with FluidAudio's script filter and
its dictionary boosting (a second, small CTC model rescores the transcript, ~100 MB in FluidAudio's
cache folder on first use; thresholds for dictation from FluidAudio #967: cbw 0, similarity 0.7).
ONNX Runtime's own Core ML provider is no way to the Neural Engine: it ran Ultra at half the CPU
speed (dynamic shapes, Core ML falls back).

Measured 2026-11-01, M5 Pro, 450 German TTS clips (50 sentences × 9 macOS voices, 30 sentences with
dictionary words, 1847 s of audio, `boost::corpus` test, `DICTATE_BOOST_MODEL`); WER after the text
dictionary, dictionary words found of 484, time per clip (median 4.1 s of audio), CPU time and peak
memory for all 450:

| | WER | with dict words | without | dict words | per clip | CPU time | memory |
|---|---:|---:|---:|---:|---:|---:|---:|
| CPU, Ultra fp32 + decoder dictionary (default) | 9.0 % | 13.2 % | 2.5 % | 318 | 74 ms | 104 s | 3.0 GB |
| CPU, Ultra fp32, no decoder dictionary | 9.4 % | 13.7 % | 2.5 % | 305 | 74 ms | 104 s | 3.0 GB |
| CPU, Ultra int8 + decoder dictionary | 9.5 % | 13.8 % | 2.7 % | 307 | 84 ms | 125 s | 1.3 GB |
| Neural Engine, no dictionary boosting | 9.5 % | 14.1 % | 2.4 % | 301 | 42 ms | 19 s | 0.25 GB |
| Neural Engine + FluidAudio dictionary boosting | **8.8 %** | **12.9 %** | 2.4 % | **337** | 127 ms | 54 s | 0.26 GB |

Memory is resident size (`ps`); the helper's physical footprint is 47 MB plus the memory-mapped model
(0.6 GB) and the system's `aned`. CPU time is a stand-in for power (Neural Engine power needs
`powermetrics`, i.e. sudo). The CTC rescoring costs the extra 85 ms per clip on the Neural Engine path. Since then the
live text skips it (43 instead of ~135 ms per pass), the final text keeps it. FluidAudio picks its
thresholds by dictionary size: with a 6-word test dictionary it once replaced "Nacht drei Tickets im"
with a dictionary word; with 32 dictionary words none of the 450 clips showed that.

## Models

The fp32 ONNX files come from [altunenes/parakeet-rs](https://huggingface.co/altunenes/parakeet-rs),
the int8 ones from [Olicorne/parakeet-tdt-0.6b-v3-ultra-onnx](https://huggingface.co/Olicorne/parakeet-tdt-0.6b-v3-ultra-onnx)
(`int8/`, MatMulNBits 8-bit, `<unk>` banned; same graph and file names as the istupakov V3 export,
which parakeet-rs 0.4.4 loads as `encoder-model.int8.onnx` / `decoder_joint-model.int8.onnx` when
no fp32 file is in the folder).

int8 vs fp32 (2026-11-01, M-series CPU, 14 TTS sentences German/English + one real 41 s recording):
same text on all TTS sentences; on the real recording 5 of 120 words differ from fp32 ("Merge gen"
for "Mergein", "wohl" for "wo"), and long audio is not faster (0.79 vs 0.59 s for 41 s). The gain
is size: 670 MB download instead of 2.5 GB, 1.6 instead of 3.6 GB in memory. The dynamic-quantization
int8 of [Masterx/parakeet-tdt-0.6b-ultra-onnx](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx)
was worse (9 words differ). fp32 stays the default.
Parakeet Ultra is [Moondream's](https://huggingface.co/moondream/parakeet-ultra), built on NVIDIA's
[parakeet-tdt-0.6b-v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3); both CC-BY-4.0.
The other families: [Parakeet CTC](https://huggingface.co/nvidia/parakeet-ctc-0.6b) (CC-BY-4.0),
[Nemotron 3.5 Streaming](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b) and
[Nemotron Unified](https://huggingface.co/nvidia/nemotron-labs-unified-asr-0.6b) (OpenMDW 1.1), all in
parakeet-rs's exports on [altunenes/parakeet-rs](https://huggingface.co/altunenes/parakeet-rs);
[Cohere Transcribe](https://huggingface.co/CohereLabs/cohere-transcribe-03-2026) (Apache 2.0), int8
files from [onnx-community](https://huggingface.co/onnx-community/cohere-transcribe-03-2026-ONNX).
