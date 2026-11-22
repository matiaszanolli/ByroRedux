# #7272: INC-G-01: The AI-package doc's Sandbox section still says the seat search always centres on the actor

- **Severity**: LOW
- **Dimension**: audit-gameplay Dim 5 / doc rot
- **Location**: `docs/engine/npc-spawn-ai-packages.md:240`; `byroredux/src/systems/sandbox.rs:61-62` (`SEAT_SEARCH_RADIUS` doc)
- **Changed in**: 5ede7b032 (#7262 edited §6 and §9 but not §5)
- **Status**: NEW
- **Description**: The doc says "Search *center* is always the actor's own spawn position". Since #7262, a Near Editor Location Sandbox centres on its `EditorPlacement`. `SEAT_SEARCH_RADIUS`'s doc still says the radius is measured from the actor's "current position".
- **Evidence**: `docs/engine/npc-spawn-ai-packages.md:240`, against `sandbox.rs`'s `travel::package_home(world, npc, behavior.near_editor_location, npc_g.translation)`. Lines 279-282 of the same doc now state the exception, so the doc contradicts itself.
- **Impact**: Doc only.
- **Related**: #7262
- **Suggested Fix**: Reword both to "the actor's own position, or its editor placement for a Near Editor Location package (#7262)".

**Found by**: `docs/audits/AUDIT_INCREMENTAL_2026-11-22b.md`

## Completeness Checks
- [ ] **SIBLING**: other Near Editor Location statements in the doc checked
