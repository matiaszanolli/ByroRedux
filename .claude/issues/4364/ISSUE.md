# #4364 — TD4-003: The mod-runtime "no engine consumer" premise survives in two audit files after #3828, and the layout still names *runtime.rs*

**Labels**: low, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4364

- **Severity**: LOW · **Dimension**: 4
- **Location**: `.claude/commands/_audit-common.md:186` ("audit it as a contract, not as a live path"), `:28`; `.claude/commands/audit-tech-debt/SKILL.md:26` ("still consumer-less") · **Status**: NEW (unfixed siblings of closed #3828) · **Effort**: trivial · **Kind**: doc-rot
- **Finding**: `SandboxRuntime` is consumed by `byroredux/src/extensions/mod.rs` and `byroredux/src/extensions/install.rs`; `crates/mod-runtime/src/runtime/` is a directory (#3853).
- **Suggested Fix**: Match `audit-safety` Dim 11's live-consumer framing; list the `runtime/` modules.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
