# Dictate on Android (beta)

**Status (2026-11-05): written and partly checked, never built into an APK, never run on a
phone.** The Rust side is tested on a Linux host, end to end through JNI with the real model, and
cross-checked for `aarch64-linux-android`. The Kotlin side is written. Its non-Compose half compiles
against Android's API 35, and its plain unit tests pass. The Gradle build, the Compose screens, lint
and the JVM screenshots have not run yet, because the machine this was written on could not reach
Google's Maven repository or download the Android SDK (details at the end). Expect the first
`./gradlew assembleDebug` to need small fixes.

Two things, both on the phone, nothing sent anywhere (the only network access is the model
download):

1. **A voice keyboard.** An Android keyboard whose main element is one big microphone button: hold
   it and talk (or tap, talk, tap). The text goes in at the cursor, in any app. Next to the button:
   next keyboard (long press: the keyboard list), backspace, space, enter.
2. **Transcribing voice messages.** Dictate is a share target for audio: in WhatsApp (or Signal,
   Telegram, …) long-press a voice message, Share, Dictate. It decodes the message on the phone
   (Ogg/Opus, AAC, MP3, WAV, whatever the phone's decoders take), transcribes it and shows the text
   with Copy and Share. Several messages at once give one text each, in order.

The app screen sets it up: microphone permission, the model download (670 MB, size shown first,
a hint on mobile data), turning the keyboard on (a jump to the keyboard settings), a field to try
the keyboard, the recent transcripts (kept on the phone only; not from password fields), and the
dictionary.

## How it is built

```
android/
  app/                 Kotlin + Jetpack Compose, package app.dictate, minSdk 29
    DictateKeyboard    InputMethodService + KeyboardView (plain views), Recorder (AudioRecord 16 kHz)
    ShareActivity      ACTION_SEND / SEND_MULTIPLE audio/* → AudioDecoder (MediaExtractor/MediaCodec)
    MainActivity       setup, try field, recent transcripts, dictionary (Compose, ui/Screens.kt)
    Engine, Native     one background thread, JNI calls into …
  rust/                dictate-android: a cdylib around dictate-core (jni crate)
```

- **Same recognition as the desktop.** `android/rust` (crate `dictate-android`) wraps
  `dictate-core`: `Asr` with Parakeet Ultra int8, the dictionary in the decoder (boosting) and on
  the text, replacements, spoken commands ("neue Zeile", …), the VAD trim. Kotlin calls it through
  `app.dictate.Native`. The model is not `Send`, so one Rust thread owns it and the JNI calls hand
  it jobs.
- **ONNX Runtime** comes from Microsoft's `com.microsoft.onnxruntime:onnxruntime-android` AAR
  (`libonnxruntime.so` in the APK). `dictate-core` opens it at run time, as on Linux (`ort`
  feature `load-dynamic`; `cfg(any(target_os = "linux", target_os = "android"))` in
  `core/Cargo.toml`, `core/src/onnxruntime.rs`, `core/src/asr.rs`). `Engine` calls
  `System.loadLibrary("onnxruntime")` before the first model load, so opening it by name finds it.
- **Model:** `parakeet-ultra-int8` (670 MB; the fp32 build is 2.5 GB, too big for phones),
  downloaded by the system's DownloadManager (resumes, shows a notification) into the app's folder
  on the shared storage (`Android/data/app.dictate/files/models/parakeet-ultra-int8/`). The URLs
  come from the core's model list (`Native.modelFiles()`), so desktop and phone fetch the same
  files. Each file arrives as `<name>.part` and is renamed when all are complete.
- **Settings** are the desktop's format: `config.toml` in the app's private folder, with the
  dictionary as `[dictionary] added/removed` over the profile's default words. A team's
  `profile.toml` (default words) and a `replacements.txt` can sit next to it (debug build:
  `adb push profile.toml /data/local/tmp/ && adb shell run-as app.dictate cp
  /data/local/tmp/profile.toml files/`).
- **Long voice messages** go through the model in pieces of at most 60 s, cut at quiet moments
  (`engine::pieces`, which Cohere uses too). One pass over minutes of audio would need more memory
  than a phone gives an app. Messages longer than 15 minutes are cut at 15 minutes.
- **The keyboard is plain Android views**, not Compose: an input method's window has none of the
  lifecycle Compose expects. Hold and tap are told apart by `MicGesture` (a press shorter than
  0.4 s is a tap: keep recording until the next tap).
- **Why not sherpa-onnx:** its Android library runs Parakeet TDT v3 int8 too and would have been
  the quick way, but without our dictionary, boosting, replacements and spoken commands. Since
  `dictate-core` cross-compiles for Android with only a cfg change, the phone runs the same code
  as the desktop.

## Build a debug APK

Needs: a JDK 17 or 21 (Android Studio brings one), the Android SDK with platform 35 and an NDK
r28 or newer, Rust 1.99+ (rustup), and `cargo-ndk`. On a Mac or Linux:

```sh
# Rust for the phone (arm64) and the emulator (x86_64), and cargo-ndk
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk --locked

# The SDK: Android Studio's SDK Manager, or the command-line tools' sdkmanager
#   SDK Platform 35, Build-Tools 35, NDK (Side by side) r28+, Platform-Tools (adb)
sdkmanager "platforms;android-35" "build-tools;35.0.0" "platform-tools" "ndk;28.2.13676358"   # or any r28+ from `sdkmanager --list`
export ANDROID_HOME=~/Library/Android/sdk          # Linux: ~/Android/Sdk
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/<version>

cd android
./gradlew assembleDebug -Pdictate.abis=arm64-v8a   # a phone only; leave -P… out for the emulator too
```

The APK is **`android/app/build/outputs/apk/debug/app-debug.apk`**, signed with the SDK's debug
key. Gradle's `buildRust` task runs `cargo ndk … build --release -p dictate-android` first (a few
minutes the first time) and packs `libdictate_android.so` next to the AAR's `libonnxruntime.so`.
The Rust code is always built optimised: a debug build of the model's code is far too slow.

