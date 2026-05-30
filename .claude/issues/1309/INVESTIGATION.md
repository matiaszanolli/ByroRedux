# Investigation — #1309 (title/body mismatch; two doc-rot findings)

## Title/body mismatch
#1309's **title** is `OB-D7-001` (stale doc: `Material::resolve_classifier_overrides`,
renamed to `resolve_pbr`). Its **body** is a different finding, `OBL-D4-NEW-04`
(stale wireframe comment in `pipeline.rs`) — which is *also* filed separately as the
OPEN **#1315**. Same title-swap class as #1304. Both findings are LOW/documentation,
both confirmed real, both fixed here.

## Finding 1 — OB-D7-001 (the title) — CONFIRMED
`byroredux/src/material_translate.rs:59` doc-comment linked
`[Material::resolve_classifier_overrides]`, but the method was renamed to `resolve_pbr`
(the code calls `material.resolve_pbr()` at :152; `pub fn resolve_pbr` lives at
`material.rs:591`). The old link was a broken intra-doc reference. Fixed → `[Material::resolve_pbr]`.
SIBLING: grep for `resolve_classifier_overrides` across `crates/`, `byroredux/`,
`.claude/commands/` → no other references.

## Finding 2 — OBL-D4-NEW-04 (the body, = #1315) — CONFIRMED
`pipeline.rs:246-255` claimed wireframe (#869) is unimplemented: "no pipeline variant
routes to `vk::PolygonMode::LINE` yet … the deferred fix ships `WireframeOpaque
{ two_sided }`". This contradicts the code right below it. Verified wireframe IS fully
wired:
- `PipelineKey::Opaque { wireframe: bool }` / `Blended { …, wireframe }` (pipeline.rs:60/67)
- `polygon_mode(vk::PolygonMode::LINE)` variant built at pipeline.rs:347, gated on
  `fillModeNonSolid` (`opaque_wireframe: Option<vk::Pipeline>`, pipeline.rs:136)
- `context/draw.rs:1589-1599` selects the key from `draw_cmd.wireframe`
The implementation superseded the comment's deferred design (it uses a `wireframe` bool
on the existing keys, not a separate `WireframeOpaque` type). Rewrote the comment to
describe the live wiring + the `fillModeNonSolid`-fallback.
SIBLING (per the body's checklist): scanned `audit-renderer.md` for the same stale
wireframe / `PolygonMode::LINE` / #869 framing → none present.

## Scope / impact
Comment-only, 2 files (`material_translate.rs`, `pipeline.rs`). No behavior change, no
SPIR-V (both are Rust comments). Workspace builds clean; the new `[Material::resolve_pbr]`
intra-doc link resolves (`resolve_pbr` is `pub`).

## Completeness checks
- **UNSAFE / DROP / LOCK_ORDER / FFI / CANONICAL-BOUNDARY**: N/A (doc-only).
- **SIBLING**: both checked clean (no other `resolve_classifier_overrides`; no stale
  wireframe framing in audit-renderer.md).
- **TESTS**: none — no behavior change (matches the issue's own checklist).

## Closes
#1309 (title OB-D7-001 + body OBL-D4-NEW-04) and the duplicate **#1315** (OBL-D4-NEW-04).
