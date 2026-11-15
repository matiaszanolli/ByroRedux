**HEAD**: 3c065f6a7 (audited) · **Baseline**: AUDIT_AUDIO_2026-10-31.md. Also deduped against the 11-14 and 11-15 incremental reports. · **Scope**: 24 audio and music commits since 10-31, plus the audio dimensions the 11-14 pass did not cover.

# Audio Audit: 2026-11-15

**Test runs**:
- `byroredux-audio`: 61 passed.
- Binary-crate `music` and `audio` tests: 85 passed.
- Headless boot guards: green.

**Result**: no handle leak and no content-reachable panic. Every music path stops a kira handle before dropping it.

| ID | Sev | Finding | Outcome |
|---|---|---|---|
| AUD-1 | MEDIUM | A malformed WAV in `Data\Music` spins a kira 0.12.5 decoder thread at 100% CPU, and nothing can stop it. Two causes: a zero-length loop region, and a `data` chunk that claims more frames than it holds. Reproduced against kira 0.12.5. | 26accaf9a: a stream with no frames is refused at load. The over-claimed case is inside kira: #6945. |
| AUD-2 | LOW | A movie hold paused, then resumed, a sound that was already fading out, so the cut voice line played to its end. | 26accaf9a: the hold skips `stop_issued` sounds. |
| AUD-3 | LOW | One unplayable track silenced its whole playlist until the place changed. | 4fe55adad: the next track is tried; Missing only when none plays. |
| AUD-4 | LOW | A load ended the sting's silence but not the sting itself, contrary to `music.md` §9. | 2419fb27c: `end_sting_layer`. §9 now states the one exception, an Oblivion category stream (#6903). |
| AUD-5 | LOW | Two music rules conflict with, or lack, a local source: Fallout 3 PlayMusic, and the Media Set crossfade field. | #6946 (decision). |
| AUD-6 | LOW | Per-frame work: the tension scan, place-string copies, and `SetCellMusicType` churn. | 1033f72da fixes the churn; the rest is in #6946. |
