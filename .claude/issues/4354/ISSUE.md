# #4354 — TD3-006: Every file:line anchor in `pipeline-overview.md` predates the #2731 and boot splits

**Labels**: low, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4354

- **Severity**: LOW · **Dimension**: 3
- **Location**: `docs/engine/pipeline-overview.md:25-34,124,137-154`; `docs/engine/launcher.md:123` · **Status**: NEW · **Age**: since `7a4cb781f` (08-13) · **Effort**: small · **Kind**: doc-rot
- **Finding**: For example, `main.rs:1035` about_to_wait is now `byroredux/src/app_events.rs:518`, and `main.rs:378` render_one_frame is now `byroredux/src/app_frame.rs:49`; see Dim 3's full anchor table.
- **Suggested Fix**: Replace line anchors with `file::function` anchors and refresh the currency note.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
