# TD3-2026-10-25-03: "Nothing inserts `PoolRegenConfig` in production" is false since `3dc16208e` (FO3/FNV Action Points regen, 2026-10-25); seven sites still say regen is inert

Labels: low, character, doc-rot, game:fnv, game:fo3, tech-debt, documentation
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6374
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW (MEDIUM if the in-flight VATS/AP work #6350 or a character audit trusts them)
- **Dimension**: 3
- **Effort**: small
- **Age**: `3dc16208e` (2026-10-25, #6350: "the ActionPoints pool goes live with load-order-priced regen"); `build_vats_model` (`byroredux/src/npc_spawn.rs:317`) returns the config and `cell_loader/references/mod.rs:362-365` does `insert_resource(regen)` on every Fallout3NV load.
- **Stale sites**:
  - `crates/core/src/character/regen.rs:121-127` ("Nothing inserts it in production on any game (#3848)... inert in effect"; also the struct now has an `action_points` lane)
  - `byroredux/src/boot/schedule/update.rs:841-849` ("No-ops today... arrives only once a live CharacterRuleset wiring reaches Oblivion")
  - `docs/engine/charal.md:243` (snippet lacks `action_points`) and `:262-266` ("Mechanism BUILT, registered, and no-op... inserted only by unit tests")
  - `docs/engine/charal-oblivion-ruleset.md:416-424`
  - `docs/feature-matrix.md:321-323` ("Pool regen remains inert")
  - `ROADMAP.md:668` ("`regen` no-ops until an Oblivion-shaped `PoolRegenConfig` exists")
- **Fix**: Reword to "inert on TES (no Fatigue/Magicka AVIFs); live for FNV/FO3 Action Points only (#6350, `build_vats_model`)". Feeds the next `/session-close`.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD3-2026-10-25-03`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
