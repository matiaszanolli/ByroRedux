# TD1-2026-09-29-04: `npc_spawn/resumable.rs` crossed to 2235 (+565)

**Labels**: low,gameplay,tech-debt,bug

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 1 · **Status**: NEW · **Effort**: small–medium
- **Location**: `byroredux/src/npc_spawn/resumable.rs`
- **Age**: `5570c221c` (+661 head-part textures, 09-23); `a070baaad` (+340 player body, 09-28).
  `advance_runtime_unit` is now 518 lines (+92).
- **Suggested Fix**: the two spawn arms are already separate state machines. Split them into
  `resumable/runtime.rs` and `resumable/prebaked.rs`, and keep `NpcSpawnJob` + part parenting in `mod.rs`.
  - Runtime FaceGen: `RuntimeNpcState`, `prepare_runtime_state`, `advance_runtime_unit`,
    `spawn_runtime_head`, head morphs, hair tint (≈443–1800).
  - Prebaked: `PrebakedNpcState`, `prepare/advance/finalize_prebaked` (≈1800–2154).

**Validated at HEAD 9fcfdc3fc**: `prod_loc byroredux/src/npc_spawn/resumable.rs` = 2235.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
