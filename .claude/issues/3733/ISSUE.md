# #3733 — NIFAL-2026-08-30-D1-02: ESM-sourced water planes are outside the #2444 boundary guard

**Severity**: LOW · **Location**: `byroredux/src/cell_loader/water.rs` (`spawn_water_plane`, `spawn_lod_water_plane`)
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-30.md` (NIFAL-2026-08-30-D1-02)

#2444 established "every drawn surface's canonical material is produced at
one boundary" and pinned it with the source-scan guard
`every_exterior_spawner_inserts_a_boundary_material` — whose own table
listed five spawners. `cell_loader/water.rs` is a sixth cell-loader
spawner that inserts a `MeshHandle` (both the CELL water plane and the LOD
water plane) and attaches `WaterPlane`/`WaterMaterial` but no canonical
`Material`, and carried no documented exemption. Since water reuses the
same `collect_static_mesh_draws` path (`render/water.rs` looks up an
already-emitted `DrawCommand` and flips `is_water`), these entities were
interned with the hardcoded-literal 11-tuple (`roughness 0.5`, `metalness
0.0`, …) — bounded today because `water.frag` never reads those fields, but
a real exposure on the RT/secondary path (`triangle.frag` reads
`hitMat.roughness`/`metalness`/`ior` generically).

## Fix implemented

Per the issue's first suggested option: routed both water spawn sites
through `translate_texture_only_material` (they each have exactly one bound
texture path — the resolved water normal map) and insert the resulting
canonical `Material` right after each site's `MeshHandle` insert, matching
the pattern every other listed spawner (`terrain.rs`, `terrain_lod.rs`,
`object_lod.rs`, `terrain_lod_btr.rs`) already uses for populations with no
source material record.

Both sites needed a `.clone()` of `normal_texture_path: Option<String>`
before the pre-existing texture-resolve `if let Some(path) =
normal_texture_path` consumed it by value.

**SIBLING** (issue's own checklist item, and the one that matters most
here): rather than adding a sixth row to the hand-maintained table — the
exact pattern that let both `terrain_lod_btr.rs` (#3336) and this issue
recur — rewrote `every_exterior_spawner_inserts_a_boundary_material` to
scan every root-level `cell_loader/*.rs` file at test time
(`std::fs::read_dir`, `_tests.rs` siblings excluded as test-only mocks) and
require every file that inserts a `MeshHandle` to also call one of the two
boundary functions. The `spawn/`/`references/` subdirectories are
correctly out of scope — `spawn/mesh_instance.rs` always attaches its
canonical `Material` via `translate_material`/`attach_mesh_water` upstream
of any `MeshHandle` insert, a structurally different, already-covered path.
A future seventh spawner with no boundary call now fails this test on its
own, without anyone needing to remember to extend a list.

**CANONICAL-BOUNDARY** (issue's own checklist item): the fix produces the
material at the boundary (`translate_texture_only_material`, which itself
owns no scalar literals — every value comes from `Material::default()` or
the shared `resolve_pbr` classifier), not a render-time re-derivation.

**TESTS** (issue's own checklist item — "the existing source-scan guard is
the test; it must fail before the fix and pass after"): verified directly —
`git stash`d just the `water.rs` change, reran the rewritten guard, watched
it fail with exactly the expected message naming `water.rs`, then restored
the fix and reran to confirm it passes.

Full workspace: `cargo test --no-fail-fast` 7059 passing, 0 failing
(unchanged count — the guard test itself was rewritten, not added new).
