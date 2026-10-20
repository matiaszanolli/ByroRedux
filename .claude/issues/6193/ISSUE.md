# TD8-2026-10-19-05: `--all-targets` (non-blocking CI step) re-accrued 20 lints in test/example targets (regression of #5850)

Labels: low,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW — **Dimension**: 8 — **Status**: re-accrual of TD8-2026-10-15-02 (#5850, closed; different sites — regression class, not same lines)
- **Location**: `field_reassign_with_default` x6: `crates/sfmaterial/src/settings.rs:279`, `crates/scripting/src/condition/tests.rs:204`, `magic_effects.rs:439,546`, `obscript_actor_values.rs:153`, `obscript_effects.rs:1575`; `items_after_test_module` x4 (bin test): `byroredux/src/asset_provider/texture.rs:1002`, `commands/quest.rs:1255`, `npc_spawn/resumable/runtime.rs:401`, `save_io/live_load.rs:193`; `empty_line_after_outer_attr` x3: `crates/scripting/tests/{perk_entry_points_real_data.rs:46,pex_recognize_e2e.rs:39,location_aliases_real_data.rs:38}`; `type_complexity` x2: `crates/hkx/src/animation.rs:2876`, `crates/plugin/src/esm/reader.rs:2109`; `needless_borrows_for_generic_args` x2: `crates/plugin/src/esm/records/actor/tests.rs:98,110`; `crates/pex/src/decompile/lower.rs:868` (`needless_update`); `crates/plugin/examples/dump_npc_subs.rs:57` (`borrow_deref_ref`); `crates/scripting/src/obscript_more/tests.rs:110` (`erasing_op`, `0x10c7 + 0`)
- **Age**: newest 6e9d55aa9 (10-18), `reader.rs`/`animation.rs` sites 10-15; four are older than 10-12 and show only because 1.96 reports them under `--all-targets` (e.g. `texture.rs:1002` 2026-06-28)
- **Effort**: small
- **Description**: `cargo clippy --workspace --all-targets --keep-going` (no `-D`) finishes; 30 warning lines, 20 distinct sites, zero errors, no deny-level lint. Renderer, physics, save, ui test targets are clean. Informational (CI step is `continue-on-error`).
- **Evidence**: `/tmp/audit/tech-debt/dim8_clippy_all.log`.
- **Proposed fix**: `cargo clippy --fix --workspace --all-targets` handles most; reorder the four `items after test module`; `0x10c7 + 0` -> `0x10c7`; `type_complexity` pair gets a `type` alias.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
