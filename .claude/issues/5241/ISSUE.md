# #5241: CHAR-2026-10-03-D1-02: #5039's `player_derived_stats_system` registration was inserted between the pool-regen comment block and its system, so the "#2153 3-deep hold stack" comment now heads the wrong registration

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: low,character,documentation,doc-rot,concurrency
- **Source report**: docs/audits/AUDIT_CHARACTER_2026-10-03.md

- **Severity**: LOW
- **Dimension**: Ruleset Seam / doc rot
- **Game**: all
- **Location**: `byroredux/src/boot/schedule/update.rs`, in the `register_update_systems` block above `pool_regen_tick_system`.
- **Status**: NEW (`c9254beb8`).
- **Description**: The comment "#2391 / ECS-D5B-03 — declared via `add_exclusive_with_access` … This system is #2153's site: it builds a 3-deep hold stack (`PoolRegenConfig` read held across `PoolRegenAccumulator` write …)" describes `pool_regen_tick_system`. The diff added the #5039 comment and the `player_derived_stats_system` registration directly under it. Read top-down, the hold-stack rationale (`PoolRegenConfig`, `PoolRegenAccumulator`) now sits on a system that touches neither, and `pool_regen_tick_system` has no rationale of its own.
- **Impact**: A `/audit-concurrency` Dim 4 reader or a future refactor attributes the exclusivity rationale to the wrong system. Comment only.
- **Suggested Fix**: Move the #5039 comment and registration above the "CHARAL pool regen" comment block, or below the `pool_regen_tick_system` registration.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other comment blocks in `register_update_systems`)

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-10-03.md` (finding CHAR-2026-10-03-D1-02, /audit-character 2026-10-03, HEAD `2c36c29d8`).*
