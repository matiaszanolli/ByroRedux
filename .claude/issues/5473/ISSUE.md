# #5473: TD4-2026-10-08-01: Three skills' First-step `git log` pathspecs name files the split wave deleted, so delta scoping silently reports those dimensions unchanged; audit-tooling cites a renamed guard

**Labels**: low,tech-debt,documentation,doc-rot
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5473

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-08.md` — `TD4-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**:
  - `.claude/commands/audit-fnv/SKILL.md:48` and `.claude/commands/audit-performance/SKILL.md:149`: name
    `byroredux/src/streaming.rs`, deleted by #5092 (`54d713dee`, 10-06).
  - `.claude/commands/audit-physics/SKILL.md:80` (Dim 4 First step): names `crates/physics/src/world.rs`, renamed to
    `world/mod.rs` by #5311 (`12ca34a70`, 10-05). `move_character`, which that dimension is about, now lives in
    `world/queries.rs`.
  - `.claude/commands/audit-tooling/SKILL.md:72`: cites `the_engine_is_invoked_with_the_boot_request_path`. #5294
    (`322c36626`, 10-05) renamed it to `the_engine_is_invoked_with_the_boot_request_path_and_profiles_env`
    (`tools/byro-launcher/src/engine.rs:225`). `_audit-validate.sh` lists it as a skill advisory.
- **Status**: NEW. The performance report notes its own pathspec under "Stale skill premises" but files nothing. The
  validator half is not tracked anywhere.
- **Effort**: trivial
- **Evidence**:
  - `git log --since=2026-10-06T12:00 --format=%h -- byroredux/src/streaming.rs` → 0 commits.
  - The same window on `byroredux/src/streaming` → 1 commit.
  - `_audit-validate.sh` extracts a path only when it starts right after a backtick
    (`grep -noE '\`[A-Za-z0-9_./{},-]+\.(rs|…)'`). A pathspec that is a later token inside a backticked command is never
    checked, so the gate printed `OK: all path references valid`.
- **Impact**:
  - `_audit-common.md` delta rule 2 lets a dimension with no commits get a one-line skim. Physics Dim 4, FNV's streaming
    dimension and Performance Dim 7 can now be skimmed while their code changes.
  - This is the "stale baseline misdirects the next audit" class. It is held at LOW because no audit has yet been shown to
    skip real changes this way: today's physics and performance auditors noticed the splits.
- **Related**: TD3-2026-10-08-03; PERF 2026-10-08 "Stale skill premises".
- **Suggested Fix**:
  - Replace the pathspecs with `byroredux/src/streaming` and `crates/physics/src/world` (directories), and rename the
    guard at `audit-tooling:72`.
  - Extend `_audit-validate.sh` to also scan whitespace-separated tokens inside backtick spans that begin with `git log`
    or `--`.

## Completeness Checks
- [ ] **SIBLING**: All skills' `First step` pathspecs scanned for other deleted files, not just the three named
- [ ] **TESTS**: A regression test pins this specific fix (`_audit-validate.sh` extended to check pathspec tokens inside backtick spans)
