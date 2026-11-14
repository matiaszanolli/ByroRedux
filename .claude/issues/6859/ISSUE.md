# #6859: BIK Phase 3: engine start sequences
State: OPEN
Labels: enhancement ui 

Part of #6855. PRD [§9 Phase 3](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/bink-video-playback.md#9-delivery-sequence).

## Acceptance Criteria
- [ ] BIK-REQ-004: start sequences play from each game's INI (Oblivion `SIntroSequence`, `SMainMenuMovieIntro`, and what Phase 0 found for the others)
- [ ] BIK-REQ-005: engine-started movies play where the original plays them, and a launch flag skips every one of them

**Exit:** a headless boot of Oblivion and of Skyrim plays its start sequence to its last frame.

