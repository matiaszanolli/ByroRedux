# D6-01: bhkPackedNiTriStripsShape per-axis Scale parsed but dropped at translate

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1777
**Labels**: low, nif-parser, nif, bug
**Severity**: LOW
**Dimension**: Collision (NIFAL canonical translation — `/audit-nifal`)
**Tier Violated**: `no-leak` (parsed `Imported`-tier field never consumed at translate — the canonical NIFAL parsed-then-dropped leak class)
**Game Affected**: FO3 / FNV / Skyrim+ (any game using packed-strips collision); manifests only on non-identity per-shape scale
**Location**: `crates/nif/src/blocks/collision/shape_mesh.rs:55,93` (field stored) → `crates/nif/src/import/collision.rs:754-758` (dropped)
**Source**: `docs/audits/AUDIT_NIFAL_2026-06-28.md` (D6-01, NEW)

## Description
`BhkPackedNiTriStripsShape` parses and **stores** its per-axis `Scale` Vector4
(`pub scale: [f32; 4]`, `shape_mesh.rs:55`; read at `:93`, with `_scale_copy`
discarded at `:95`). The resolve arm at `collision.rs:757` calls
`resolve_packed_mesh(data, scale)` passing only `scene.havok_scale` — the stored
`s.scale` is never read in the resolve path (the only `.scale` reads in
`collision.rs` at `:745-746` belong to `BhkMeshShape`, not the packed shape).
`resolve_packed_mesh` (`collision.rs:871`) applies only the uniform `havok_scale`.

## Evidence
- `reference/nifxml/nif.xml` `bhkPackedNiTriStripsShape` (line 3193) carries
  `Scale type="Vector4" default="#VEC4_1110#"` plus a `Scale Copy` ("Same as scale").
  The struct field is live (not `_`-prefixed) yet has no consumer.
- Contrast `BhkMeshShape`, whose authored per-axis Scale **is** folded in
  (`collision.rs:745-750`).
- Sibling-but-distinct: `BhkNiTriStripsShape.Scale` is read as `_scale` and discarded
  at `shape_mesh.rs:31` — same low-impact identity-default field, also pre-existing.

## Impact
Low. The packed scale defaults to `(1,1,1,0)` and vanilla Bethesda content authors it
as identity virtually universally — the world scale is carried by `havok_scale`. Only a
modded/custom packed-strips shape with a genuinely non-identity per-shape scale would
render mis-sized collision. Pre-existing (predates #1744 — the
`resolve_packed_mesh(data, scale)` call traces to `42aef192`/`75474e71`, not the recent
commits). Surfaced by the shape-by-shape scale diff that #1744 prompted.

## Suggested Fix
Pass `s.scale` into `resolve_packed_mesh` the same way `BhkMeshShape` folds its Scale
(with the same finite/non-zero guard that falls back to identity), or document the drop
with a cited justification if packed scale is provably always identity in target content.
Fold (or document) both `bhkPackedNiTriStripsShape` and the sibling
`bhkNiTriStripsShape.Scale` together.

## Completeness Checks
- [ ] **SIBLING**: `BhkNiTriStripsShape.Scale` (`shape_mesh.rs:31`, discarded as
  `_scale`) gets the same treatment (fold or document) — and `BhkMeshShape`'s existing
  fold (`collision.rs:745-750`) is the reference pattern
- [ ] **CANONICAL-BOUNDARY**: the per-shape scale stays folded at the NIFAL collision
  translate boundary (`import/collision.rs::resolve_shape` / `resolve_packed_mesh`) —
  never re-applied by a downstream consumer. Cross-ref `/audit-nifal`
- [ ] **TESTS**: a regression test pins the fold (mirror `mesh_shape_folds_per_axis_scale`)
  — e.g. `packed_tristrips_shape_folds_per_axis_scale` asserting a non-identity authored
  scale reaches the resolved `TriMesh` vertices, with an identity-default no-op case
