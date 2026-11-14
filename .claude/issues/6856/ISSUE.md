# #6856: BIK Phase 0: movie corpus, verify items and design (no engine code)
State: OPEN
Labels: enhancement ui 

Part of #6855. PRD [§9 Phase 0](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/bink-video-playback.md#9-delivery-sequence). No engine code.

## Acceptance Criteria
- [ ] §3 census: a checked-in corpus script re-measures files, Bink versions and audio tracks per game; its output is in ROADMAP's per-game compatibility matrix
- [ ] Verify, BIK-REQ-003: whether Skyrim, Fallout 4 and Starfield also play Bink only from loose files
- [ ] Verify, BIK-REQ-004 / §3 start rows: what Fallout 3's `sIntroMovie` plays, and where Oblivion's new-game intro and Skyrim's logo are named
- [ ] Verify, BIK-DEC-002: which of several audio tracks the original plays
- [ ] Verify, BIK-PRES-001: what the original does with the letterbox flag clear
- [ ] Verify, Q2: the open Bink 2 decoders (NihAV, the unmerged FFmpeg patch): decode rate on the `KB2g` / `KB2j` corpus and licences
- [ ] `docs/engine/video-playback.md` written and added to the PRD's Related table

**Exit:** the census is in ROADMAP for every in-scope game, the design is merged, and every verify item is closed or filed as an issue.

