# TD3-2026-09-29-02: AGENTS.md is a divergent fork of CLAUDE.md (84 differing lines) and still recommends the #3895 test trap

**Labels**: low,tech-debt,documentation,doc-rot

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 3 · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Location**: `AGENTS.md` (added `db625997b` 2026-07-27; last edited `76c4521cf` 2026-09-23)
- **Evidence**:
  - `AGENTS.md:10` reads `cargo test -p byroredux-core    # Run ECS/core tests (162 tests)`. That is the
    exact command CLAUDE.md:16-25 warns silently drops the #486 inspect-gated guards (#3895).
  - It carries a "rustc ≥ 1.94 / distro rustc 1.93.1" toolchain section that CLAUDE.md dropped.
  - It repeats the `GameArchive` line.
  - Neither file refers to the other.
- **Suggested Fix**:
  - Fold any still-true fact unique to AGENTS.md into CLAUDE.md.
  - Replace AGENTS.md with a pointer, or a symlink, to CLAUDE.md.

**Validated at HEAD 9fcfdc3fc**: `AGENTS.md:10` reads `cargo test -p byroredux-core    # Run ECS/core tests (162 tests)` (no `--features inspect`); the rustc ≥ 1.94 section is at :16; `diff CLAUDE.md AGENTS.md` shows 84 differing lines; AGENTS.md:71/133/136 repeat the TD3-01 stale names.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
