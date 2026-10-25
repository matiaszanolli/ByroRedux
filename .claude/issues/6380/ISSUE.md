# TD3-2026-10-25-09: stale or malformed comments in recently touched code

Labels: low, doc-rot, tech-debt, documentation
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6380
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW
- **Dimension**: 3
- **Effort**: trivial
- **Sites**:
  - `byroredux/src/npc_spawn/ai_package.rs:3470`: test comment "Ambush (9): a procedure with no runtime, so the package wins with no behavior". Ambush has had a runtime since `9aaf5e1cf` (10-24: `AmbushBehavior`, `ai_package.rs:250-256`, `:390`). The test presumably passes because `pack(.., None)` has no `PLDT` location; reword to say that.
  - `byroredux/src/asset_provider/texture.rs:1262`: the `default_sound_candidates_hit_their_tagged_game_archive` doc opens a ```` ```sh ```` fence (`:1258`) and never closes it (the doc ends at a blank line). Age: `a6295284e` (2026-09-06, #3913).
  - `crates/plugin/examples/probe_weap_vats.rs:19` and `probe_weap_dnam.rs:12` cite `probe_weap_align.rs` as evidence. That file was deleted in this window (`git log --diff-filter=D`). Italicise or drop.
- **Fix**: As stated.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD3-2026-10-25-09`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
