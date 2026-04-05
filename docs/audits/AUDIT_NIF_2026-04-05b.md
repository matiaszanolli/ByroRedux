# NIF Parser Audit — 2026-04-05b (Post-Refactor + Collision)

**Scope**: `crates/nif/src/` — block parsing, version handling, stream position, import pipeline, coverage
**Auditor**: Claude Opus 4.6 (5 specialist agents)
**Prior audit**: 2026-04-05 (pre-refactor)
**Codebase changes since prior audit**: Major refactoring (import.rs → import/ directory with 6 submodules), new collision module (`import/collision.rs`), `ImportedNode` now carries collision data

## Executive Summary

| Severity | Count |
|----------|-------|
| HIGH | 2 |
| MEDIUM | 10 |
| LOW | 13 |
| INFO | 3 |

**Collision system is functional but incomplete.** The newly added `import/collision.rs` extracts shapes from the NIF scene graph, but rigid body transforms are discarded (NIF-401), the flat import path used by the cell loader drops all collision data (NIF-402), and capsule/cylinder orientations are lost (NIF-407/408).

**Oblivion remains blocked** — constraint and particle blocks cause cascading failures without block_sizes (NIF-515, NIF-517). No change from prior audit.

**FO76/Starfield shader parsing is broken** — flag arrays shift all subsequent fields (NIF-204, NIF-205). No change from prior audit.

**Coverage improved significantly**: 104 fully parsed block types (up from 89), 13 skip-only (down from 30). Collision subsystem now parsed.

**NiTexturingProperty shortfall root cause identified**: Missing `has_texture_transform` in shader texture sub-entries (NIF-302).

## Delta from 2026-04-05 Audit

| Prior Finding | Status |
|--------------|--------|
| NIF-01: NiMorphData missing legacy float keys | Still open |
| NIF-02: Oblivion v20.0.0.5 no block_sizes | Still open |
| NIF-03: read_bool() u8 vs u32 for Oblivion | Still open |
| NIF-04: NiGeometry Oblivion format mismatch | Still open |
| NIF-05: Oblivion string palette not implemented | Still open |
| NIF-06: 30 Havok types skip-only | **IMPROVED** — 17 now fully parsed, 13 remain skip-only |
| NIF-07: FO4/FO76/Starfield shader arrays | Still open |

---

## HIGH

### NIF-515: Havok constraint skip-only types cause cascading failure on Oblivion
- **Severity**: HIGH
- **Dimension**: Coverage
- **Location**: `crates/nif/src/blocks/mod.rs` (skip-only entries)
- **Game Affected**: Oblivion
- **Status**: NEW
- **Description**: 7 constraint types (bhkRagdollConstraint, bhkLimitedHingeConstraint, bhkMalleableConstraint, etc.) are skip-only via NiUnknown. When `block_size` is `None` (Oblivion v20.0.0.5), `NiUnknown::parse` returns `Err`, breaking the parse loop. Since collision blocks appear before geometry in the block list, all geometry after the first unrecognized constraint is unreachable.
- **Suggested Fix**: Implement minimal byte-exact parsers for the 7 constraint types, or detect Oblivion and skip collision subgraph references entirely.

### NIF-517: Particle system blocks cause hard parse failure in Oblivion
- **Severity**: HIGH
- **Dimension**: Coverage
- **Location**: `crates/nif/src/blocks/mod.rs`
- **Game Affected**: Oblivion
- **Status**: NEW
- **Description**: ~15 NiPSys* types have no parser. On games with `block_sizes` these are safely skipped, but Oblivion (no block_sizes) fails hard with no recovery for any NIF containing particle effects.
- **Suggested Fix**: Add NiPSys* to the dispatch table with minimal parsers that consume the correct byte count, or use nif.xml to compute sizes.

---

## MEDIUM

### NIF-201: has_dedicated_shader_refs() excludes FO76 and Starfield
- **Severity**: MEDIUM
- **Dimension**: Version Handling
- **Location**: `crates/nif/src/version.rs:155-157`
- **Game Affected**: FO76, Starfield
- **Description**: Only matches `Fallout4` but nif.xml `BS_GTE_FO4` includes FO76 and Starfield. Currently unused for those games but semantics are wrong.
- **Suggested Fix**: Add `Self::Fallout76 | Self::Starfield` to the match arm.

