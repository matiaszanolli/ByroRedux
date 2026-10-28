# SKY-D1-01: decode_sse_skin_payload's contract doc, including the #613 "no partition remap" invariant, is spliced onto the SseSkinPayload type alias

Labels: low, nif-parser, nif, documentation, doc-rot, game:skyrim, legacy-compat
Source: docs/audits/AUDIT_SKYRIM_2026-10-27.md

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-27.md` finding `SKY-D1-01` (Dimension 1, BSTriShape packed geometry) · Severity LOW (doc-rot) · Status NEW · Game: Skyrim SE.

**Location**: `crates/nif/src/import/mesh/skin.rs:634-645` (`decode_sse_skin_payload`, `SseSkinPayload`)

## Description
The doc block that opens "Resolve `shape.skin_ref` → `NiSkinPartition` → `SseSkinGlobalBuffer` …" ends with "The caller widens the packed indices without partition remapping." It then runs straight into "Decoded SSE skin payload: per-vertex `(weights, skin-global bone indices)`". All of it attaches to `pub type SseSkinPayload`. `pub fn decode_sse_skin_payload` itself has no doc. The type alias was inserted between the doc and the fn (32ec26bf4), and the no-remap line was added later on top (8c834e0be).

## Evidence
Current source:
```rust
/// Resolve `shape.skin_ref` → `NiSkinPartition` → `SseSkinGlobalBuffer` ...
///
/// The caller widens the packed indices without partition remapping.
/// Decoded SSE skin payload: per-vertex `(weights, skin-global bone
/// indices)`, returned by [`decode_sse_skin_payload`].
pub type SseSkinPayload = (Vec<[f32; 4]>, Vec<[u8; 4]>);

pub fn decode_sse_skin_payload(scene: &NifScene, shape: &BsTriShape) -> Option<SseSkinPayload> {
```
`git blame -L 634,646` shows the 014adc886 fn doc, then the 8c834e0be line, then the 32ec26bf4 alias doc and alias, then the fn.

## Impact
The one load-bearing statement against re-introducing the #613/#2577 partition remap renders on a type alias, not on the function it constrains. `core::source_scan::spliced_doc_blocks` (#6373) only flags docs that open with a sibling's name, so it cannot see this.

## Related
#6373, EXT-D6-2026-10-27-04 (same blind spot).

## Suggested Fix
Move the alias, with its own two-line doc, above the fn doc block.

## Completeness Checks
- [ ] **SIBLING**: Other `pub type` aliases inserted between a doc and its fn in `crates/nif/src/import/` checked
- [ ] **TESTS**: The spliced-doc scan (or a targeted source-shape test) pins that the no-remap sentence sits on `decode_sse_skin_payload`