Android Studio: open the `android/` folder, it finds the Gradle project. Make sure `cargo` is on
the PATH Studio sees (start Studio from a terminal, or add `~/.cargo/bin` to its environment).

## Install and try it

```sh
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
```

(USB debugging on: Settings → About phone → tap "Build number" 7 times → Developer options → USB
debugging. Or copy the APK to the phone and open it, allowing installs from that app.)

1. Open **Dictate**. Tap **Allow** for the microphone.
2. **Download** the model (670 MB; on Wi-Fi). The notification shows the progress; the card fills
   in when it is done.
3. **Open settings** → turn on **Dictate (voice)**. Android warns that a keyboard could collect
   what you type; Dictate keeps transcripts on the phone and sends nothing. Back in the app,
   **Switch keyboard** → Dictate (voice).
4. Tap the **Try it** field, hold the microphone, speak, let go. The first time loads the model
   (a few seconds).
5. In WhatsApp: long-press a voice message → Share → **Transcribe with Dictate**.

To get back to your usual keyboard: the keyboard key at the left (long press: the list of
keyboards).

## Checks

```sh
cargo test --release -p dictate-android                     # the JNI crate on the host, no NDK
cargo test --release -p dictate-android -- --ignored        # + a clip end to end (int8 model and ONNX Runtime:
                                                            #   dictate-core model download parakeet-ultra-int8 / onnxruntime)
cd android
./gradlew testDebugUnitTest lintDebug                       # unit tests and lint (no Rust, no NDK needed)
./gradlew recordRoborazziDebug                              # the screens as PNGs on the JVM → app/build/outputs/roborazzi/
```

The desktop builds don't need any of this: `android/rust` is a workspace member but not a
default member, so `cargo build`/`test`/`clippy` at the root skip it, and on the host it builds
without the NDK.

## What was tried, and where

| What | Where | Result |
|---|---|---|
| `dictate-android` unit tests (model files, dictionary round trip, resampling, missing model) | Linux x86_64 | pass |
| A clip through `Dictation` (int8 model, ONNX Runtime 1.31) | Linux x86_64 | pass, word for word |
| `Native.kt` on a desktop JVM against the host `libdictate_android.so`: every JNI function, 16 and 48 kHz input, errors as exceptions | Linux x86_64, JDK 21 | pass (3.6 s of speech in 0.11 s) |
| `cargo clippy --target aarch64-linux-android -p dictate-android -p dictate-core -- -D warnings` | Linux, std built from source (`-Zbuild-std`), C parts compiled with zig instead of the NDK | clean; nothing linked |
| The Kotlin outside Compose (keyboard, recorder, decoder, download, engine, share pipeline) | kotlinc 2.4 against API 35's `android.jar` | compiles |
| Plain unit tests (`TypingTest`, `PcmTest`, `TranscriptLogTest`) | kotlinc + kotlin-test on the JVM | pass |
| Gradle build, Compose screens, lint, Robolectric/Roborazzi screenshots | none yet | not run (no Google Maven, no SDK) |
| On a phone or an emulator | none yet | not run |

## Not yet / next

- [ ] First `./gradlew assembleDebug`, `testDebugUnitTest`, `lintDebug`, `recordRoborazziDebug` on a
      machine with the SDK, and fixes from it.
- [ ] First run on a phone: model load time and memory with int8 on a phone CPU, transcription
      speed, the keyboard in a few apps (WhatsApp, Gmail, a browser), a WhatsApp voice note.
- [ ] A release build: signing, R8, a smaller APK (the AAR's Java part is unused).
- [ ] Live text while speaking (the desktop's 0.6 s live pass); text stays empty until you let go
      for now.
- [ ] Settings on the phone beyond the dictionary (spoken commands on/off, boost weight), the
      replacements list.
- [ ] Unloading the model when Android asks for memory (`onTrimMemory`).
- [ ] iOS (separate issue).
