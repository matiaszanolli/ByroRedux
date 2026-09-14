# #4368 — TD4-007: Backticked names of deleted or out-of-repo files in three skills

**Labels**: low, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4368

- **Severity**: LOW · **Dimension**: 4
- **Location**: `.claude/commands/audit-concurrency/SKILL.md:104` (*extensions.rs*), `.claude/commands/audit-tech-debt/SKILL.md:181` (*papyrus_provider.rs*, *extensions.rs*, and `compatibility.rs`/`runtime.rs`, which resolve only to unrelated same-named files), `.claude/commands/audit-renderer/SKILL.md:287` (memory file *reference_glsl_pathtracer.md*) · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Suggested Fix**: Italicise them; optionally widen the gate's memory-file skip rule beyond `feedback_*`.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
