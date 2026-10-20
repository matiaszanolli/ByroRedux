# TD4-2026-10-19-02: Skill layout rows and `First step` commands still name five files that were split into thin hubs, so symbol attributions are wrong and delta scoping silently reports nothing

Labels: medium,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: MEDIUM (same silent-miss class as closed #5473; the validator passes because the hub file still exists)
- **Dimension**: 4 | **Effort**: small | **Age**: `d7c47d741` (10-15), `c0de708ae` / `ef3dafe38` / `83fce5388` (10-16)
- **Hubs now** (prod LOC): `fragment/vm.rs` 134 (was 6213) -> `vm/{interp,checker,registrations,script_model,value,natives/}`; `translate/effects.rs` 97 (was 5295) -> `effects/{model,lower,guards,receivers,scope,inline,vm_bridge,prims/}`; `condition.rs` 123 -> `condition/{eval,catalog,context,requirement}`; `byroredux/src/save_io.rs` 86 -> `save_io/{registry,commands,live_load,player}`; `plugin/src/esm/cell/mod.rs` 68 -> `lighting.rs` etc.
- **Wrong attributions**: `audit-scripting/SKILL.md:11,36,68,90,267,282,710` (`fragment/vm.rs` "interpreter + `check_*`"), `:128,129,135` (`translate/effects.rs`; `every_effect_primitive_bounds_its_argument_count` is in `effects/tests.rs:4856`), `:340` (`condition.rs::evaluate` is `condition/eval.rs:627`); `audit-concurrency/SKILL.md:280` (test is `condition/tests.rs:1087`); `audit-save/SKILL.md:8,28,100,251,283` and `audit-gameplay/SKILL.md:86` (`build_save_registry`/`MUTABLE_DELTA_COLUMNS` in `save_io/registry.rs`, `PRE_RELOAD_RESOURCES`/`execute_pending_save_loads`/`reload_*_session` in `live_load.rs`, `SaveCommand`/`validate_form_ids` in `commands.rs`, `apply_player_pose` in `player.rs`); `audit-starfield/SKILL.md:79` (`decode_starfield_lighting_tail` is `esm/cell/lighting.rs:148`).
- **Silent-miss commands**: `audit-save/SKILL.md:101` (`git log -p -S'register_' --since=2026-10-15 -- byroredux/src/save_io.rs` returns 0 commits; the dir returns 6), `:252` (`grep 'fn validate_' ... save_io.rs` misses `commands.rs:24,79`), `:284` (`git log -- save_io.rs`: 0 since 10-16 vs 24 for the dir); `audit-scripting/SKILL.md:128` (`git log --since=2026-10-16 -- translate/effects.rs`: 1 commit vs 11 for `effects/`).
- **Proposed fix**: repoint each pathspec to the dir (`-- byroredux/src/save_io`, `-- crates/scripting/src/translate/effects`) and each symbol to its new file; add the five hubs to the "thin-dispatch file + directory pairs" list in `_audit-common.md:9` (today it names `save_io`/`components` but not `vm`, `effects`, `condition`, `esm/cell`). Prefer a one-line rule in `_audit-common.md` ("a path-scoped `git log`/`grep` on a hub file is wrong; scope to `<stem>/`") over per-site fixes.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
