# #4387 — TD8-008: Three `byroredux` binary feature gates are never compiled in their non-default state in CI

**Labels**: low, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4387

- **Severity**: LOW · **Dimension**: 8
- **Location**: `byroredux/Cargo.toml:7-22`; cfg sites `byroredux/src/main.rs:8,897`, `byroredux/src/boot/mod.rs:95,121`; `.github/workflows/ci.yml` · **Status**: NEW (same class as closed #1763 / #3894, which each got a lane) · **Effort**: trivial · **Kind**: tech-debt
- **Finding**: No lane builds `byroredux` with `--no-default-features` (the `not(feature = "debug-server")` arms), `--features tracing-tracy` or `--features dhat-heap`, so any of the three can rot until a developer reaches for the profiler.
- **Suggested Fix**: Add three separate `cargo check -p byroredux` lines to the existing check job.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
