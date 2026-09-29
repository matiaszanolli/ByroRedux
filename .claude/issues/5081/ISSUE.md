# NIF-D4-2026-09-29-01: two docs still describe Starfield BSGeometry streams as "decoded Y-up" after b9e961eeb moved the basis change to import

**Labels**: low,documentation,doc-rot,nif-parser,nif,game:starfield

**Source**: `docs/audits/AUDIT_NIF_2026-09-29.md`
**Severity**: LOW (doc-rot; behaviour is correct)
**Dimension**: Geometry Handoff
**Game Affected**: Starfield (bsver ≥ 172)
**Location**: `crates/nif/src/import/mesh/tangent.rs` (`synthesize_tangents_yup` doc); `docs/engine/per-game-translation-survey.md` (tangent-path list)

## Description
b9e961eeb (committed as "Enhance physical lighting documentation…") changed `import/mesh/bs_geometry.rs` so that positions, UDEC3 normals, UDEC3 tangents and the bounding-sphere centre all go through `zup_to_yup_pos` at import, and corrected the parser-side field doc (`BSGeometryMeshData.vertices` is now "source mesh-local Z-up units"). Two docs were not updated:
- The `synthesize_tangents_yup` doc still says its Starfield inputs are "positions and normals decoded Y-up by the BSGeometryMeshData parser".
- The survey still lists "UDEC3 Y-up (Starfield BSGeometry)" as one of the tangent paths.

## Evidence
`git diff f97775ca8..HEAD -- crates/nif/src/import/mesh/bs_geometry.rs` removes "Positions are already Y-up (decoded by the BSGeometryMeshData parser)" and "BSGeometry is Starfield-native Y-up". `blocks/bs_geometry.rs` performs no axis swap.

## Impact
None at runtime (the synth's inputs are Y-up by the time it runs). The risk is to a future editor who, trusting "parser decodes Y-up", re-introduces a double or missing basis change.

## Related
#4554 (closed), #1232, #4394.

## Suggested Fix
Reword both docs to "converted to Y-up at import (`import/mesh/bs_geometry.rs`)".

Validated at HEAD 9fcfdc3fc: `tangent.rs` doc still says "decoded Y-up by the BSGeometryMeshData parser"; survey line lists "UDEC3 Y-up (Starfield BSGeometry)".

## Completeness Checks
- [ ] **SIBLING**: other doc sites describing BSGeometry streams as parser-side Y-up
