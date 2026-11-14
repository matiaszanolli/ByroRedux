# #6858: BIK Phase 2: playing a movie request (PlayBink, video.status, video.skip)
State: OPEN
Labels: enhancement game:fnv ui audio 

Part of #6855. PRD [§9 Phase 2](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/bink-video-playback.md#9-delivery-sequence). Depends on the native-page pause and the `audio` crate's streaming playback (M44).

## Acceptance Criteria
- [ ] BIK-REQ-001: ObScript `PlayBink` carries its flags (four on FO3/FNV, one on Oblivion, which is dispatched)
- [ ] BIK-REQ-002: Papyrus `Game.PlayBink` reaches the player with its arguments
- [ ] BIK-REQ-003: files resolve relative to `Data\Video`; a missing one is **Failed**
- [ ] BIK-PRES-001 … BIK-PRES-006: letterbox, frozen game time, A/V within one frame, Escape per the flag, mute/pause reported, the requesting script resumes when the movie ends
- [ ] BIK-RPT-001 … BIK-RPT-003: `video.status`, headless playback to the last frame, `video.skip`
- [ ] BIK-DEC-003: a Bink 2 request logs one **Skipped** line and the game continues
- [ ] §7.2 scenarios 2 (FNV intro in `fnv-new-game.sh`), 3 (flags) and 4 (Bink 2 reporting) pass

**Exit:** scenarios 2, 3 and 4 pass.