### NIF-204: BSLightingShaderProperty FO76/Starfield fields misaligned
- **Severity**: MEDIUM
- **Dimension**: Version Handling
- **Location**: `crates/nif/src/blocks/shader.rs:292-310`
- **Game Affected**: FO76, Starfield
- **Description**: For BSVER > 130, variable-length flag arrays shift all subsequent fields. Parser does not consume these flag bytes; UV/texture/emissive reads contain garbage. Block_size correction prevents cascade but all material data is wrong.

### NIF-205: BSEffectShaderProperty FO76 layout divergence not handled
- **Severity**: MEDIUM
- **Dimension**: Version Handling
- **Location**: `crates/nif/src/blocks/shader.rs:648-707`
- **Game Affected**: FO76, Starfield
- **Description**: Same issue as NIF-204 — FO76+ has a different field layout after shader flags. Parser reads Skyrim/FO4 layout, producing garbage.

### NIF-302: NiTexturingProperty shader TexDesc missing has_texture_transform
- **Severity**: MEDIUM
- **Dimension**: Stream Position / Block Parsing
- **Location**: `crates/nif/src/blocks/properties.rs:238-254`
- **Game Affected**: Oblivion (v10.1+)
- **Description**: Shader texture sub-entries do not read `has_texture_transform` (bool) and conditional 32-byte transform data required for `version >= 10.1.0.0`. **Root cause of the previously-noted NiTexturingProperty shortfall.** The shortfall is per-shader-texture, not a fixed 1 byte. For the common case of `num_shader_textures=0`, there is no shortfall.
- **Suggested Fix**: After reading clamp/filter/uv_set/map_id, read `has_texture_transform: bool`. If true, read the 32-byte TexTransform (translation, scale, rotation, method, center).

### NIF-303: BsTriShape vertex size underflow wraps to huge skip
- **Severity**: MEDIUM
- **Dimension**: Stream Position
- **Location**: `crates/nif/src/blocks/tri_shape.rs:347-349`
- **Game Affected**: All (with malformed data)
- **Description**: `vertex_size_bytes - consumed` wraps to a huge usize if `consumed > vertex_size_bytes`, causing seek past end-of-data with confusing EOF error.
- **Suggested Fix**: Guard: `if consumed < vertex_size_bytes { stream.skip(...) } else if consumed > vertex_size_bytes { return Err(...) }`.

### NIF-401: bhkRigidBody translation/rotation discarded in collision import
- **Severity**: MEDIUM
- **Dimension**: Import Pipeline
- **Location**: `crates/nif/src/import/collision.rs:26-53`
- **Game Affected**: All games with dynamic collision objects
- **Description**: `extract_collision()` resolves the shape tree but never applies `body.translation` or `body.rotation`. Static geometry (typically zero offset) is unaffected, but dynamic objects (crates, ragdoll bones) have misaligned collision shapes.
- **Suggested Fix**: Convert body.translation/rotation from Havok coords to engine space and apply to the resolved shape.

### NIF-402: Flat import path (cell loader) discards all collision data
- **Severity**: MEDIUM
- **Dimension**: Import Pipeline
- **Location**: `crates/nif/src/import/walk.rs:93-147`
- **Game Affected**: All games (cell loading path)
- **Description**: `walk_node_flat()` never calls `extract_collision()`. The cell loader at `byroredux/src/cell_loader.rs` uses `import_nif()` (flat), so all collision data from cell-loaded NIFs is silently dropped.
- **Suggested Fix**: Switch cell loader to `import_nif_scene()` (hierarchical), or add collision output to the flat path.

### NIF-508: bhkCompressedMeshShape skip-only (Skyrim collision)
- **Severity**: MEDIUM
- **Dimension**: Coverage
- **Location**: `crates/nif/src/blocks/mod.rs`
- **Game Affected**: Skyrim+
- **Description**: Majority of Skyrim collision uses bhkCompressedMeshShape. Skip-only means most Skyrim collision data is lost.

### NIF-513: bhkNPCollisionObject skip-only (FO4 collision)
- **Severity**: MEDIUM
- **Dimension**: Coverage
- **Location**: `crates/nif/src/blocks/mod.rs`
- **Game Affected**: FO4+
- **Description**: FO4 uses bhkNPCollisionObject (new physics system). Skip-only means all FO4 collision data is lost.

