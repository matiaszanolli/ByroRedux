# #4381 — TD8-002: Five `populate_*_fragments` entry points have zero callers workspace-wide, yet stay re-exported from `lib.rs`

**Labels**: low, scripting, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4381

- **Severity**: LOW · **Dimension**: 8
- **Location**: `crates/scripting/src/fragment/populate.rs:145,254,397,532,552`; re-exports `crates/scripting/src/lib.rs:69-76` · **Status**: NEW (follow-on to #3854) · **Age**: `27875a021` (08-23), `ab2a193a9` (09-01) · **Effort**: small · **Kind**: tech-debt
- **Finding**: The engine calls only the `populate_owned_*_with_providers` family. `populate_quest_fragments_from_pex_detailed_with_providers`, `populate_quest_fragments_from_script_with_providers`, `populate_scene_fragments_from_pex_detailed_with_providers`, `populate_scene_fragments_from_script` ("exposed for focused conformance tests", yet no test calls it) and `populate_scene_fragments_from_script_with_providers` have no caller, test or example. Because they are `pub` and re-exported, the dead-code lint cannot see them.
- **Suggested Fix**: Delete the five functions and their re-exports; keep the `_internal` helpers the owned variants use.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
