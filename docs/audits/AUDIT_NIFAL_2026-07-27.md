# NIFAL Audit — 2026-07-27

Deep audit of **NIFAL** (the NIF Abstraction Layer; spec: [nifal.md](../engine/nifal.md)),
run as a 9-dimension orchestrated sweep (3 concurrent Task agents per batch).
Repo HEAD: `db625997`. Delta base: `ca7a4e0e` (the prior sweep,
[AUDIT_NIFAL_2026-07-25.md](AUDIT_NIFAL_2026-07-25.md), ~40 commits).

**Scope**: all 9 dimensions per `.claude/commands/audit-nifal/SKILL.md`.

**Method**: each dimension was audited independently against the live tree — code
reading plus, where the claim was empirical, **corpus measurement through the real
production code path** (throwaway `crates/nif/examples/` probes driving `parse_nif` +
the actual importer, against vanilla Oblivion / FNV / Skyrim SE / FO4 / FO76 /
Starfield archives; all probes deleted afterward). Targeted `cargo test` runs per
crate, plus the `#[ignore]`-gated `cross_game_translation_completeness` harness.
Working tree left unchanged apart from the pre-existing `composite.frag` edit.

---

## Executive Summary

**This sweep breaks a four-report streak of zero-finding "converged" verdicts.**
24 findings: **3 HIGH, 9 MEDIUM, 12 LOW.**

The two HIGH collision findings are corpus-measured, root-cause candidates for the
two currently-open grounding issues, and have been live for as long as the code has
existed:

- **NIFAL-D6-01** — `resolve_compressed_mesh` divides every chunk index by 3.
  Measured on vanilla Skyrim SE `Meshes0.bsa`: **100% of 22,535 chunks are
  vertex-indexed**, so the divisor is categorically wrong. **505,248 of 2,199,732
  authored collision triangles survive (23%)**, and the survivors connect the wrong
  vertices. 4,415 chunks yield zero triangles; 64 whole meshes end up void.
  `bhkCompressedMeshShape` is Skyrim's *primary* collision format. Strongest available
  root-cause candidate for **#2202**.
- **NIFAL-D6-02** — `bhkBoxShape` runs a *size* vector through the position-space
  `(x, z, −y)` map, negating a lane. **100% of `Cuboid` colliders in Oblivion (1,432),
  FNV (1,149) and Skyrim SE (1,453)** come out with a negative half-extent, which
  Rapier clamps to `1e-3`. An Oblivion 500×27×502 floor slab becomes 500×27×0.002.
  Contributing-cause candidate for **#2193**.