### NIF-516: NiCollisionObject (base class) skip-only
- **Severity**: MEDIUM
- **Dimension**: Coverage
- **Location**: `crates/nif/src/blocks/mod.rs`
- **Game Affected**: Oblivion
- **Description**: Occasionally appears in Oblivion NIFs. Same cascading failure risk as NIF-515 when block_size unavailable.

---

## LOW

### NIF-101: NiSourceTexture direct_render/persist_render_data read as u8, nif.xml says bool
- **Severity**: LOW
- **Dimension**: Block Parsing
- **Location**: `crates/nif/src/blocks/texture.rs:69-75`
- **Game Affected**: FO3/FNV/Skyrim+
- **Description**: These fields are typed `bool` in nif.xml (u32 for v20.2+) but read as u8. Block_size validation for FNV shows no mismatch, suggesting u8 is correct in practice (nif.xml bug). No action needed unless real-file testing shows shortfalls.

### NIF-202: compact_material()/has_emissive_mult() exclude FO76/Starfield
- **Severity**: LOW
- **Dimension**: Version Handling
- **Location**: `crates/nif/src/version.rs:128-142`
- **Game Affected**: FO76, Starfield
- **Description**: Both match FO3NV|SkyrimLE|SkyrimSE|FO4 but not FO76/Starfield. nif.xml gates apply to all games from FO3 onward.
- **Suggested Fix**: Add `Self::Fallout76 | Self::Starfield` to both.

### NIF-206: NiSkinPartition SSE gated on exact bsver==100
- **Severity**: LOW
- **Dimension**: Version Handling
- **Location**: `crates/nif/src/blocks/skin.rs:185,302`
- **Game Affected**: SkyrimSE (hypothetical BSVER 101-129)
- **Description**: `detect()` maps BSVER 101-129 to SkyrimSE, but SSE fields are gated on `bsver==100` exactly. A NIF with BSVER=105 would skip SSE-specific fields.
- **Suggested Fix**: Broaden check to `bsver >= 100 && bsver < 130`.

### NIF-208: has_shader_emissive_color() wrong for FO3 (BSVER < 34)
- **Severity**: LOW
- **Dimension**: Version Handling
- **Location**: `crates/nif/src/version.rs:146-151`
- **Game Affected**: FO3
- **Description**: Flag matches Fallout3NV but FO3 files have BSVER=21-25 where emissive color is absent. No parser currently calls this flag.

### NIF-211: bhkRigidBody body_flags threshold 76 should be 83
- **Severity**: LOW
- **Dimension**: Version Handling
- **Location**: `crates/nif/src/blocks/collision.rs:185`
- **Game Affected**: None in practice (no game uses BSVER 35-75)
- **Suggested Fix**: Change `bsver < 76` to `bsver < 83`.

### NIF-301: bhkSphereShape may miss padding in non-Bethesda Gamebryo files
- **Severity**: LOW
- **Dimension**: Stream Position
- **Location**: `crates/nif/src/blocks/collision.rs:237-241`
- **Description**: Non-Bethesda Gamebryo 2.3 files may have 8-byte padding. Block_size recovery covers this.

### NIF-403: BsTriShape two_sided misses BSEffectShaderProperty
- **Severity**: LOW
- **Dimension**: Import Pipeline
- **Location**: `crates/nif/src/import/mesh.rs:172-179`
- **Game Affected**: Skyrim+
- **Description**: Only checks BSLightingShaderProperty for double-sided flag; misses BSEffectShaderProperty glow meshes.

### NIF-404: BsTriShape duplicates material extraction (~130 lines) instead of using extract_material_info
- **Severity**: LOW (Structural)
- **Dimension**: Import Pipeline
- **Location**: `crates/nif/src/import/mesh.rs:126-258`
- **Description**: Inline material extraction creates parity drift with NiTriShape path. NIF-403 is a concrete example.

### NIF-405: BSShaderPPLightingProperty normal map not extracted
- **Severity**: LOW
- **Dimension**: Import Pipeline
- **Location**: `crates/nif/src/import/material.rs:207-227`
- **Game Affected**: FO3/FNV
- **Description**: Extracts diffuse from textures[0] but not normal map from textures[1].

