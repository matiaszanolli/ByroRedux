# #5287 — GAME-D7-2026-10-05-02: ModelStage.key sits under a bare #[allow(dead_code)] and is never read

- **Labels**: low,gameplay,tech-debt,bug
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5287

- **Severity**: LOW (dead code; the allow has no justification)
- **Dimension**: 7 (justify every `#[allow(dead_code)]`) and 6 (the LSCR model cover, `e60911864`)
- **Location**: `byroredux/src/loading_screen.rs:84-85`; the only writer is `:425-427`
- **Status**: NEW
- **Description**:
  - The model-stage key is built (`:209`) and stored, but nothing compares it. Every cover spawns a fresh stage, and only the `Artwork::Image` key is compared, for reuse (`:249-252`).
  - Every other `allow(dead_code)` in the gameplay files carries a reason (`inventory.rs:894`, `:907` for #4464; `interaction.rs:181` is `cfg_attr(not(test))`).
- **Suggested Fix**: drop the field and the attribute. If the field is meant for a future stage cache, add a comment saying so.

_Source: `AUDIT_GAMEPLAY_2026-10-05.md` (GAME-D7-2026-10-05-02), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
