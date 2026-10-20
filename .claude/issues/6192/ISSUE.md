# TD8-2026-10-19-04: Zero/test-only-caller `pub` helpers and one unused `_`-parameter that survived the #5800 split

Labels: low,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW — **Dimension**: 8 — **Status**: NEW
- **Location**: `crates/scripting/src/fragment/vm/checker.rs:1163` (`check_native(.., _receiver_kind: Kind, ..)`: unused in the 140-line body; all three callers `checker.rs:967,1078`, `natives/game_utility.rs:272` pass one — twice a dummy `Kind::None`; the sibling `check_forms/_location/_alias` `_statement` params are legit — they are `Self::check_*` fn-pointer table entries `checker.rs:1013-1022`); `crates/renderer/src/vulkan/allocator.rs:340` (`log_fragmentation_report`, zero callers; the `mem.frag` command uses `fragmentation_report_lines` directly, `world_info.rs:1364`); test-only callers: `crates/audio/src/lib.rs:626` (`manager_mut`), `crates/core/src/ecs/components/bsx.rs:53` (`contains_marker_children`), `scene_flags.rs:101,106` (`is_node`, `sorting_disabled`), `crates/core/src/form_id.rs:104` (`as_uuid`), `crates/game-detect/src/vdf.rs:67` (`string_map`), `crates/plugin/src/esm/records/mod.rs:86` (`CHALLENGE_FLAG_SHOW_ZERO_PROGRESS` alias, zero consumers)
- **Age**: `_receiver_kind` ef3dafe38 (2026-10-16, #5800 split); others older
- **Effort**: trivial
- **Description**: `_receiver_kind` is a pure survivor (delete the param, do not rename); the helpers are the same "reachable only by tests" class as closed #5745. `is_oil_explosion` (`fog_volume.rs:187`) is deliberately kept per AUDIT_RENDERER_2026-09-05 — not flagged. Bit-flag vocabulary consts (`shader_flags.rs`, `bsx.rs`, `challenge.rs`) are decoded-field vocabulary — not flagged.
- **Evidence**: `git grep -nw` per name; scan scripts `/tmp/audit/tech-debt/d8w/scan*.py`, outputs `zero.txt`, `testonly.txt` (the 384-row test-only list is noisy — shared method names — only the verified rows are listed here).
- **Proposed fix**: drop `_receiver_kind` from the signature and its three call sites; delete `log_fragmentation_report` and the dead items, or gate the test-only ones `#[cfg(test)]`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
