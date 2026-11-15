# #6921 — INC-2026-11-14-09: A music stem that fails to start is re-read, re-decoded and re-logged every frame

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-9). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: MEDIUM
- **Location**: `byroredux/src/music/controller.rs` (Stems branch of `play`); `byroredux/src/music/mod.rs` (`AudioOutput::play`); `crates/audio/src/world.rs` (`play_music_voice` returns `()`)
- **Changed in**: `controller.rs`, `mod.rs` (commits `45b3b0a64`, `bf42dca77`)

## Description
A playlist marks itself Missing when `out.play` returns false, but the Stems branch leaves `state.stems[i] = None` with `wanted` still true. Every frame it repeats `provider.extract(key)` (a full archive or loose read), a decode, and a `log::warn!`.

Separately, `play_music_voice` swallows a `mgr.play` error and `AudioOutput::play` still returns `true`, so the controller records a track that is not playing; a live non-looping playlist sees `ended` and starts the "next" track every frame.

## Evidence
`if out.play(voice, &stem.key, d.level * stem.level, start_fade, true) { state.stems[i] = Some(..) }` — no else branch. `AudioOutput::play`: `audio.play_music_voice(voice, sound, level, fade, looping); true`.

## Impact
Rare (a listed but corrupt/unreadable file, or a kira play failure), but then a per-frame archive read + decode + log line on the main thread, against MUS-SRC-012 (a missing file is noted once).

## Suggested Fix
Make `play_music_voice` return `bool`; remember a failed stem key per directive and treat it as Missing.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (playlist and one-shot paths on a `play_music_voice` failure)
- [ ] **TESTS**: A regression test pins this specific fix
