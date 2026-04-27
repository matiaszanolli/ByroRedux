# NIF-D5-09: Starfield BSFaceGenNiNode undispatched — alias to NiNode (1 LOC for 1,282 face meshes)

URL: https://github.com/matiaszanolli/ByroRedux/issues/727
Labels: enhancement, nif-parser, low

---

## Severity: LOW

## Game Affected
Starfield (face meshes)

## Location
- `crates/nif/src/blocks/mod.rs` — no arm; would alias to `NiNode`

## Description
One per face NIF — 1,282 / 1,282 in `FaceMeshes.ba2`. Likely a `NiNode` subclass that adds FaceGen-blend coefficients; pre-Starfield Bethesda used `NiBSAnimationNode` for similar role.

## Evidence
2026-04-26 corpus sweep:
- `Starfield - FaceMeshes.ba2` — 1,282 occurrences (one per file)

## Impact
FaceGen morph data dropped; SF NPC face customisation reverts to whatever the base mesh ships with.

## Suggested Fix
Add an alias arm to plain `NiNode` first to clear the NiUnknown row (1 LOC), then a dedicated parser once the trailing FaceGen fields are reverse-engineered. nif.xml's `BSFaceGenNiNode` predecessor exists (line ~6620) but the SF wire layout is unconfirmed.

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D5-09)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: N/A
- [ ] **TESTS**: Byte-exact dispatch test (passes through to NiNode parser); corpus regression
