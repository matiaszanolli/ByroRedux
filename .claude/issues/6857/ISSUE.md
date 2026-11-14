# #6857: BIK Phase 1: Bink 1 decoding through the run-time FFmpeg
State: OPEN
Labels: enhancement audio 

Part of #6855. PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/bink-video-playback.md#9-delivery-sequence).

## Acceptance Criteria
- [ ] BIK-DEC-005: decoding goes through the system FFmpeg found at run time; nothing links it; without it every request is **Skipped** with one log line
- [ ] BIK-DEC-001: Bink 1 video decodes to frames at the file's resolution and frame rate
- [ ] BIK-DEC-002: Bink audio decodes to PCM at the file's rate and channel count
- [ ] BIK-DEC-004: a decode failure ends the movie as **Failed** with a log line naming file and frame
- [ ] §7.2 scenario 1: an ignored `cargo test` decodes every Bink 1 file of every in-scope game to its last frame

**Exit:** scenario 1 passes for every Bink 1 file of every in-scope game, and the decode rate is published.

