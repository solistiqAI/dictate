# Dictate on Android (beta)

**Status (2026-11-06): built on a Mac, runs on a Galaxy S25 (Android 17)**: shared voice
messages, the floating button and HeliBoard's mic key tried by hand; the automatic WhatsApp
transcripts and the chat strip are built but not yet seen with a real incoming voice message.

Everything runs on the phone and nothing is sent anywhere; the only network access is the model
download, and the **offline build** has no internet permission at all (below).

1. **Dictating into any text field, two ways to start** (the app asks which: *Start recording
   with* floating button, my keyboard's mic key, or both). An app can't add a key to someone
   else's keyboard, so these are the two closest things:
   - **Floating button** (works with any keyboard, Gboard included): an accessibility service shows
     a small round mic button just above the keyboard while a text field has focus. Tap, talk, tap
     again; the text goes into the field. Drag it where you like; it keeps its place relative to
     the keyboard's top.
   - **Your keyboard's mic key**: Dictate is a *voice input method*. Keyboards whose mic key hands
     off to the voice input method open it; it listens at once, stops when you stop talking (or on
     a tap), types the text and switches back to your keyboard. Made for **HeliBoard**.
   - Dictate is also a speech recognizer for the system (`RecognitionService`, and the
     `RECOGNIZE_SPEECH` dialog), for apps that ask Android for voice input.
2. **Transcribing voice messages.** Dictate is a share target for audio: in WhatsApp (or Signal,
   Telegram, …) long-press a voice message, Share, Dictate. It decodes the message on the phone
   (Ogg/Opus, AAC, MP3, WAV, whatever the phone's decoders take), transcribes it and shows the text
   with Copy and Share. Several messages at once give one text each, in order.

The app screen sets it up: microphone permission, the model download (670 MB, size shown first,
a hint on mobile data), how to start a recording and the setup of that (the accessibility service
for the button; the voice input method for the mic key, with which input method the mic key opens
right now), a field to try it, the recent transcripts (kept on the phone only; not from password
fields), the dictionary, and the speed of the last transcription.

## Incoming WhatsApp voice messages, without sharing

Switched on in the app ("Voice messages", off by default), every WhatsApp voice message that comes
in shows up as a notification of Dictate's own: the sender as the title, the text below; a tap
opens the chat (WhatsApp's own tap action), "Copy" copies the text. It also lands in the recent
transcripts. WhatsApp's own transcripts don't offer German on Android, hence this.

How (`VoiceNoteListener`, the pure part in `VoiceNotes` with tests): a `NotificationListenerService`
looks only at WhatsApp's notifications (`com.whatsapp`, `com.whatsapp.w4b`). When a chat's newest
message is a voice message (WhatsApp writes "🎤 Sprachnachricht (0:12)", the microphone and the
duration in any language), it waits up to a minute for the new `.opus` file in WhatsApp's voice
notes folder (`Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Voice Notes/<week>/`) to stop
growing, decodes and transcribes it. Needs notification access and access to all files (the folder
has a `.nomedia`, so the media store doesn't list it); both are "restricted settings" for an app
not from a store (App info → ⋮ → Allow restricted settings). WhatsApp must download voice messages
by itself (its default). Nothing of the notifications is kept or logged but the transcript.

Over an open WhatsApp chat, a dark strip below WhatsApp's header shows that chat's transcribed
voice messages of the last three days (`ChatStrip`, from `VoiceNoteLog`): one line with the newest;
a tap shows them all, each in full (in a group with the sender), a long press on one copies it, ✕
hides the strip until the next voice message of that chat. It needs the floating button's
accessibility service, which then also reads the open chat's name from WhatsApp's header (view id
`conversation_contact_name`, hence `flagReportViewIds`; matched against the notification's chat
name without case, spaces and direction marks). A WhatsApp update that renames that view hides
the strip, nothing else.

## Which keyboards, which way

| Keyboard | Floating button | Its mic key opens Dictate |
|---|---|---|
| **Gboard** | yes | no: Gboard's mic always uses Google's voice typing. Reach the voice keyboard through Gboard's switch-keyboard key (globe / long-press space) or the navigation bar's keyboard icon: it records at once and switches back after typing. |
| **HeliBoard** (made for it) | yes | yes (see below) |
| FlorisBoard | yes | yes: its mic key (`FlorisImeService.switchToVoiceInputMethod`) switches to the **first enabled** input method with a `voice` subtype, so turn off other voice input methods (Google voice typing) in the keyboard list |
| AOSP keyboard and other AOSP-based ones | yes | should: they use the same hand-off as HeliBoard (not tried) |
| Samsung Keyboard | yes | not known yet: whether its voice input setting lists Dictate needs checking on a Galaxy phone |
| Dictate's own keyboard | not shown (it has the big button) | |

How the mic key finds Dictate (from HeliBoard's source, `RichInputMethodManager.updateShortcutIme`):
the keyboard asks `InputMethodManager.getShortcutInputMethodsAndSubtypes()` and switches to the
first answer. Android answers with an *enabled* input method that has a subtype of mode `voice`
(Dictate's `method.xml` has one), but an enabled one that comes with the phone wins over apps
(AOSP `InputMethodManagerService`). So on phones where **Google voice typing** is an enabled
keyboard, turn it off in the keyboard list; the app screen says what the mic key opens right now.
HeliBoard hides its mic key when there is no voice input method at all.

### HeliBoard setup

1. Install HeliBoard (GitHub releases or F-Droid), turn it on and choose it as your keyboard.
2. In Dictate: *Start recording with* → "My keyboard's mic key" (or both) → **Open settings** →
   turn on **Dictate (voice)**. Turn off **Google voice typing** there if it is listed.
3. The app screen should now say "Your keyboard's mic key opens Dictate". In HeliBoard, the mic
   key is on the toolbar or a long press on the comma key, depending on its settings
   (Settings → Toolbar / Keys: "Voice input key").
4. Tap the **Try it** field, tap HeliBoard's mic key, talk, stop talking: the text appears and
   HeliBoard comes back.

### Floating button setup

1. *Start recording with* → "Floating button" (or both) → **Accessibility settings** → Installed
   apps (Downloaded apps) → **Dictate button** → on.
2. Android 13 and later grey that switch out for apps installed from an APK ("restricted
   setting"). Then: **App info** (the button next to it) → ⋮ (top right) → **Allow restricted
   settings** → confirm, and turn the service on again.
3. Tap any text field: the button sits above the keyboard on the right. Drag it elsewhere if it
   covers something. Tap, talk, tap again.

How the text gets into the field: Dictate sets the field's text with the dictation at the
cursor (`ACTION_SET_TEXT`, then the cursor after it). That leaves the clipboard alone. Where a
field doesn't take that, it puts the text on the clipboard and pastes (`ACTION_PASTE`). The text then
stays on the clipboard: an accessibility service may write the clipboard but not read it (only the
current keyboard and the focused app may, AOSP `ClipboardService`), so it can't put back what
was there before. If nothing can be inserted, the text stays on the clipboard with a short
notice.

Recording runs in a foreground service of type `microphone` (notification "Dictate is
listening", with Stop). Starting it from an accessibility service is allowed: the tap comes from
the button, a visible `TYPE_ACCESSIBILITY_OVERLAY` window, which counts as being in the foreground
for starting such a service (AOSP `ActiveServices`: `REASON_UID_VISIBLE`). The visible button is
**not** enough for the microphone itself: recording only starts once the service runs. Started
together, Android 17 on a Galaxy S25 handed the recording silence (`audioserver`: "App op 27
missing, silencing record"), and the model heard nothing. If the service doesn't start within 2 s,
recording starts anyway; when Android silences it, the app says so instead of "Nothing heard".

While you talk, a dark bubble above the button shows the words so far (the newest three lines),
as the desktop's overlay does. The recording so far goes through the model again and again, one
pass at a time, at least 0.6 s apart and twice as long as the last pass took; the text that goes
into the field is still the pass over the whole recording at the end. The voice keyboard shows
the words so far in the field itself, as underlined composing text, replaced by the final text.

When a keyboard's mic key (HeliBoard, FlorisBoard) opens the voice keyboard while the floating
button's accessibility service is on, the voice keyboard hands over at once: the recording runs in
that service, the user's keyboard comes straight back, and the live text shows in the bubble above
it, with the button (red) to stop. A pause of 1.5 s after speech ends it too, and so does the mic
key pressed again. Without the service the voice keyboard records itself, as before.

### The system's voice input

`DictateRecognizer` is an `android.speech.RecognitionService`, and `RecognizeActivity` answers
`RecognizerIntent.ACTION_RECOGNIZE_SPEECH`. Apps that ask Android for speech recognition can get
Dictate when it is chosen as the voice input. That's under Settings → Apps → Default apps →
Digital assistant → Voice input on most phones, and differs by maker. Both listen hands-free
(until the speaker stops) and give one final result; there are no partial results yet.

## How it is built

```
android/
  app/                 Kotlin + Jetpack Compose, package app.dictate, minSdk 29
    DictateKeyboard    voice InputMethodService + KeyboardView (plain views); Listening, Recorder (AudioRecord 16 kHz)
    FloatingMicService AccessibilityService: the floating button (MicButton), MicService (foreground, microphone)
    DictateRecognizer  RecognitionService; RecognizeActivity: RECOGNIZE_SPEECH
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
  0.4 s is a tap: keep recording until the next tap). Hands-free (opened from another keyboard,
  the recognizer) `SilenceStop` ends the recording after 1.5 s of quiet following speech, or after
  8 s without any.
- **Voice messages** are averaged down to 16 kHz while decoding (Opus decodes at 48 kHz), so
  long ones fit in an app's memory.
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
./gradlew assembleOnlineDebug -Pdictate.abis=arm64-v8a    # a phone only; leave -P… out for the emulator too
./gradlew assembleOfflineDebug -Pdictate.abis=arm64-v8a   # the build without internet
```

Two builds of the same app (product flavors, the same package, so one replaces the other):

- **online** (`android/app/build/outputs/apk/online/debug/app-online-debug.apk`): downloads the
  speech model itself (the only network access).
- **offline** (`…/apk/offline/debug/app-offline-debug.apk`, version name `…-offline`): its manifest
  removes `INTERNET` and `ACCESS_NETWORK_STATE` (`app/src/offline/AndroidManifest.xml`), so Android
  guarantees it sends nothing; check with `aapt2 dump permissions <apk>`. The model comes from a
  folder on the phone: copy `parakeet-ultra-int8` (the three files; on a desktop `dictate-core model
  download parakeet-ultra-int8` puts them into the models folder) onto the phone, e.g. into
  Download, then "Import model folder" in the app (`ModelImport`). Or straight into place:
  `adb push parakeet-ultra-int8 /sdcard/Android/data/app.dictate/files/models/`.

Both are signed with the SDK's debug key. Gradle's `buildRust` task runs `cargo ndk … build --release -p dictate-android` first (a few
minutes the first time) and packs `libdictate_android.so` next to the AAR's `libonnxruntime.so`.
The Rust code is always built optimised: a debug build of the model's code is far too slow.

Android Studio: open the `android/` folder, it finds the Gradle project. Make sure `cargo` is on
the PATH Studio sees (start Studio from a terminal, or add `~/.cargo/bin` to its environment).

## Install and try it

```sh
adb install -r android/app/build/outputs/apk/online/debug/app-online-debug.apk   # or offline/…
```

(USB debugging on: Settings → About phone → tap "Build number" 7 times → Developer options → USB
debugging. Or copy the APK to the phone and open it, allowing installs from that app.)

1. Open **Dictate**. Tap **Allow** for the microphone.
2. **Download** the model (670 MB; on Wi-Fi). The notification shows the progress; the card fills
   in when it is done.
3. **Start recording with**: floating button, your keyboard's mic key, or both. Then follow the
   card for it ([floating button](#floating-button-setup), [HeliBoard](#heliboard-setup)).
   Android warns when you turn on a keyboard or an accessibility service that it could see what you
   type. Dictate only looks at which field has focus, keeps transcripts on the phone and sends
   nothing.
4. Tap the **Try it** field and start a recording. The first one loads the model (a few seconds).
5. In WhatsApp: long-press a voice message → Share → **Transcribe with Dictate**.

## Measuring the speed on the phone

Every transcription logs its speed:

```sh
adb logcat -s Dictate
# Dictate: model loaded in 2140 ms
# Dictate: transcribed 4.2 s of audio in 850 ms (0.20 x real time)
```

The app screen shows the last one at the bottom ("Last transcription: 4.2 s of audio in 0.85 s").

## Checks

```sh
cargo test --release -p dictate-android                     # the JNI crate on the host, no NDK
cargo test --release -p dictate-android -- --ignored        # + a clip end to end (int8 model and ONNX Runtime:
                                                            #   dictate-core model download parakeet-ultra-int8 / onnxruntime)
cd android
./gradlew testOnlineDebugUnitTest lintOnlineDebug lintOfflineDebug   # unit tests and lint (no Rust, no NDK needed)
./gradlew recordRoborazziOnlineDebug                       # the screens as PNGs on the JVM → app/build/outputs/roborazzi/
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
| `cargo clippy --target aarch64-linux-android` (and `x86_64-linux-android`) `-p dictate-android -p dictate-core -- -D warnings` | Linux, std built from source (`-Zbuild-std`), C parts compiled with zig instead of the NDK | clean; nothing linked |
| The Kotlin outside Compose (keyboard, recorder, decoder, download, engine, share pipeline) | kotlinc 2.4 against API 35's `android.jar` | compiles |
| Plain unit tests (`TypingTest`, `PcmTest`, `TranscriptLogTest`, `ListeningTest`) | kotlinc + kotlin-test on the JVM | pass (14) |
| How HeliBoard finds the voice input method | HeliBoard's source | `getShortcutInputMethodsAndSubtypes()`: a `voice` subtype qualifies |
| Accessibility service → microphone foreground service on Android 14/15; clipboard read | AOSP frameworks/base (`ActiveServices`, `WindowState`, `AccessibilityServiceConnection`, `ClipboardService`) | allowed while the button is visible; clipboard not readable |
| Gradle build, Compose screens, lint, Robolectric/Roborazzi screenshots | none yet | not run (no Google Maven, no SDK) |
| Gradle build, unit tests, lint, Roborazzi | macOS (Apple silicon), JDK 21, SDK 35, NDK r28c | pass (lint: 0 errors) |
| Emulator (Android 15, arm64) | macOS, M5 Pro | the app runs; the model crashes with SIGILL in ONNX Runtime: the emulator claims SVE2/SME2 the VM can't run (emulator only) |
| A shared Opus voice note | Galaxy S25 (Snapdragon 8 Elite), Android 17 | word for word; model load 1.3 s, 7.3 s of speech in 0.27 s |
| Floating button | Galaxy S25, Android 17 | first build: recordings silenced (fixed: recording waits for the microphone service) |

## Not yet / next

- [x] First Gradle build, tests, lint, screenshots (macOS); first run on a phone (Galaxy S25:
      model load 1.3 s, 7.3 s of speech in 0.27 s on the CPU).
- [ ] An incoming WhatsApp voice message through the automatic transcript and the chat strip.
- [ ] A release build: signing, R8, a smaller APK (the AAR's Java part is unused).
- [ ] On a phone: the floating button in a few apps (WhatsApp, Gmail, Chrome, Signal); HeliBoard's
      mic key; whether Samsung Keyboard's voice input setting lists Dictate (Galaxy phone).
- [ ] NPU (Snapdragon's Hexagon) through ONNX Runtime's QNN execution provider: fixed-shape,
      quantized encoder chunks; see the PR for what it needs. CPU int8 first, measured with the log.
- [ ] Settings on the phone beyond the dictionary (spoken commands on/off, boost weight), the
      replacements list.
- [ ] Unloading the model when Android asks for memory (`onTrimMemory`).
- [ ] iOS (separate issue).