- **NIFAL-D3-01** — `LightKind`, `direction` and `outer_angle` are correctly resolved
  at import and then **discarded**: the canonical `LightSource` has no field to
  receive them, so all four kinds spawn as point lights. Measured across all 8,032
  Oblivion NIFs: 95 `NiDirectionalLight` blocks become full-white omni lights at
  8,192-unit effective range, on ubiquitous placed content (`landscape\vine01.nif`,
  town/Daedric statues, priory doors, every race's ears + hair). The renderer already
  implements spot and directional (`GpuLight.color_type.w`, `lighting.glsl`).

### The systemic finding

Six of this sweep's HIGH/MEDIUM findings share **one failure shape**:

> correctly parsed → correctly resolved at import → **dropped or corrupted at or past
> the boundary** → in a category the spec records as "converged" → with **zero test
> coverage at the drop point**.

Dimension 9 traced this to two verifiable causes:

1. **The convergence audits were call-graph reads, not output measurements.** Proving
   a boundary is *single* never proves its *output is right*. NIFAL-D3-01 is the
   purest case — the boundary is single, the import is correct, and the canonical
   struct simply has no field to receive the value. NIFAL-D6-01/02 are the sharpest —
   the Dimension-6 checklist counts resolve *arms*, and the defect lives inside two
   arms the count declares PASS.
2. **The completeness harness measures the wrong tier.** `MaterialStats::record`
   takes `&ImportedMesh` and never constructs a canonical type — `translate_material`
   is not called anywhere in the harness. It covers 2 of 9 categories, import-half
   only, on a 1–2% single-directory sample, behind floors with ~33 pp median slack.
   **0 of the 6 HIGH/MEDIUM findings were within its reach.** The 2026-07-25
   all-green run was consistent with all six being live and should not have been read
   as convergence evidence.

The spec's stated discipline ("every fix ships a regression pin at both the boundary
**and** the consumer") is real where it was applied — every named regression pin in
this sweep held. The gap is that whole arm bodies were never pinned at all:
`resolve_compressed_mesh` has *zero* tests, and 898/898 `byroredux-nif` lib tests pass
with Skyrim's primary collision format 77% destroyed.

### What held

Every named regression pin, every documented limitation, and the cardinal
`no-render-time-fallback` invariant. Specifically:

- **`if game ==` across the entire shader tree: 0.** All 30 GLSL sources swept
  (`triangle.frag`, all ten `include/*.glsl`, `water.frag`, `composite.frag`,
  `ui.frag`, all compute). 79 per-game token hits, **all inside comments**.
- The **collision arm-set diff is empty**: 16 parsed `Bhk*Shape` structs, 16 resolve
  arms, exactly the #1334 set, with a structural guard test that mechanically
  re-derives it.
- `translate_material` still has **exactly two callers**; the only other `Material {`
  literals are the `--cornell` synthetic harness and test fixtures.
- **9 of 9** `BSLightingShaderProperty` variants forward their trailing data
  (exhaustive no-`_`-arm match). Pre-#343 drop stays closed.
- All **7 parked node/mesh fields** measured at zero canonical consumers.
- Both prior-sweep fixes hold: **MAT-D1-01** (word-boundary keyword match) and
  **NIFAL-D4-01** (`FurnitureMarkerKind` resolved once at the boundary), the latter
  re-verified through `0dcb71b7`'s seat-reservation rework.
- The delta's new **fire-refraction `material_kind` 103** is correctly wired:
  authored SLSF1 discriminator, classified at *import* alongside 101/102, glass gate
  correctly never demotes it, shader ladder auto-generated by `build.rs` and pinned.
  No `GpuInstance` drift across all 5 mirrors.
- `a4c11bfb`'s de-strip de-duplication is genuine, and **flipped collision winding is
  ruled out as the #2202 mechanism** — the collision path now calls the same
  `NiTriStripsData::to_triangles()` as the render path, and `havok_to_engine` is a
  proper rotation (det +1).
- `e3b9b115`'s Starfield gate split verified by re-measurement the commit itself
  couldn't run: Meshes02 **7,552/7,552 clean (100%)**, MeshesPatch 29,843 clean / 6
  truncated (the documented residual).

---

## Per-Category Tier Matrix

| Category | Boundary fn | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|---|
| Material | `material_translate.rs::translate_material` | **PASS** (exactly 2 callers: `nif_loader.rs:900`, `spawn.rs:1166`) — LOW pin gap, MAT-D1-NEW-01 | PASS (emissive no-op stands; MAT-D1-01 fix holds) | PASS — LOW `ior` overload note, MAT-D1-NEW-03 | PASS (renderer reads `m.metalness`/`m.roughness` directly) |
| Geometry / Transform | `coord.rs` + `rotation.rs::sanitize_rotation` (parse-time) + `transform.rs::compose_transforms` | **PASS** — LOW residual copies, NIFAL-D2-01 | PASS | PASS (D2-NEW-02 re-confirmed unreachable, no action) | PASS (`MeshRegistry::upload` generic over `V: Copy`) |
| Skinning | `mesh/skin.rs` (#613 global remap) | N/A | PASS | PASS (global indices only; u16 guard intact; `22798ecc` narrowing verified zero-reader) | PASS |
| **Lights** | `import/walk/mod.rs` → `LightKind` | N/A | **FAIL** — NIFAL-D3-02 (uncited `2048.0`) | **FAIL** — NIFAL-D3-01 (kind/direction/cone discarded) | PASS (zero renderer matches on source block types) |
| Nodes | (by design, no single boundary — spec §2) | N/A (documented; `walk_node_flat` emits no `ImportedNode` at all) | PASS | **FAIL** — NIFAL-D4-02 (`billboard_mode` dropped on the cell path) | N/A |
| Furniture (Nodes sub-cat) | `references/attach.rs::furniture_component` | PASS (1 site) | PASS | PASS (D4-01 fix holds through `0dcb71b7`) | N/A |
| Particles | `systems/particle.rs::apply_emitter_overlays` | **PASS** for the four named overlays (2 callers) — LOW bypass for 3 more, NIFAL-D5-01 | PASS (`initial_color` still unapplied; size-curve still deferred) | PASS | PASS (force fields converted once at overlay time) |
| **Collision** | `import/collision/shape.rs::resolve_shape_inner` | **FAIL** — NIFAL-D6-02 (canonical value contradicts authored data) | **FAIL** — NIFAL-D6-01 (uncited `/3` divisor) | **FAIL** — NIFAL-D6-03, -D6-04 | N/A |
| Animation | `anim_convert.rs::convert_nif_clip` | PASS (6 callers of one fn — correct) — LOW table dup, NIFAL-D7-03 | **FAIL** — NIFAL-D7-01 (fabricated 1.0 s duration) | PASS (B-spline not era-gated; text-keys wired; morph-weights reach a real component) | PASS |
| Shader flags / texture sets | `shader_flags.rs` + `import/material/dedicated_shader.rs` | PASS (block-type dispatch) | **FAIL** — NIFAL-D8-01 (synthesized threshold outranks authored BGSM) | PASS (9/9 variants forward; FO4 flag chain intact) | **PASS** (whole shader tree: **0** `if game ==`) |
| *Completeness harness* | `crates/nif/tests/translation_completeness.rs` | — | — | — | — · **FAIL as a verification instrument** — NIFAL-D9-01, -D9-02, -D9-03 |

Doc-drift confirmed (carried into NIFAL-D6-06 / -D7-02 / -D8-02): `resolve_shape` /
`resolve_shape_inner` live in `collision/shape.rs`, not `collision/mod.rs`; the #1592
FO4-flag merge lives in `dedicated_shader.rs`, not `walker.rs`.

---

## Findings

### HIGH

#### NIFAL-D6-01 — `resolve_compressed_mesh` divides chunk indices by 3; 77% of Skyrim's authored collision geometry is destroyed

- **Dimension**: Collision · **Tier Violated**: `no-fabrication` + `no-leak`
- **Game Affected**: Skyrim LE/SE + all DLC/CC (every game shipping
  `bhkCompressedMeshShape`). Oblivion/FO3/FNV unaffected; FO4+ never reaches this arm.
- **Location**: [shape.rs:498-535](../../crates/nif/src/import/collision/shape.rs#L498-L535)
  (divisor at `:505-507` list path, `:527-529` strip path)

The code asserts *"Havok chunk indices reference into the flat u16 vertex component
array (pre-multiplied by 3)"*. Measured against vanilla `Skyrim - Meshes0.bsa` (18,862
NIFs, every non-empty chunk):

```
non-empty chunks:                                    22535
  all indices %3 == 0 (component-indexed):               0
  max index in [n, 3n]  (component-indexed):             0
  max index  <  n       (VERTEX-indexed):            22535   ← 100%
```

`meshes\architecture\riften\rttempleplazafloorr01.nif` — a *floor* tile — has one
chunk of 24 vertices / 24 sequential indices, a clean 8-triangle soup. After `/3`
every triangle becomes `(5,5,5)`, `(4,4,4)`, … — all degenerate, all filtered by the
`a != b && b != c && a != c` guard. Resolved output: `TriMesh(v=24, tris=0)`.

| Archive / subtree | chunks | tris **today** | tris **direct** | chunks yielding **0** |
|---|---|---|---|---|
| `Skyrim - Meshes0.bsa` | 22,535 | **505,248** | 2,199,732 | **4,415 (19.6%)** |
| `Skyrim - Meshes1.bsa` | 1,607 | 48,339 | 174,104 | 190 (11.8%) |
| …`meshes\dungeons\nordic\*` (Bleak Falls Barrow tileset) | 1,717 | **49,650** | 197,335 | **299 (17.4%)** |

`nif.xml:2629-2631` documents `Num Vertices` as "multiplied by 3" with
`Vertices length="Num Vertices #DIV# 3"` — the ×3 is on the **vertex count**, and the
parser already divides it out (`compressed_mesh.rs:172-174`). `Indices` is documented
plainly as "Vertex indices as used by strips", with no ×3. The importer
double-applies a divisor already consumed at parse. The comment's "Confirmed via
Havok source" citation attaches to the vertex count, not the index encoding.

**Relation to #2202**: when all of a NIF's chunks void out (64 whole meshes in
Meshes0), `extract_collision` still returns `Some(TriMesh { indices: [] })`, which
sets `collisions_empty = false` ([spawn.rs:331](../../byroredux/src/cell_loader/spawn.rs#L331))
and **suppresses the `synthesize_static_trimesh` fallback** (`spawn.rs:1381`), then
degrades to `SharedShape::ball(1e-3)` (`crates/physics/src/convert.rs:162-166`). Net:
a placed architecture REFR with a 0.001-unit point collider and no fallback. On the
Bleak Falls Barrow tileset specifically, 75% of collision surface is missing.

**Fix**: drop the `/3` at both sites; add a `resolve_compressed_mesh` unit test from
the `rttempleplazafloorr01` chunk shape plus a corpus assertion that no vanilla SSE
mesh resolves to a zero-triangle `TriMesh`. Verify against nifly's
`bhkCompressedMeshShapeData` reader before landing (no-guessing).

#### NIFAL-D6-02 — `bhkBoxShape` half-extents run through the position-space Z-up→Y-up map, negating one lane

- **Dimension**: Collision · **Tier Violated**: `single-boundary`
- **Game Affected**: **ALL** — Oblivion, FO3, FNV, Skyrim LE/SE
- **Location**: [shape.rs:139-144](../../crates/nif/src/import/collision/shape.rs#L139-L144);
  consumed at `crates/physics/src/convert.rs:117-125`

`havok_to_engine(x, y, z) == (x, z, -y)` is correct for **positions**; applied to a
**half-extent** it emits `half_extents.z = -hy * scale`, negative for every authored
`dimensions[1] > 0`. The axis permutation is right; only the sign is wrong.

| Archive | `Cuboid` colliders | with a **negative** half-extent |
|---|---|---|
| `Oblivion - Meshes.bsa` | 1,432 | **1,432 (100%)** |
| `Fallout - Meshes.bsa` (FNV) | 1,149 | **1,149 (100%)** |
| `Skyrim - Meshes0.bsa` | 1,453 | **1,453 (100%)** |

The sole consumer clamps with `.max(1e-3)`, so an Oblivion 500×27×502 floor slab
becomes 500×27×0.002 and an FNV 32×1.4×32 platform becomes 32×1.4×0.002. Every box
collider in every supported game is a paper-thin sheet along a *horizontal* axis — an
actor grounds on a 2-thousandth-of-a-unit sliver at one edge and misses the rest of
the footprint. Plausible contributor to **#2193**. The only existing `BhkBoxShape`
translate test checks the `NaN` guard.

**Fix**: `.abs()` the mapped vector; unit-test `dimensions: [1,2,3]` →
`half_extents == (1,3,2) * scale`, all positive. Add a `debug_assert!` on non-negative
`Cuboid` half-extents in `convert.rs` so the clamp can never silently absorb a sign
error again. Sibling *position* uses of `havok_to_engine` are legitimately signed and
need no change.

#### NIFAL-D3-01 — `LightKind` / direction / cone resolved at import, then discarded

- **Dimension**: Skinning/Lights · **Tier Violated**: `parked-not-leak`
- **Game Affected**: **Oblivion (severe, measured)**; FO76 (2 spot blocks); all games
  structurally
- **Location**: canonical `LightSource` (`crates/core/src/ecs/components/`), consumed
  by `spawn_nif_lights`

`LightKind` (Ambient/Directional/Point/Spot), `direction` and `outer_angle` are
resolved correctly at import and then have nowhere to go — the canonical `LightSource`
carries no kind/direction/cone field, so `spawn_nif_lights` emits a point light for
all four kinds. This is **not** a "renderer doesn't support it yet" deferral:
`GpuLight.color_type.w` already documents `0=point/1=spot/2=directional` and
`lighting.glsl:80-101,300-315` implements both.

Probed all 8,032 NIFs in Oblivion's `Meshes.bsa` through the production import path:
**95 `NiDirectionalLight` blocks, all `color=[1,1,1]`, all radius 0** → all pass the
colour gate → all become full-white omni lights with an 8,192-unit effective range
(radius 0 → 4096 fallback × `LIGHT_RANGE_EXTENSION` 2.0). Affected models are
ubiquitous placed content: `landscape\vine01/02.nif`, `clutter\key\key.nif`,
town/Daedric statues, `priory\priorydoor01.nif`, three Oblivion citadel interior kits,
every race's ears + hair meshes. FNV/FO3 measurably unaffected (their ambient blocks
are all zero-colour and filtered out).

The prior sweep's "Lights PASS" holds for *what it checked* — zero renderer matches on
the source block types (re-confirmed: 8 hits outside `crates/nif/`, all comments and
test fixtures). The drop on the **other side** of the boundary was never checked.
`nifal.md` §2 records Lights as "converged", which is now half-stale.

**Fix**: add `kind` / `direction` / `outer_angle` to the canonical `LightSource` and
populate them at the spawn boundary; wire `GpuLight.color_type.w` from `kind`.

---

### MEDIUM

#### NIFAL-D4-02 — `billboard_mode` silently dropped on the cell-loader path

- **Dimension**: Nodes · **Tier Violated**: `parked-not-leak`
- **Game Affected**: **all cell-loaded games** — Oblivion, FO3, FNV, Skyrim SE, FO4, Starfield
- **Location**: [import.rs:196-200](../../byroredux/src/cell_loader/references/import.rs#L196-L200)

`placement_root_billboard: None` is hardcoded for every NIF; the flat walker
(`walk_node_flat`) never produces an `ImportedNode`, so the value never leaves the
parser on that path. The loose path works (`scene/nif_loader.rs:454-456`);
`spawn.rs:416-418` is only ever reached for `.spt` placeholders. `billboard_system`
**is** live in the scheduler (`boot.rs:877`), running each frame over an empty
NIF-sourced set. Measured prevalence on live archives: **550** FNV / **1,527** Skyrim
Meshes0 / **517** FO4 / **213** Oblivion / **5** Starfield `NiBillboardNode`
instances, all parsing clean.

`nifal.md:102-104` asserts the opposite ("consumed at the spawn sites") — which is why
four prior sweeps restated it PASS from the spec rather than the code. #994 is CLOSED
and covered only the `.spt` half.

**Fix**: propagate the nearest-ancestor billboard mode onto `ImportedMesh` in
`walk_node_flat` and attach per-mesh at `spawn.rs:1134`, reusing the #1235 `flags`
parity mechanism.

#### NIFAL-D6-03 — a geometrically-void `TriMesh` returns `Some(...)`, suppressing the documented fallback

- **Dimension**: Collision · **Tier Violated**: `no-leak`
- **Game Affected**: ALL structurally; observed today on Skyrim SE via NIFAL-D6-01
- **Location**: `shape.rs:403-405`, `:447-449`, `:540-542`

All three `TriMesh` resolvers guard only on **vertices**; `all_indices` is never
checked. The NIFAL contract at this boundary is "`Some` ⟹ a usable collider exists",
and the consumer is written to it — so a void `Some` is strictly *worse* than `None`:
it produces no collider **and** blocks the recovery mechanism designed for exactly
this case. Measured: 64 whole meshes in `Skyrim - Meshes0.bsa` resolve this way today.
The primitive/hull arms already model the right behaviour (`shape.rs:181-183`).

**Fix**: add `|| all_indices.is_empty()` to the three guards. Cheap, and it converts
any future variant of D6-01 from a silent hole in the floor into a coarse collider.

#### NIFAL-D6-04 — `CmsChunk` strip-chunk trailing triangle-list indices parsed then dropped

- **Dimension**: Collision · **Tier Violated**: `no-leak`
- **Game Affected**: Skyrim LE/SE (+ DLC/CC) · **Location**: `shape.rs:513-535`

`nif.xml:2633` documents `Strips` as "Length of strips **longer than one triangle**" —
`Indices` holds the strip runs *followed by* a plain triangle list. The importer walks
only `chunk.strips` and abandons `chunk.indices[sum(strips)..]`. Measured on
`Meshes0.bsa`: 19,208 of 21,722 strip chunks (88%) have a residual, totalling
**761,904 unconsumed indices, 100% divisible by 3** — confirming the trailing block is
a plain triangle list, and that ~253,968 additional authored triangles are dropped on
top of the D6-01 loss.

**Fix**: after the strip loop, decode `chunk.indices[idx_offset..]` as a plain triangle
list. Fix alongside D6-01 — same function, shared fixture.

#### NIFAL-D2-02 — `resolve_compressed_mesh` chunk-strip walk panics on `sum(strips) > indices.len()`

- **Dimension**: Geometry/Transform (cross-cuts Collision) · **Tier Violated**: none (robustness)
- **Game Affected**: Skyrim SE / FO4 / FO76 · **Location**: `shape.rs:515-534`

The strip walk clamps the slice *end* but not the *start* (`idx_offset = end`). With
`sum(strips) > indices.len()` and ≥2 strips, the next iteration builds an inverted
range and panics — reproduced standalone. The parser reads `num_indices`/`num_strips`
independently with no tying invariant. Every sibling malformed-geometry path in this
module degrades to `None` (#1779/#1409/#1385); this one aborts the cell load.

**Fix**: clamp `idx_offset` to `indices.len()` (or `break` on overrun), matching the
sibling degrade-to-`None` discipline. Bundle with D6-01/D6-04.

#### NIFAL-D3-02 — the `2048.0` no-attenuation fallback is an uncited constant that *is* the shipped behaviour

- **Dimension**: Skinning/Lights · **Tier Violated**: `no-fabrication`
- **Game Affected**: FNV (measured 82/82), FO3 (same path, 27 spawnable), FO4, Starfield
- **Location**: [walk/mod.rs:1637-1660](../../crates/nif/src/import/walk/mod.rs#L1637-L1660)

A probe over FNV's 14,881 NIFs shows **82 of 82** spawnable point lights receive
exactly `2048.0` — the quadratic/linear attenuation-solve branches are dead on vanilla
content, so the invented constant is the operative radius for all of it. The engine
already works around it (`spawn.rs:497-500` prefers the ESM radius), but that covers
only LIGH REFRs. Contrast `LIGHT_RANGE_EXTENSION`, which carries its OpenMW citation —
that is the model to follow.

**Fix**: cite a source for `2048.0` (Gamebryo 2.3 `NiLight` default, or a measured
derivation), or replace it with one. Per `feedback_no_guessing`, ask for the reference
rather than picking a nicer-looking number.

#### NIFAL-D7-01 — embedded-clip duration ignores transform-channel key times

- **Dimension**: Animation · **Tier Violated**: `no-fabrication`
- **Game Affected**: all — the inline-transform-controller shape is Oblivion/FO3/FNV loose content
- **Location**: [entry.rs:558-579](../../crates/nif/src/anim/entry.rs#L558-L579)

`import_embedded_animations` computes duration from float/colour/bool/texture-flip
channels only — it never walks `clip.channels`, the **transform** channels added by
the #1440 inline-transform-controller arm (`entry.rs:333-355`). A transform-only
embedded clip therefore gets the fabricated `1.0` fallback, and the loop wrap
(`player.rs:100-107`) truncates a 4-second authored fan/door/lift rotation to its
first second.

The #1440 test (`anim/tests/channel.rs:660-757`) asserts `duration ≈ 1.0` with the
comment "duration follows the last key time" — but its fixture's last key is *at* 1.0,
so it passes for the wrong reason and would still pass with the transform channels
removed entirely.

**Fix**: extend the `max_time` scan over `clip.channels`; change the fixture's last key
to a non-1.0 time.

#### NIFAL-D8-01 — synthesized FO4 alpha-test threshold (128/255) blocks the authored BGSM value

- **Dimension**: Shader-flags/Effects · **Tier Violated**: `no-fabrication`
- **Game Affected**: Fallout 4 (and any BSVER ≥ 130 content pairing a NIF F4SF2 bit-25 with a BGSM)
- **Location**: [material.rs:1038-1042](../../byroredux/src/asset_provider/material.rs#L1038-L1042)

The #1985-seeded `128/255` threshold gates on `!mesh.alpha_test`, which is **not**
chain-local — it arrives pre-set from the NIF F4SF2 bit-25 path
(`dedicated_shader.rs:283`). Every payload-carrying sibling in that loop uses a
chain-local sentinel, and the BGEM sibling at `:1152` overwrites unconditionally;
BGSM is the outlier. This inverts the priority that #1592's own comment states (the
NIF flag is strictly lower-priority than the BGSM merge, which should OR-upgrade).

Reachability on *vanilla* FO4 is unmeasured and flagged as such.

**Fix**: add a `set_alpha_test` chain-local sentinel so the authored BGSM
`alpha_test_ref` wins.

#### NIFAL-D9-01 — alphabetical truncation collapses each game's sample to one directory

- **Dimension**: Completeness · **Tier Violated**: (harness gap — no production tier)
- **Game Affected**: all seven · **Location**: `translation_completeness.rs:110,236-237`

`files.sort(); truncate(200)` means Skyrim samples **1.06% of its corpus, 100% from
`meshes\actors\`**, and Oblivion **2.49%, 100% from `meshes\architecture\`**. The
cross-game comparison the harness exists to make is confounded by content class, not
game. Every large fill-rate divergence chased this sweep (Oblivion `nrm=0%`,
Starfield `tex=0%`, FO76 `tex=9.6%`) resolved to correct-by-format or sample artifact —
**no unverified-game leak was found** — but that conclusion required manual work the
harness should have made unnecessary.

**Fix**: stratified sampling (round-robin across top-level directories) before
truncation.

#### NIFAL-D9-02 — the harness measures the raw tier; no canonical type is ever constructed

- **Dimension**: Completeness · **Tier Violated**: (harness gap)
- **Game Affected**: all seven · **Location**: `translation_completeness.rs:145,224-254`

`MaterialStats::record` takes `&ImportedMesh`. `translate_material` is never called.
Two of its columns read fields whose own doc comments say "the renderer never sees
it," measured *before* the BGSM/CDB merge that supplies FO4/FO76/Starfield PBR. It
covers 2 of 9 NIFAL categories, import-half only. This is the mechanical reason the
harness could not have caught any of this sweep's six HIGH/MEDIUM translate findings.

Root cause is a crate-graph constraint — `crates/nif` sits below `byroredux`, where
`translate_material` lives.

**Fix**: add a canonical-tier sibling harness in `byroredux/tests/` that drives
`translate_material` (and, as they gain boundaries, the other categories) and asserts
on canonical-tier output, not `Imported*` fill rates.

---

### LOW

| ID | Dimension | Tier | Summary |
|---|---|---|---|
| **MAT-D1-NEW-01** | Material | `single-boundary` | The nif crate writes `material_kind` 101/102/**103** as bare literals; the claimed "pin" (`lighting_shader_pbr_tests.rs:180`) is a literal-to-literal assert *inside the producing crate*. Zero tests anywhere tie the importer's values to `byroredux_renderer::MATERIAL_KIND_*`. A renumber would keep renderer↔shader in lockstep and nif tests green while silently dropping every effect / no-lighting / fire-haze surface to the default lit arm, on every game. **Independently corroborated by Dim 8.** Fix: two-line cross-crate assert in `byroredux`. |
| **MAT-D1-NEW-02** | Material | (consumer-guard asymmetry) | `draw_command_eligible_for_tlas` (`predicates.rs:437`) carries the "even if a producer accidentally sets `in_tlas`" belt for `EFFECT_SHADER` but not `FIRE_REFRACTION`, despite the latter's own constant doc requiring the same exclusion. No live defect; defence-in-depth gap. |
| **MAT-D1-NEW-03** | Material | `no-leak` (minor) | Canonical `Material::ior` now carries a discriminated overload (distortion strength when `kind == 103`) documented only at the producer and in the shader, not on the canonical field. Blast radius currently zero. |
| **NIFAL-D2-01** | Geometry | `single-boundary` | `#2193`'s de-strip dedup is incomplete: two hand-written strip-parity copies survive — `resolve_compressed_mesh` (`shape.rs:519-524`), written in the **exact convention the commit retired**, and the `NiSkinPartition` loop (`blocks/skin.rs:309-318`). Verified orientation-equivalent today (odd permutations, cyclic rotations of each other), so latent — but precisely the drift the commit exists to prevent, and neither residual is covered by the new parity test. |
| **NIFAL-D4-03** | Nodes | (doc) | `nifal.md:243` still lists `BSFurnitureMarker` as "parsed, not walked into `Imported*`, blocked on AI sit/lean/sleep packages" — stale in *both* columns since M41.5/M42. §2 is the dedup baseline sweeps read to skip verification; a row claiming a live category is unconsumed is exactly the failure mode that hid D4-02. |
| **NIFAL-D5-01** | Particles | `single-boundary` | `texture_path` / `src_blend` / `dst_blend` are authored emitter overrides on the same `ImportedParticleEmitter` struct, but folded by an identical 9-line block copy-pasted at `nif_loader.rs:551-559` and `spawn.rs:619-627`, *outside* `apply_emitter_overlays` — whose own doc claims it folds "every authored emitter override". Structurally the pre-#1513 state for a different field subset. Not cosmetic: the blend pair selects the additive-vs-alpha-over batch-merge path and `texture_path` gates whether the emitter spawns at all. Zero live impact (the two blocks are identical). |
| **NIFAL-D6-05** | Collision | `parked-not-leak` | `CmsChunk.transform_index` + `chunk_transforms` parsed then dropped. 27% of chunks (6,118/22,535) carry a non-zero `transform_index`, but all **12,069** parsed transform entries decode as exact identity and zero chunks are geometry-less back-references — a genuine measured no-op, just nowhere recorded as a deliberate park. No code change; ledger entry. |
| **NIFAL-D6-06** | Collision | (doc) | `crates/nif/src/import/collision.rs` was split under #1876 but is still cited in **9 places** across `nifal.md` ×2, `physal.md` ×3, `nif-parser.md`, `physics.md` ×2, `per-game-translation-survey.md`, and `_audit-common.md` — two as markdown links that resolve to nothing. Additionally `nifal.md:185` still claims **13** shape variants; live count is **16**. SKILL.md Dim-6 attributes `resolve_shape`/`resolve_shape_inner` to `collision/mod.rs` (path exists, so `_audit-validate.sh` passes) but they live in `collision/shape.rs`. |
| **NIFAL-D7-02** | Animation | `parked-not-leak` | Record corrected: morph-weight channels **do** reach a canonical component at HEAD (`systems/animation.rs:273-280` → `AnimatedMorphWeights`, scheduler write declared at `boot.rs:710`). The prior sweep's report said so, but `nifal.md:206-207` — the authoritative spec — was never updated, and SKILL.md:222 mirrors the stale claim. The genuinely parked item is the *GPU morph-blend consumer*, not the translation. Per-light **ambient** is still genuinely parked (explicitly discarded at `systems/animation.rs:182-194`). |
| **NIFAL-D7-03** | Animation | `single-boundary` (secondary) | The `operation`→`FloatTarget` and `target_color`→`ColorTarget` discriminator tables are duplicated between the KF arm (`channel.rs:382-389`, `:295-300`) and the embedded arm (`entry.rs:358-365`, `:378-383`). Byte-identical today. **Not a duplicate of #2067**, which covers the parse-side prologue in `crates/nif/src/blocks/`. |
| **NIFAL-D8-02** | Shader-flags | (doc) | `nifal.md:215-217` and SKILL.md:227 both cite a `ShaderFlags<'a>` typed view that `9a9a4c5d` (#1897/#1914) **deleted as dead code**, and describe the bit-collision guards as "compile-time asserts" when they are `#[test]` runtime asserts. Risks a future sweep reading the deliberate removal as a regression. |
| **NIFAL-D9-03** | Completeness | (harness gap) | Fill-rate floors carry ~33 pp median slack — FO4 `mat_path` could halve (77%→40%) and stay green. `metO`/`rghO` sit pinned at 100% on all seven games with **no assertion**; `normal_map` is asserted for no game. |

---

## Documented-limitation ledger (parked-not-leak / no-action — do NOT re-report next sweep)

Re-verified against HEAD `db625997`:

- **`D2-NEW-02`** (`AUDIT_NIFAL_2026-05-30.md`, LOW, no tier violated): the defensive
  second SVD-repair path inside `coord.rs::zup_matrix_to_yup_quat`
  (`svd_repair_to_quat`) is unreachable in production. **Hardened this sweep**: the one
  caller that does *not* come from the node walk (`precombine.rs:76` ←
  `BSPackedGeomDataCombined.transform`) was traced to `read_ni_transform_struct`, so
  *every* production input is sanitized. `sanitize_rotation` has exactly two production
  callers, both in `stream.rs`. No action.
- **Node/mesh parked fields** — all 7 (`bs_value_node`, `bs_ordered_node`,
  `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index`) measured
  at **zero** canonical consumers; every non-test hit is producer-side. `nifal.md` §2
  passthroughs (`ImportedTextureEffect` dead extractor, `NiSwitchNode` identity,
  `BSInvMarker`, `bs_bound` loose-path-only) re-verified as documented. `BSFurnitureMarker`
  is the one row that has gone stale — see NIFAL-D4-03.
- **Collision**: `BhkPlaneShape` → `None` (#1334, documented at its arm);
  `BhkNPCollisionObject` (FO4/FO76/Starfield `BhkSystemBinary` blob — separate project,
  falls back to `synthesize_static_trimesh`); `BhkPCollisionObject` + `BhkSimpleShapePhantom`
  / `BhkAabbPhantom` (trigger volumes, need a `TriggerVolume` ECS path, #1363);
  Havok material/layer filters parsed-not-translated (produces *over*-collision, never
  missing collision). **New**: `CmsChunk.transform_index` / `chunk_transforms`
  (NIFAL-D6-05, measured all-identity across 12,069 vanilla SSE entries).
- **Particles**: `initial_color` intentionally unapplied (re-confirmed empirically —
  all 5 sampled FNV FX NIFs report `[1,1,1,1]`); size-over-life *curve* documented
  future work; multi-emitter scene-first attribution is **#1402, CLOSED as a
  documented deferral** (reproduced live — all 7 emitters in `explosionlarge01.nif`
  share one row); legacy `NiParticleSystemController` unconsumed fields measured
  zero-corpus (#2090/#1327).
- **Animation**: per-light **ambient** colour channels genuinely parked;
  `AnimationTextKeyEvents` is produced, registered and drained but no system reads the
  labels (footsteps are distance-driven); `NiGeomMorpherController` is chain-walked on
  the embedded path with no extraction arm there (zero impact until a morph consumer
  lands). Morph-weight channels are **no longer parked** — see NIFAL-D7-02.
- **Material / shader flags**: emissive scale is a **measured no-op** (§4) — a
  normalization constant would be a `no-fabrication` violation; `BSEffectShaderProperty.base_color_scale`
  diffuse-tint render path deferred via `EmissiveSource::Effect`, not dropped;
  `material_kind: u32` deliberately kept as the GPU dispatch contract. **New**:
  fire-refraction (kind 103) is **unreachable on FO76/Starfield** — format-absent, not
  a leak (nif.xml's 32-entry `BSShaderCRC32` has no `Fire_Refraction` identifier and
  the typed flag word is zeroed for BSVER ≥ 132). Synthesising one would be a
  `no-fabrication` violation.
- **Skinning**: `body_part_flags` parked, zero consumers. `22798ecc`'s narrowing of
  skinned-vertex output to positions verified zero-reader — RT hit shading reads the
  global bind-pose SSBO (`include/ray_hit.glsl:22-51`), so the deleted normal/tangent
  lanes had no consumers.
- **Pre-existing open issues confirmed still accurate, not regressed, not duplicated**:
  `#2109`, `#2108` (Starfield BGEM / EFFECT_PALETTE), `#2099`, `#2098` (Starfield UV /
  bound), `#1827` (Starfield BSGeometry bone data), `#1981` (skinned WorldBound vs
  ragdoll), `#2067` (NiSingleInterpController prologue), `#1576` (model-less Starfield
  forms), `#1856` (closed by `595a1898` — documentation + pinning test, **no** runtime
  per-game branch).

## Non-finding note

The working tree carries an uncommitted edit to
`crates/renderer/shaders/composite.frag:381` that hard-zeroes `causticLum`. It has no
bearing on NIFAL, but it is an un-recompiled WIP stub (the `.spv` is unmodified) that
would kill all caustics if committed as-is.

## Method note

All corpus numbers come from throwaway `crates/nif/examples/` probes driving the real
`parse_nif` + importer code paths (not re-implementations), against vanilla Oblivion /
FNV / Skyrim SE `Meshes0`+`Meshes1` / FO4 / FO76 / Starfield archives. All seven games
had data on disk; none were skipped. Counterfactual columns ("direct index", "strip
residual") re-implement only the arithmetic under test. All probe files were deleted.
