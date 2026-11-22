# #7130: PAR-D2-2026-11-20-06: a present-but-undecodable EGM or cart-idle HKX is logged at `debug`

**Labels**: low,import-pipeline,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7130

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D2-06 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: byroredux/src/npc_spawn/resumable/runtime.rs:1191-1199 (`EgmFile::parse` Err), byroredux/src/asset_provider/animation.rs:105-113 (cart-idle `decode_animation` Err)
- **Status**: NEW (every other hkx/facegen consumer — actor_graph, archetype_idle, object_graph, the other animation.rs arms — warns)
- **Description**: The file was found in an archive (so the data is defective, not absent) but the failure is `log::debug!`, indistinguishable from "this NPC has no EGM". The NPC renders with the unmorphed head; the IDLE loop falls through to "no character animation HKX" (also debug). Contract (c): a degraded decode must not be silent.
- **Impact**: a corrupt/mod EGM silently loses facegen; vanilla census has zero undecodable sidecars.
- **Trigger Input**: `.egm` with size != `64 + morphs*(4+nv*6)`; `.hkx` failing `decode_animation` under an IDLE event candidate path.
- **Suggested Fix**: `warn!` once per path (an NPC loop would otherwise repeat it).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
