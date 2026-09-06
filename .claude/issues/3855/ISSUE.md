# #3855: TD1-2026-09-05-06: `boot.rs` crossed 2232 production LOC — promote #3739's five `register_*_systems` functions to five files

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-06) via `/audit-publish`, 2026-09-05. Labels: `low,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3855 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-06), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `byroredux/src/boot.rs` (2232 production / 2670 total LOC)
- **Status**: NEW (file-level; the function-level predecessor #3739 is CLOSED and correctly so)
- **Age**: `40d533a8` origin; 1018 → 2670 total across **119 commits** — the most-edited file in this bucket
- **Description**: #3739 (`d03f7a35`, 2026-09-03) split `build_scheduler` into five per-stage
  `register_*_systems` functions. That was a function-level fix and it holds — `build_scheduler`
  is now an 18-line orchestrator: eight lines of comment, `Scheduler::new()`, five `register_*` calls. It did not, and was not meant to, move the file below the
  file-level threshold; the file crossed on unrelated growth. The follow-through is mechanical
  because #3739 already drew the boundaries.
- **Evidence**: the file's production mass, by symbol —

  | Symbol | LOC | Concern |
  |---|---|---|
  | `run` (`:74`) | 319 | process entry: settings load, event loop, `App` construction |
  | `init_tracing` (`:48`) | 26 | process entry |
  | `expand_boot_request` (`:2015`) | 36 | CLI expansion |
  | `expand_game_profile_args` (`:2189`) | 161 | CLI expansion |
  | `build_world` (`:397`) | 351 | every `insert_resource` / component registration |
  | `build_scheduler` (`:753`) | 18 | orchestrator (post-#3739) |
  | `register_early_systems` (`:771`) | 76 | Stage::Early |
  | `register_update_systems` (`:847`) | **382** | Stage::Update (+ 8 nested dispatch shims) |
  | `register_post_update_systems` (`:1231`) | **210** | Stage::PostUpdate |
  | `register_physics_systems` (`:1443`) | 47 | physics |
  | `register_late_systems` (`:1490`) | **413** | Stage::Late |
  | `install_runtime_registries` (`:1908`) | 107 | registry install |

  Five production functions exceed 200 LOC. The five `register_*` bodies total ≈1130 LOC — over
  half the file.
- **Impact**: `boot.rs` is documented in `_audit-common.md` as "the authority for *which stage does
  X run in*". At 2670 lines that authority is hard to consult, and 119 commits means nearly every
  feature lands a line here — the highest merge-conflict surface in the binary.
- **Related**: #3739 (function-level, closed); #2731 (the `main.rs` split that created this file —
  **do not re-propose splitting `main.rs`**, verified at 1267 total / 1096 production today).
- **Suggested Fix**: `boot/{mod,cli,world,registries}.rs` + `boot/schedule/{mod,early,update,post_update,physics,late}.rs`,
  moving each `register_*_systems` body verbatim. `mod.rs` keeps `run`/`init_tracing` and the
  `pub(crate)` re-exports so no call site changes. This is the lowest-risk split in the bucket —
  the boundaries already exist as function boundaries and there is a
  `byroredux/src/scheduler_access_tests.rs` guard on the result.
- **Effort**: small

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
