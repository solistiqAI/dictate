# mediaremote-adapter in dictate

Copy of https://github.com/ungive/mediaremote-adapter at commit 0e82bf1c44ec39554fbfc402ec88a48c6ae2a0fd
(2026-10-17, BSD 3-Clause, see LICENSE), without its CMake and scripts (src/test/NowPlayingTest.* stays: the `test` command uses it).

Why: since macOS 15.4 only Apple-signed processes may use the MediaRemote framework ("Now Playing").
The adapter loads a small helper framework into Apple's `/usr/bin/perl`, which is entitled, and prints
now-playing JSON / sends play-pause commands. VoiceInk uses the same adapter to pause media while
recording; dictate does too (`src/platform/macos/media.rs`).

Build: `scripts/bundle.sh` compiles `src/{adapter,private,utility}/*.m` with clang into
`Dictate.app/Contents/Resources/MediaRemoteAdapter.framework/MediaRemoteAdapter` and copies
`bin/mediaremote-adapter.pl` next to it. No changes to the sources.

Update: copy the same folders from a newer commit, update the commit above, run `scripts/install.sh`.
