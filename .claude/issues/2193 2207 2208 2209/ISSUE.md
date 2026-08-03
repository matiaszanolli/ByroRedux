# Issues 2193, 2207, 2208, 2209

## #2193 (HIGH) — OBL-2026-07-25-01: is_grounded stays false at Oblivion interior spawn (ICMarketDistrictTheGildedCarafe) — jump unconditionally broken

**Location**: `crates/nif/src/import/collision/shape.rs:354` (`resolve_tri_strips_data_refs`, suspected root); consumed at `byroredux/src/systems/character.rs:195,219,335`; `crates/physics/src/components.rs:107`

Follow-up to #2013 (closed). After the #2013 spawn-positioning fix, Oblivion no longer free-falls at spawn, but `is_grounded` never flips true while resting on solid architecture at `ICMarketDistrictTheGildedCarafe`. A diagnostic probe found the resting contact's surface normal inverted (dot-up ≈ -0.99), suspected wrong-winding collision triangle from the NiTriStrips-based Oblivion collision import path. Not yet isolated to a specific mesh/line. `is_grounded` gates jump availability and the gravity-integration branch — this is a real player-control bug, not cosmetic.

**Suggested fix**: isolate winding-order divergence between Oblivion's NiTriStrips collision import and the shared FO3/FNV path (which grounds correctly); fix without regressing correctly-oriented floors elsewhere. Needs live Vulkan + real Oblivion data to *observe* the symptom, but a unit test can pin correct winding on the parsed mesh data.

## #2207 — NIFAL-D6-03: a geometrically-void TriMesh returns Some(...), suppressing the synthesized-collider fallback

**Location**: `crates/nif/src/import/collision/shape.rs:403-405, :447-449, :540-542`

All three `TriMesh` resolvers guard only on vertices (`all_verts.is_empty() || !finite`), never checking `all_indices`. A mesh with valid vertices but an empty index list returns `Some(TriMesh { indices: [] })` — a void collider that's worse than `None`, since it blocks `spawn.rs`'s `synthesize_static_trimesh` fallback (gated on `collisions_empty == false`). Measured: 64 whole meshes in `Skyrim - Meshes0.bsa` hit this via NIFAL-D6-01.

**Fix**: add `|| all_indices.is_empty()` to all three guards.

## #2208 — NIFAL-D6-04: CmsChunk strip-chunk trailing triangle-list indices parsed then dropped (761,904 indices on Skyrim Meshes0)

**Location**: `crates/nif/src/import/collision/shape.rs:513-535`

`nif.xml` documents chunk `Strips` as strip runs *followed by* a plain triangle list in `Indices`. The importer only walks `chunk.strips` and abandons `chunk.indices[sum(strips)..]`. Measured: 88% of strip chunks in Skyrim Meshes0.bsa have a residual, 100% divisible by 3 (confirming trailing plain-triangle-list), totalling 761,904 unconsumed indices / ~253,968 dropped triangles.

**Fix**: after the strip loop, decode `chunk.indices[idx_offset..]` as a plain triangle list using the same degenerate-skip guard.

## #2209 — NIFAL-D2-02: resolve_compressed_mesh chunk-strip walk panics on sum(strips) > indices.len()

**Location**: `crates/nif/src/import/collision/shape.rs:515-534`

The chunk-strip walk clamps the slice *end* (`end.min(chunk.indices.len())`) but not `idx_offset` itself before the next iteration. With `sum(strips) > indices.len()` and ≥2 strips, the next iteration builds an inverted range (`start > end`) and panics. Every sibling malformed-geometry path in this module degrades to `None` (#1779/#1409/#1385); this is the outlier that aborts cell load instead.

**Fix**: clamp `idx_offset` to `indices.len()` (or `break` on overrun) after each strip.

## Domain classification
- All four: **nif** (byroredux-nif) — #2193's investigation also touches byroredux binary (character.rs) and byroredux-physics, but the suspected root and required fix live in the NIF collision import path.
- #2207, #2208, #2209 share the exact same function (`resolve_compressed_mesh` / chunk-strip walk in `shape.rs`) — fix together, same fixture family.
