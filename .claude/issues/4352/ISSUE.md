# #4352 — TD3-004: docs/engine still points at *byroredux/src/boot.rs* after the `boot/` split (3 broken links, 8 present-tense sites)

**Labels**: low, ecs, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4352

- **Severity**: LOW · **Dimension**: 3
- **Location**: broken links `docs/engine/launcher.md:6,195`, `docs/engine/physics.md:94`; present-tense `docs/engine/m47-2-design.md:248`, `docs/engine/npc-spawn-ai-packages.md:188,507,513`, `docs/engine/packal.md:166`, `docs/engine/save-load-roundtrip.md:111`, `docs/engine/pipeline-overview.md:124,142` · **Status**: NEW (skill-file sites are #4174) · **Age**: `8c5e02aab` (09-09) · **Effort**: small · **Kind**: doc-rot
- **Suggested Fix**: Re-point to `byroredux/src/boot/world.rs`, `byroredux/src/boot/cli.rs`, `byroredux/src/boot/schedule/post_update.rs`, `byroredux/src/boot/schedule/update.rs` and `byroredux/src/boot/schedule/mod.rs` as mapped in the Dim 3 evidence. Cite functions, not line numbers.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
