# #7186 — TOOL-D4-2026-11-21-02: the profile expander does not know the godray, shaft and Cornell-reference labs are game-independent

Labels: low,tech-debt,bug

- **Severity**: LOW (PLAUSIBLE; not run)
- **Dimension**: Boot Handoff & Persistence
- **Exposure**: a developer whose per-user `profiles.toml` sets `[defaults].game` (the shipped file has no `[defaults]`)
- **Location**: `byroredux/src/boot/cli.rs` `expand_game_profile_args` (`has_other_load_flags` list) against `byroredux/src/scene.rs` `diagnostic_scene`
- **Status**: NEW
- **Description**: the list is hand-kept, and gained `--combustion-lab-steam` and `--recede-lab` this window. It still misses `--godray-lab*`, `--shaft-lab*` and `--cornell-reference[-view]`, which `diagnostic_scene` handles. So with a default game set, a bare `--shaft-lab` launch also gets that game's `--esm`, archives and `[defaults].cell`. The lab still wins the scene branch, but the boot does ESM and archive work it was meant to skip.
- **Evidence**: coordinator: `grep` of the `has_other_load_flags` window for `shaft-lab`, `godray-lab` and `cornell-reference` finds none.
- **Suggested Fix**: one shared predicate, `cornell::is_harness_launch(args)`, used by both `diagnostic_scene` and the expander, or a test that compares the two sets.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / readers / folds named above)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_TOOLING_2026-11-21.md`
