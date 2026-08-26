# Issues 2262, 2263, 2264, 2273 — doc-rot batch fix

## #2262 — audit-tech-debt/SKILL.md #[ignore]-count baseline recipe unscoped
- `grep -RIn '#\[ignore\]' .` scans whole repo textually (323), including markdown
  self-references, instead of scoping to `.rs` sources under crates/byroredux (135
  raw mentions, 96 actual attribute lines).
- Fix: scope both Phase-1 baseline grep and Dimension-9 discovery grep to
  `--include='*.rs' crates byroredux`, matching the other two baseline metrics
  in the same block. Recommended: `grep -RIn '^\s*#\[ignore\]' --include='*.rs' crates byroredux | wc -l`

## #2263 — XXXX-protocol false-positive exclusion list stale
- `.claude/commands/audit-tech-debt/SKILL.md` Dimension-5 exclusion note only names
  `reader.rs` and `records/misc/magic.rs`.
- Commit 560c6741d (#1849) added two more legit references:
  `crates/plugin/src/esm/cell/wrld.rs:175`, `crates/plugin/src/esm/cell/mod.rs:871`
- Fix: extend file list, or better, key exclusion on comment content instead of file paths.

## #2264 — ROADMAP.md calls PACK/QUST/DIAL/MESG/PERK/SPEL/MGEF "stubs"
- **Already fixed** by an unrelated commit `f6555b7b` (2026-08-19), 17 days after
  this issue was filed (2026-08-02). ROADMAP.md:1021 already reads "PACK /
  QUST / DIAL / MESG / PERK / SPEL / MGEF now fully parsed (#446/#447 closed;
  see M24.2/M43 rows above for decode detail)" — exactly what the issue asked
  for. No code change needed; closing with a note pointing at that commit.

## #2273 — MaterialTable collision-policy comment stale field count
- `crates/renderer/src/vulkan/material.rs:1143` says "75 scalar fields" but
  GpuMaterial has 87 fields / 348 bytes since 2026-07-27 growth.
- Fix: update to 87, or reference `size_of::<GpuMaterial>()` instead of restating count.