### NIF-406: NiTexturingProperty bump/normal texture not extracted
- **Severity**: LOW
- **Dimension**: Import Pipeline
- **Location**: `crates/nif/src/import/material.rs:193-205`
- **Game Affected**: Oblivion
- **Description**: Ignores bump_texture (slot 5) and normal_texture (slot 6). Oblivion meshes with normal maps render flat-lit.

### NIF-407: Capsule axis orientation discarded, assumes Y-aligned
- **Severity**: LOW
- **Dimension**: Import Pipeline
- **Location**: `crates/nif/src/import/collision.rs:76-85`
- **Description**: Computes half_height from endpoint distance but discards axis direction. Works when capsule is Z-aligned in Havok (Y after conversion), fails for arbitrary orientations.

### NIF-408: Cylinder axis orientation discarded (same as NIF-407)
- **Severity**: LOW
- **Dimension**: Import Pipeline
- **Location**: `crates/nif/src/import/collision.rs:88-97`

### NIF-410: bhkSimpleShapePhantom transform discarded
- **Severity**: LOW
- **Dimension**: Import Pipeline
- **Location**: `crates/nif/src/import/collision.rs:152-155`
- **Description**: Phantom has a 4x4 transform but only shape_ref is used; phantom's position is lost.

---

## INFO

### NIF-102: NiSkinPartition booleans read as byte, nif.xml says u32 bool — verified correct
- **Severity**: INFO
- **Description**: Tests confirm u8 is correct for FNV. Known nif.xml inconsistency.

### NIF-307: No overflow protection on u32 count-to-usize casts
- **Severity**: INFO
- **Description**: Malicious NIF could trigger OOM but not stream corruption. Defensive only.

### NIF-409: SVD repair only on parent in compose_transforms
- **Severity**: INFO
- **Location**: `crates/nif/src/import/transform.rs:10-28`
- **Description**: Child degenerate matrices only affect translation component; quaternion extraction handles degenerate via its own SVD.

---

## Block Type Coverage Matrix

| Category | Count | Notes |
|----------|------:|-------|
| Fully parsed | 104 | +15 since prior audit (collision + NiPixelData + controllers) |
| Skip-only | 13 | Down from 30 (7 constraints, bhkCompressed*, NiCollisionObject, bhkNP*, bhkPhysics/Ragdoll System) |
| Total registered | 117 | |
| Missing (known needed) | ~20 | NiPSys* family, BSLODTriShape, BSGeometry (Starfield), BSSubIndexTriShape trailing data |

### Per-Game Estimated Coverage

| Game | Geometry | Materials | Collision | Animation | Particles |
|------|----------|-----------|-----------|-----------|-----------|
| FNV | Full | Full | Full | Full | None |
| FO3 | Full | Full | Full | Full | None |
| Skyrim LE | Full | Full | Partial (no compressed mesh) | Full | None |
| Skyrim SE | Full | Full | Partial | Full | None |
| Oblivion | Blocked (no block_sizes + missing types) | Partial | Blocked | Partial | Blocked |
| FO4 | Full (geometry) | Full | None (bhkNP skip) | Full | None |
| FO76 | Geometry only | Garbage (shader misalign) | None | Unknown | None |
| Starfield | None (BSGeometry not parsed) | N/A | None | Unknown | None |

---

## Prioritized Fix Order

### P0 — Oblivion blockers (enables N23.3)
1. Implement 7 constraint type parsers (NIF-515)
2. Add NiPSys* minimal parsers or size calculation (NIF-517)
3. Implement NiCollisionObject parser (NIF-516)

### P1 — Collision correctness
4. Apply bhkRigidBody translation/rotation to collision shapes (NIF-401)
5. Add collision extraction to flat import / switch cell loader to hierarchical (NIF-402)
6. Parse bhkCompressedMeshShape for Skyrim (NIF-508)

### P2 — Material completeness
7. Fix NiTexturingProperty shader texture has_texture_transform (NIF-302)
8. Extract normal maps from BSShaderPPLightingProperty (NIF-405)
9. Extract Oblivion bump/normal textures (NIF-406)
10. Unify BsTriShape material extraction with extract_material_info (NIF-404)

### P3 — Defensive / future games
11. Guard BsTriShape vertex size underflow (NIF-303)
12. Fix version flag coverage for FO76/Starfield (NIF-201, NIF-202)
13. Parse bhkNPCollisionObject for FO4 (NIF-513)
14. Add BSGeometry parser for Starfield (NIF-514)
