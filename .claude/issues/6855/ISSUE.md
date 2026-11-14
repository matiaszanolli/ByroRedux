# #6855: Bink video playback — tracking (PRD bink-video-playback)
State: OPEN
Labels: enhancement game:fnv ui audio 

Tracking issue for the APPROVED PRD [Bink video playback](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/bink-video-playback.md) (approved by the owner 2026-11-11; decisions Q1–Q3 taken as recommended: the system FFmpeg reached at run time with nothing linked, Bink 2 skipped until the open decoders are measured, engine movies where the original plays them with a skip flag).

The games' own Bink movies play: engine-started intros and logos, and every script `PlayBink` request, full-screen with sound, holding the game, ending on Escape when allowed. Found by the FNV new-game route: VCG00 runs `PlayBink "FNVIntro.bik" 1 1 0 1`, which is only logged today.

## Phases
- [ ] Phase 0: #6856 — movie corpus, verify items, design (no engine code)
- [ ] Phase 1: #6857 — Bink 1 decoding through the run-time FFmpeg
- [ ] Phase 2: #6858 — playing a movie request (PlayBink, video.status, video.skip)
- [ ] Phase 3: #6859 — engine start sequences

