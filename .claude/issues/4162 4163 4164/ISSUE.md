# Batch: #4162, #4163, #4164

## #4162 — NIF-D3-2026-09-11-02: nif-parser.md's dispatch-arm/type-name counts are internally inconsistent and stale
**Severity**: LOW · **Location**: `docs/engine/nif-parser.md:305-306,658`

Doc's dispatch-arm/type-name counts disagree across sections (~248/~315, 254/310, 251/315) and
against the fresh 255/312 measured this session. Also undercounts nested `type_name_static`
re-derivation match blocks as two when there are four. FO76 coverage row also stale (98.18%
"pending #3461" — #3461 is closed at 100.0000%).

Fix: Refresh counts to 255/312, fix "two" → "four" nested matches, refresh FO76 row.

## #4163 — NIF-D5-2026-09-04-03: bhkPlaneShape parses fully but hard-resolves to None with no measured coverage
**Severity**: LOW · **Location**: `crates/nif/src/import/collision/shape.rs:102-104`

Design decision (no half-space CollisionShape variant, don't approximate as solid Cuboid) is
sound but unmeasured — nothing distinguishes "no bhkPlaneShape present" from "one present and
intentionally dropped."

Fix: Add a counter to `summarize_collision_authoring` (or equivalent measured assertion).

## #4164 — NIF-D5-2026-09-04-04: BhkConvexListShape reads HavokMaterial via raw read_u32_le
**Severity**: LOW · **Location**: `crates/nif/src/blocks/collision/shape_compound.rs:175`

Zero behavioral effect today (this block type's version scope never reaches
`read_havok_material`'s version-dependent gate), but inconsistent with every sibling shape parser.

Fix: Swap raw `read_u32_le()` for the shared `read_havok_material` helper.
