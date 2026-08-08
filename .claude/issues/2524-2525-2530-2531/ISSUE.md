# Issues 2524, 2525, 2530, 2531

Four audit findings across two domains:
- #2524 → **binary** (`byroredux`) — MEDIUM — NifImportRegistry LRU eviction drops freed clip handles in precombined path
- #2525 → **nif** (`byroredux-nif`) — MEDIUM — 3 sites bypass the bulk-read-then-map idiom
- #2530 → **binary** (`byroredux`) — HIGH — loose-NIF load path never extracts/spawns lights
- #2531 → **binary** (`byroredux`) — MEDIUM — packed-Havok proxy unions skinned bind-pose geometry

---

## #2524 — PERF-D3-NEW-01: NifImportRegistry LRU eviction drops freed AnimationClipRegistry handles in the precombined-mesh insert path

**Severity**: MEDIUM · **Dimension**: GPU Memory Pressure & Eviction Thrash
**Location**: `byroredux/src/cell_loader/precombined.rs:313-316`
**Status**: NEW (fresh reintroduction of the #863 bug class at a call site added 2026-08-04, `9e5540899` — not a regression of the original fix, whose three original call sites remain correct)

### Description
`NifImportRegistry::insert` returns `Vec<u32>` — the `AnimationClipRegistry` handles of any entries the 2048-cap LRU sweep evicted as a side effect of this insert — and is marked `#[must_use = "evicted clip handles must be released into AnimationClipRegistry to free their keyframe arrays — see #863"]`. Four of the five production call sites forward the returned handles to `AnimationClipRegistry::release`. The precombined-mesh commit path does not:
```rust
// byroredux/src/cell_loader/precombined.rs:313-316
{
    let mut reg = world.resource_mut::<NifImportRegistry>();
    let _freed = reg.insert(path.clone(), parsed.clone());
}
```
Binding the `#[must_use]` return to a named variable (`_freed`, not the bare `_` discard) satisfies both the `must_use` and `unused_variables` lints, so the compiler gives no warning — the exact silent-drop shape #863's original bug had before the `Vec<u32>` contract was added.

### Evidence
Confirmed directly at `precombined.rs:313-316`. `AnimationClipRegistry::release` is what actually clears a slot's channel collections — skipping it leaves those collections (and their backing allocations) resident indefinitely. The precombine path's own inserted entry never itself owns a clip handle, but the LRU sweep triggered by *this* insert can evict any other cache entry once the registry is at cap (2048 default, or `BYRO_NIF_CACHE_MAX`), including animated NIFs registered via the three correctly-forwarding call sites — whichever victim the sweep picks, if it owned a clip handle, that handle is silently dropped here instead of released.

### Impact
A slow CPU-RAM leak (not VRAM) — bounded by `AnimationClipRegistry`'s slot count growing without corresponding frees, gated on (a) FO4 precombined-mesh content being loaded (M49), (b) the `NifImportRegistry` LRU cache being at its cap, and (c) the evicted victim happening to be an animated NIF with a registered clip handle. In a long FO4 session that revisits precombine-heavy cells repeatedly, this compounds the same way #863 originally did, just through a narrower door.

### Related
#863 (original fix, three-of-four-then-correct call sites), #544 (clip_handles map cleanup on eviction).

### Suggested Fix
Mirror `partial.rs:69`'s pattern — capture the returned `Vec<u32>` as `freed` (not `_freed`), and after the block, if non-empty, forward each handle to `world.resource_mut::<AnimationClipRegistry>().release(h)`.

### Completeness Checks
- [ ] **TESTS**: A regression test forces an LRU eviction during a precombine-path insert and confirms the evicted entry's clip handle is released
- [ ] **SIBLING**: All five `NifImportRegistry::insert` call sites forward the returned handles consistently

---

## #2525 — PERF-D8-NEW-02: Three per-element decode loops bypass the crate's own established bulk-read-then-map idiom for half-float/quaternion arrays

**Severity**: MEDIUM · **Dimension**: NIF Parse Performance
**Location**: `crates/nif/src/blocks/extra_data.rs:377-385` (`BsPositionData::parse`), `crates/nif/src/blocks/node.rs:1080-1088` (`BsDistantObjectInstancedNode::parse`, transforms), `crates/nif/src/blocks/legacy_particle.rs:624-638` (`NiLegacyParticlesData::parse`, rotations)
**Status**: NEW

### Description
#1263 (NIF-D5-NEW-03) and #2032 (PERF-D8-01) both established the same fix shape for "array needs a per-element transform the raw bytes don't carry" (half-float decode, byte-swizzle, etc.): bulk-read the raw fixed-width values in one `read_*_array` call, then `.chunks_exact(k).map(transform).collect()`. Three call sites never got the memo and still do `allocate_vec` + a per-element loop of individual `read_u16_le()`/`read_f32_le()` calls: `BsPositionData::parse` (per-vertex half-float blend-factor array, FO4/FO76 cloth/dismemberment), `BsDistantObjectInstancedNode::parse` transforms (`Vec<[f32; 16]>`, 16 individual `read_f32_le()` calls per transform instead of one bulk read + `chunks_exact(16)`), and `NiLegacyParticlesData::parse` rotations (reads `w,x,y,z` and reorders to `[x,y,z,w]` per quaternion, could bulk-read + swizzle in the `.map()`). None of these are per-frame (all are one-time import-side parses, cached after first load), so the impact is bounded CPU overhead on cell-load / streaming-worker latency, not steady-state frame time.

### Evidence
Confirmed directly at all three locations — each still does a per-element read loop instead of the bulk-read-then-map idiom established at `crates/nif/src/blocks/bs_geometry.rs:410-446`.

### Impact
Extra per-element call overhead on the NIF-parse critical path for cell load / exterior streaming (a budget-bound path). Scales with vertex/instance count on FO4/FO76 cloth meshes and Starfield distant-object-instancing nodes; bounded by real-world content sizes, so this is a throughput/latency inefficiency rather than a correctness or memory-safety issue. dhat allocation-bound tests can't catch this class (allocation *count* is identical either way — the difference is N extra function-call/bounds-check/cursor-advance round trips instead of one bulk `read_exact`).

### Related
#1263 (NIF-D5-NEW-03, the original 3-site fix in `bs_geometry.rs`), #2032 (PERF-D8-01, the `BoneWeight` sibling fix in this exact dimension); the `node.rs` transforms site is also cited under PERF-D8-NEW-01 (this session) for its separate allocation-bound issue — fixing the bulk-read shape here does not by itself fix that finding, both changes are complementary.

### Suggested Fix
Apply the same `read_*_array(count * k)?.chunks_exact(k).map(transform).collect()` shape at all three sites, mirroring `bs_geometry.rs:421-446`. For `node.rs`'s `[f32;16]` case, use `read_f32_array(count * 16)?.chunks_exact(16).map(|c| c.try_into().unwrap())`. Since dhat bounds can't catch this class, propose a wall-clock or read-call-count regression test alongside the fix.

### Completeness Checks
- [ ] **TESTS**: A wall-clock or read-call-count regression test pins the bulk-read shape at all three sites (dhat allocation-count tests can't catch this class)
- [ ] **SIBLING**: All three sites converted consistently to the established idiom

---

## #2530 — NIFAL-D3-NEW-01: Loose-NIF load path never extracts or spawns any of a mesh's authored lights

**Severity**: HIGH · **Dimension**: Lights · **Tier Violated**: single-boundary / no-fabrication (the extraction call is *absent* on one of the two production load paths, not a bad translation of present data)
**Game Affected**: All (Oblivion → Starfield) — every loose-loaded NIF carrying an embedded `NiPointLight` / `NiSpotLight` / `NiAmbientLight` / `NiDirectionalLight`
**Location**: `byroredux/src/scene/nif_loader.rs` (entire file, 1165 lines — `parse_import_and_merge` / `load_nif_bytes_with_skeleton`)
**Status**: NEW — `gh issue list` search for "light"/"nif_loader" found only closed `#156`, which added the extraction+spawn path used by the cell loader only, not this one. Not a duplicate.

### Description
`byroredux_nif::import::import_nif_lights` — the sole function that walks a parsed `NifScene` and produces `Vec<ImportedLight>` — has exactly three call sites in the whole tree: `crates/nif/examples/import_probe.rs:47` (debug example), `byroredux/src/streaming.rs:895` (exterior grid pre-parse), and `byroredux/src/cell_loader/references/import.rs:116` (cell-loader ref import). `byroredux/src/scene/nif_loader.rs` — the module backing `cargo run -- path/to/mesh.nif` (documented in `CLAUDE.md`'s Quick Reference/Usage as a primary invocation, and the cache path behind *all* skeleton/body/hand NPC-part loading) — calls neither `import_nif_lights` nor a light-populating path. `grep -in light byroredux/src/scene/nif_loader.rs` returns zero matches across the full file, and `world.insert(entity, LightSource ...)` never appears in it — the only `LightSource` insertion site in the whole repo is `byroredux/src/cell_loader/spawn.rs:779`, unreachable from the loose loader.

### Evidence
```
$ grep -rn "import_nif_lights\b" --include='*.rs' crates/nif byroredux
crates/nif/src/import/mod.rs:483:pub fn import_nif_lights(scene: &NifScene) -> Vec<ImportedLight>
crates/nif/examples/import_probe.rs:47:    let lights = byroredux_nif::import::import_nif_lights(&scene);
byroredux/src/streaming.rs:895:        let lights = byroredux_nif::import::import_nif_lights(&scene);
byroredux/src/cell_loader/references/import.rs:116:    let lights = byroredux_nif::import::import_nif_lights(&scene);

$ grep -in "light" byroredux/src/scene/nif_loader.rs
(no output)
```

### Impact
A torch, candle, lantern, or streetlamp NIF loaded standalone (`cargo run -- <mesh>.nif`) renders its flame/bulb geometry but contributes zero light to the scene — visible content loss, not cosmetic. Since `load_nif_bytes_with_skeleton`'s cache path backs *every* skeleton/body/hand NPC-part load (not just the standalone entry point, per that function's own doc comment), the blast radius extends to normal cell-loaded NPC rendering wherever NPC-part NIFs carry lights, though the most directly observable case is the documented loose-load workflow.

### Related
Sibling gap to closed `#156` (which fixed the cell-loader path only). Not a duplicate of any open issue.

### Suggested Fix
Call `byroredux_nif::import::import_nif_lights(&scene)` in `parse_import_and_merge`, store the result on the loader's cache-entry struct, and add a light-spawn loop in `load_nif_bytes_with_skeleton` mirroring `cell_loader/spawn.rs::spawn_nif_lights` — widen `is_spawnable_nif_light`/`light_radius_or_default` to `pub(crate)`-shared (they already are `pub(crate)` in `spawn.rs`) or lift them to a shared helper rather than re-deriving the sanitization logic a third time.

### Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: The loose-loader's light spawn goes through the same `LightSource` construction shape as the cell-loader path, not a fourth divergent one
- [ ] **TESTS**: A regression test loads a standalone NIF with an embedded light and confirms a `LightSource` entity spawns

---

## #2531 — NIFAL-D6-NEW-01: synthesize_packed_havok_proxy unions skinned-mesh bind-pose geometry into the compatibility AABB, unlike its Architecture-trimesh sibling

**Severity**: MEDIUM · **Dimension**: Collision · **Tier Violated**: NIFAL translate boundary (canonical-fallback tier — the compatibility-proxy consumer introduced this cycle by `716b7ee9`/`8ee151e0`, not the raw/translate tiers proper)
**Game Affected**: FO4 / FO76 / Starfield — any `RenderLayer::Actor` (CREA — creature) or `RenderLayer::Clutter` placement with packed (`BhkNPCollisionObject`) collision authoring and a skinned render mesh
**Location**: `byroredux/src/cell_loader/spawn.rs:118-135` (`synthesize_packed_havok_proxy`'s mesh filter), contrasted with `byroredux/src/cell_loader/spawn.rs:1680-1687` (the sibling `ArchitectureTriMesh` gate, which requires `mesh.skin.is_none()`)
**Status**: NEW — brand-new code path (landed `8ee151e0`, this delta window). Not a duplicate; sibling of the same-cycle-fixed `#2355` (that issue was "no proxy at all" for Clutter/Actor; this is "proxy built from the wrong pose data" once the sibling fix landed).

### Description
The Architecture trimesh fallback (`synthesize_static_trimesh`) explicitly excludes skinned meshes (`mesh.skin.is_none()`, "never synthesize for animated bodies"). `synthesize_packed_havok_proxy` has no equivalent check:
```rust
let geometry = meshes
    .iter()
    .filter(|mesh| {
        !mesh.material.is_decal
            && !mesh.material.alpha_test
            && mesh.material.material_kind
                != byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION
            && !mesh.positions.is_empty()
    })
    .map(|mesh| ProxyMeshGeometry { positions: &mesh.positions, ... });
```
`mesh.positions` on a skinned `ImportedMesh` is bind-pose (T-pose/rest-pose) local geometry — the same array GPU skinning deforms at render time, not a runtime-posed shape. Creature (CREA) REFRs on FO4+/FO76/Starfield reach `spawn_placed_instances` through the generic REFR path (`npcs: &HashMap<u32, NpcRecord>` is keyed by NPC_ only — CREA is absent, so it falls through to `spawn_synth_child` → `spawn_placed_instances` with `base_layer = RenderLayer::Actor`), so a creature whose model is a skinned mesh and whose NIF authors only packed Havok gets its collision cuboid built from bind-pose vertex positions.

### Evidence
No test in either commit constructs an `ImportedMesh` with `skin: Some(...)` through this path — both new tests (`packed_proxy_bakes_outer_scale_into_cuboid_extent`, `packed_proxy_is_keyframed_and_parented_to_visual_placement`) use `ImportedMesh::from_geometry(...)`, which defaults `skin: None`. The gap is untested as well as unguarded.

### Impact
A bind-pose T-pose skeleton for many creature/character rigs has limbs splayed far wider than the resting silhouette, so the resulting `Cuboid` half-extents can be substantially oversized relative to the creature's visible footprint — an invisible collision block extending well beyond the rendered model, obstructing movement in open space around the creature. The proxy is `Keyframed` and parented to `placement_root` (not any bone), so it never reflects animated posture — the mis-sizing is permanent for the creature's lifetime, not a spawn-frame transient. Scoped to skinned creature/actor content on FO4+/FO76/Starfield, exactly the population most likely to lack decoded classic collision.

### Related
Sibling of fixed `#2355`.

### Suggested Fix
Either (a) add a `mesh.skin.is_none()` filter to the closure, matching the Architecture precedent, and fall back to each mesh's already-computed `local_bound_center`/`local_bound_radius` (pose-independent, mesh-local) for skinned submeshes instead of dropping the creature to "unresolved"; or (b) use the authored `local_bound_center`/`local_bound_radius` directly for skinned submeshes rather than raw bind-pose vertex positions — preserves the "conservative coarse box" intent without trusting bind-pose extremities as representative.

### Completeness Checks
- [ ] **TESTS**: A regression test constructs an `ImportedMesh` with `skin: Some(...)` through `synthesize_packed_havok_proxy` and confirms it either falls back to the local bound or is excluded
- [ ] **CANONICAL-BOUNDARY**: The fallback fix stays a spawn-time decision (per-REFR, once), not re-derived per-draw
