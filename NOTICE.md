# Third-party code and models

Dictate itself is MIT licensed (`LICENSE`). It includes or downloads the following.

## Code in this repository

| What | Where | Licence |
|---|---|---|
| [parakeet-rs](https://github.com/altunenes/parakeet-rs) 0.4.4, with one hook added (`DICTATE-PATCH.md`) | `vendor/parakeet-rs/` | MIT OR Apache-2.0 (`vendor/parakeet-rs/LICENSE`) |
| [mediaremote-adapter](https://github.com/ungive/mediaremote-adapter) | `vendor/mediaremote-adapter/` | BSD-3-Clause (`vendor/mediaremote-adapter/LICENSE`) |

## Built in at build time

| What | Used for | Licence |
|---|---|---|
| [FluidAudio](https://github.com/FluidInference/FluidAudio) (Swift package, pinned in `ane/Package.swift`) | Parakeet on the Neural Engine, PocketTTS, system audio helper (macOS) | Apache-2.0 |
| [llama.cpp](https://github.com/ggml-org/llama.cpp) via `llama-cpp-2` | Gemma for "Korrigieren"/"Formatieren" | MIT |
| [ONNX Runtime](https://github.com/microsoft/onnxruntime) via `ort` | speech models on the CPU/GPU | MIT |
| Rust crates (see `Cargo.lock`; `cargo license` lists them) and npm packages for the page (`ui/package-lock.json`) | | their own, MIT/Apache-2.0 mostly |
| [ONNX Runtime for Android](https://central.sonatype.com/artifact/com.microsoft.onnxruntime/onnxruntime-android) (`onnxruntime-android` AAR) | the Android app's speech model | MIT |
| [Jetpack Compose](https://developer.android.com/jetpack/compose), AndroidX Activity | the Android app's screens | Apache-2.0 |
| [Material icons](https://github.com/google/material-design-icons) (mic, stop, backspace, enter, keyboard, copy) as vector drawables in `android/app/src/main/res/drawable/` | the Android keyboard and screens | Apache-2.0 |
| [Gradle wrapper](https://gradle.org) (`android/gradlew`, `android/gradle/wrapper/`) | building the Android app | Apache-2.0 |
| [Robolectric](https://robolectric.org), [Roborazzi](https://github.com/takahirom/roborazzi), JUnit (tests only) | the Android app's tests and screenshots | MIT, Apache-2.0, EPL-1.0 |

## Models (downloaded on demand, never part of the repository or the app)

| Model | Licence |
|---|---|
| [Parakeet Ultra](https://huggingface.co/moondream/parakeet-ultra) (Moondream), on NVIDIA's [parakeet-tdt-0.6b-v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3); ONNX exports by [altunenes/parakeet-rs](https://huggingface.co/altunenes/parakeet-rs) and [Olicorne](https://huggingface.co/Olicorne/parakeet-tdt-0.6b-v3-ultra-onnx), Core ML build by FluidAudio | CC-BY-4.0 (attribution: NVIDIA, Moondream) |
| [Parakeet CTC 0.6B](https://huggingface.co/nvidia/parakeet-ctc-0.6b) | CC-BY-4.0 |
| [Nemotron 3.5 ASR Streaming](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b), [Nemotron Unified ASR](https://huggingface.co/nvidia/nemotron-labs-unified-asr-0.6b) | OpenMDW 1.1 |
| [Cohere Transcribe](https://huggingface.co/CohereLabs/cohere-transcribe-03-2026), int8 by [onnx-community](https://huggingface.co/onnx-community/cohere-transcribe-03-2026-ONNX) | Apache-2.0 |
| [Gemma 4 E4B](https://huggingface.co/google/gemma-4-E4B-it), GGUF by [lmstudio-community](https://huggingface.co/lmstudio-community/gemma-4-E4B-it-GGUF) | Apache-2.0 |
| [PocketTTS](https://huggingface.co/kyutai/pocket-tts) (Kyutai) voices, fetched by FluidAudio | CC-BY-4.0 |

The eval clips in `eval/clips/` are synthetic speech made with macOS system voices (`say`).

The app's icons and the wordmark at the top of its window carry the SOLISTIQ mark, a trademark of
SOLISTIQ GmbH: `docs/images/logo.png`, `assets/app-icon.svg` and `assets/AppIcon.icns`, the
window's `ui/src/components/Mark.svelte`, and the Android launcher icon
(`android/app/src/main/res/drawable/ic_launcher_foreground.xml`, `ic_launcher_monochrome.xml`). They are not covered by the MIT licence: forks and builds of their
own use their own icons (a brand folder, `docs/teams.md`, or these files replaced).
