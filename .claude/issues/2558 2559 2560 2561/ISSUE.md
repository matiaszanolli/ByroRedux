# Issues 2558, 2559, 2560, 2561 — low-severity doc/tech-debt cleanup

## #2558 — FNV-D4-02 (LOW): stale FormID in NCR-faction spot-check test comment
- File: crates/plugin/src/esm/records/tests.rs:492-493
- Comment cites 0x0011E662 (a REPU record) instead of real NCR main faction 0x000A46E7.
- Test logic (loose name-substring match) is correct and unaffected; comment-only fix.

## #2559 — FNV-D5-01 (LOW): stale per-block baseline TSV after #2332's bhkSPCollisionObject split
- File: crates/nif/tests/data/per_block_baselines/fallout_nv.tsv (and check fo3 sibling)
- fallout_nv.tsv:80 still shows bhkCollisionObject 12981 0 with no separate bhkPCollisionObject line after commit 8ee151e0 split the dispatch arm.
- Fix: regenerate + check in FNV (and FO3 if affected) baseline TSV; run per_block_baselines --ignored test to confirm clean.

## #2560 — FNV-D8-01 (LOW): ROADMAP 145.1 FPS Prospector headline no longer reproducible after FSR became default
- File: ROADMAP.md:432,1117 (also references line 70 in issue evidence)
- 5c7acfe2 (2026-07-24) made FSR 3.1 Quality the default upscaler, 6 days after the 145.1 FPS baseline was captured under native TAA (8a668eff, 2026-07-18). Repro command now measures ~254 FPS.
- Fix: annotate ROADMAP.md noting pre-FSR-default capture date, point to already-correct "TAA (native)"/"FSR Quality" labeled columns. Doc-only.

## #2561 — FNV-D9-01 (LOW): guard_system::resolve_anchor duplicates travel_system::resolve_destination
- Files: byroredux/src/systems/guard.rs:70-79 (resolve_anchor), byroredux/src/systems/travel.rs:81-96 (resolve_destination), byroredux/src/systems/escort.rs:57 (existing correct reuse pattern), byroredux/src/boot.rs:920 (stale comment wording)
- Guard re-implements travel::resolve_destination's NearReference-FormID-resolve first half instead of sharing it (Escort already does share it via aliased import).
- Fix: extract shared NearReference -> GlobalTransform.translation resolve into an Option<Vec3>-returning helper both Guard and Travel call before applying their own (intentionally divergent) fallback; fix boot.rs:920 wording from "reuses" to "mirrors" for Guard.
