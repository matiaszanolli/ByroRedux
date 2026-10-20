Tracking issue for the four LSCR (loading-screen cover) items that `byroredux/src/loading_screen.rs` documents as "remain TODO". The P6 model cover (Skyrim inline SNAM/RNAM/XNAM pose, FO4 TNAM→TRNS) ships without them; none is reachable from a broken path — the cover renders with a fixed 0.3 rad stage-camera elevation.

- [ ] Full legacy XML / NIF menu animation (the pre-Creation loading-screen presentation).
- [ ] TRNS "Around Origin" semantics for the FO4 TNAM→TRNS transform.
- [ ] ONAM / ZNAM interactive zoom bounds.
- [ ] MOD2 authored camera-path NIFs (replaces the fixed `STAGE_CAMERA_ELEVATION_RAD`).

Filed so the two TODO comments in `loading_screen.rs` cite a live owner instead of aging silently (they cross the 30-day triage line on 2026-11-01). Source: TD5-2026-10-19-01 (#6181, `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md`).
