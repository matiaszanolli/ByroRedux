# Issue #982

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/982
**Title**: NIF-LOW-BUNDLE-2026-05-12: 20 LOW + INFO findings from NIF audit (doc / telemetry / niche-version / alias-loss / defence-in-depth)
**Labels**: documentation, nif-parser, low
**Audit source**: docs/audits/AUDIT_NIF_2026-05-12.md

---

**Source**: `docs/audits/AUDIT_NIF_2026-05-12.md` (all 6 dimensions; LOW/INFO findings only)
**Severity**: LOW (bundle — individual findings cap at LOW or INFO)
**Bundle pattern**: mirrors #926 REN-LOW-BUNDLE-2026-05-09

20 LOW/INFO findings grouped here to keep the issue tracker focused. Most are 1-3 LOC each. Bundle into a single hygiene PR or close as won't-fix per the per-finding rationale below.

## Group A — Doc / Comment drift (3)

### NIF-D1-NEW-02: `NiMorphData.has_legacy_weight` comment says `BSVER in 0..=11` but gate is `< 10`
- **Location**: `crates/nif/src/blocks/controller/morph.rs:181-187`
- **Status**: doc-only — gate is correct per nif.xml `vercond="#BSVER# #LT# 10"`. Comment misleads future readers.
- **Fix**: Update comment to `BSVER < 10` (i.e. `bsver in 0..=9`; vanilla Oblivion at bsver=11 is correctly excluded).

### NIF-D2-NEW-09: `bsver()` doc-comment calls 130/155/172 "canonical" but FO4 ships 130/132/139
- **Location**: `crates/nif/src/version.rs:160-189`
- **Fix**: Document FO4 spans `{130, 132, 139}`, FO76 spans 152..=167, Starfield is unbounded ≥168. Or rename method to `canonical_retail_bsver()`.

### NIF-D2-NEW-05: `bhkRigidBody.body_flags` threshold uses 83, nif.xml says 76
- **Location**: `crates/nif/src/blocks/collision.rs:348-356`
- **Status**: zero-shipping impact (no Bethesda title ships in bsver 76-82 gap). Doctrine deviation only — comment is also self-contradictory.
- **Fix**: Change `bsver < 83` to `bsver < 76`; fix the inverted-rationale comment; add bsver=75 / bsver=76 boundary tests.

## Group B — Niche version-condition gates (3)

### NIF-D2-NEW-06: `NiSourceTexture.Is Static` gated `>= 5.0.0.1` but nif.xml has no `since=`
- **Location**: `crates/nif/src/blocks/texture.rs:100-104`
- **Status**: out-of-tier (Morrowind V4 isn't M-tier support). Latent — drifts 1 byte on Morrowind-era source-texture blocks.
- **Fix**: Drop the version gate; read unconditionally. Add Morrowind-era regression test if pursued.

### NIF-D2-NEW-07: `has_material_crc` / `has_shader_alpha_refs` skip BSVER 35-82 `Unknown` corner
- **Location**: `crates/nif/src/version.rs:211-268` (predicates), used at `crates/nif/src/blocks/tri_shape.rs:123, 1394`
- **Status**: no shipping retail content lives in this gap. Forward-compat hygiene against future Bethesda dev-tool builds.
- **Fix**: Replace `variant().has_shader_alpha_refs()` with `stream.bsver() > 34` and similarly for `has_material_crc()`. Mirror precedent set by `has_properties_list` at `base.rs:103`.

### NIF-D4-NEW-09: Pre-4.1.0.12 `NiZBufferProperty.z_function` read unconditional
- **Location**: `crates/nif/src/blocks/properties.rs:842-848`, consumed at `crates/nif/src/import/material/walker.rs:448-457`
- **Status**: latent — no shipping content pre-4.1.0.12 (all Bethesda is v10+).
- **Fix**: Gate the `z_function` read on `version >= V4_1_0_12`.

## Group C — Telemetry gaps (3)

### NIF-D3-NEW-05: Dispatch-level `Ok(NiUnknown)` recovery bumps total but skips per-type rollup
- **Location**: `crates/nif/src/lib.rs:441-443`
- **Status**: operators reading the `recovered N block(s) via …: {per_type_rollup}` warn can't tell which unknown types are flooding.
- **Fix**: Add `bump_counter(&mut recovered_by_type, type_name);` alongside the `recovered_blocks += 1;` at line 442. One-line change.

### NIF-D3-NEW-06: Havok constraint stubs excluded from `drift_histogram` — #939's signal blind to ~45 stubbed under-reads per skeleton load
- **Location**: `crates/nif/src/lib.rs:366-373` + `is_havok_constraint_stub` at `lib.rs:127-139`
- **Status**: intentional noise-control omission, but not surfaced anywhere. Future audit running `nif_stats --drift-histogram` against skeleton-heavy content will conclude constraints parse cleanly when in fact they're systematically reconciled.
- **Fix**: Either populate `drift_histogram` unconditionally (filter post-hoc), or add `stubbed_drift_histogram` field for visibility without polluting the real signal.

### NIF-D3-NEW-07: `_group_id` over-read on v10.0.x dispatch-fallback theoretically possible
- **Location**: `crates/nif/src/blocks/mod.rs:144-146` interaction with `mod.rs:1000-1014` and `lib.rs:343-444`
- **Status**: unreachable with shipping content — block_sizes table is gated `>= 20.2.0.5`, so the combination can't arise. Defence-in-depth only.
- **Fix**: Move `_group_id` consumption out of `parse_block` prologue into per-subclass NiObject-base parser. Defer unless content surfaces.

## Group D — Importer cascade / ordering subtleties (4)

### NIF-D4-NEW-04: Shape-level `collision_ref` dropped on NiTriShape / BsTriShape / BSGeometry
- **Location**: `crates/nif/src/import/walk.rs:328-394, 582-650`
- **Status**: most Bethesda content attaches `bhkCollisionObject` to parent NiNode; Oblivion + some FO3 modded content puts it on the NiTriShape directly. Shape-level collision silently disappears.
- **Fix**: Mirror the NiNode `if let Some(ref mut coll_out) = collisions { ... }` pattern into each shape branch.

### NIF-D4-NEW-05: Parent NiAlphaProperty silently overwrites shape's src/dst blend factors
- **Location**: `crates/nif/src/import/material/walker.rs:441-445`
- **Status**: legacy alpha-property cascade — shape-level NiAlphaProperty with flags=0 doesn't set `alpha_blend`/`alpha_test`, so the gate stays open and the parent inherits.
- **Fix**: Track "any NiAlphaProperty consumed" separately from `alpha_blend`/`alpha_test` state, gate on that. Or skip `apply_alpha_flags` for properties after the first.

### NIF-D4-NEW-06: BSEffectShader implicit `alpha_blend = true` not cleared by explicit opaque NiAlphaProperty
- **Location**: `crates/nif/src/import/material/walker.rs:413-422` + 427-431
- **Status**: undocumented asymmetry. Arguable as intentional (BGEM is authoritative) but the implicit→explicit transition is silent.
- **Fix**: Defer the implicit `alpha_blend = true` until after the `alpha_property_ref` branch runs, with an explicit "BGEM owns transparency" comment if intentional.

### NIF-D4-NEW-07: BSGeometry inline-LOD path aborts when LOD0 is External even if LOD2+ are Internal
- **Location**: `crates/nif/src/import/mesh.rs:962-996`
- **Status**: currently dead defensive code (parser invariant guarantees Internal-only when the flag is set), but fragile to format quirks.
- **Fix**: Iterate `for m in &shape.meshes` for stage A (mirror stage B), picking first `Internal` LOD; fall back to stage B (external) only when no Internal LOD resolves.

## Group E — Modded-content corner cases (1)

### NIF-D4-NEW-08: Legacy NiVertexColorProperty silently overrides Skyrim+ shader-flag vertex-color intent on mixed content
- **Location**: `crates/nif/src/import/material/walker.rs:835-838`
- **Status**: vanilla Skyrim+ never collides (statics don't ship legacy NiVertexColorProperty alongside BSLightingShaderProperty). Modded content sometimes does.
- **Fix**: Gate `info.vertex_color_mode = ...` write on `!info.has_material_data`, OR read SLSF `Vertex_Colors` / `Vertex_Alpha` bits and respect modern property as authoritative.

## Group F — Defence-in-depth / future-proofing (3)

### NIF-D5-NEW-05: `NiRollController` missing — Oblivion cinematic / door content may cascade
- **Location**: missing from `crates/nif/src/blocks/mod.rs:574-585` `NiSingleInterpController` alias group
- **Status**: couldn't confirm vanilla Oblivion incidence without archive sweep. Cheap insurance.
- **Fix**: Add to the `NiSingleInterpController` alias group as one-token addition.

### NIF-D6-NEW-06: `KFMParser::allocate_vec` missing `#[must_use]`
- **Location**: `crates/nif/src/kfm.rs:686`
- **Status**: sibling to the pinned `stream.rs:209` — defence-in-depth. All 6 current call sites bind the result; no live regression. Structurally permits the exact "bound-check-only discard" pattern that #831 fixed.
- **Fix**: Add `#[must_use = "allocate_vec returns a sized Vec; bind it or use check_alloc instead"]` mirroring `stream.rs:209`.

### NIF-D6-NEW-07: `BSPositionData` half-float decode loop could batch via `read_u16_array` + `.map(half_to_f32)`
- **Location**: `crates/nif/src/blocks/extra_data.rs:411-418`
- **Status**: pure throughput optimization on FaceGen NPC-load path. Same byte-budget guard applies.
- **Fix**: `stream.read_u16_array(n)?.into_iter().map(half_to_f32).collect()` — one bulk read + one alloc pass vs n paired bound-check + read calls.

## Group G — Alias-loss + test hygiene (2)

### NIF-D5-NEW-07: `NiTriShape | NiTriStrips` and `NiTransformInterpolator | BSRotAccumTransfInterpolator` aliases lose wire-name identity
- **Location**: `crates/nif/src/blocks/mod.rs:271` and `mod.rs:699`
- **Status**: not load-bearing today (renderer treats both topology variants as triangle-list; animation treats both interp variants as plain transform). Becomes load-bearing when M-something wires up `BSRotAccumTransfInterpolator` accumulation or strip-topology rendering.
- **Fix**: Add a `kind` discriminator on both routed structs, mirroring `BsTriShapeKind` / `BsRangeKind` precedent.

### NIF-D2-NEW-08: `detect_oblivion` test asserts `(V20_2_0_7, uv=10, uv2=0)` triplet unreachable from real header
- **Location**: `crates/nif/src/version.rs:373-377` (test), interacts with `crates/nif/src/header.rs:113-119`
- **Status**: synthetic test feeds a triplet the header parser can never produce. Pins behaviour that's unreachable from a real NifHeader.
- **Fix**: Drop the unreachable triplet or replace with `(20.2.0.7, uv=10, uv2=11)` — a real NifSkope export shape that exercises the same `uv < 11 → Oblivion` route.

## Group H — Won't-fix / out-of-scope (1)

### NIF-D5-NEW-06: Pre-Morrowind legacy property types (`NiTransparentProperty`, `NiYAMaterialProperty`, `NiRimLightProperty`, `NiTextureProperty`, `NiTextureModeProperty`, `NiMultiTextureProperty`)
- **Location**: missing from `crates/nif/src/blocks/mod.rs`
- **Status**: niche `V20_2_4_7` window is a niftools-authored version not shipped by Bethesda; pre-Morrowind property types only appear in Civ IV-era content.
- **Decision**: **won't-fix** — close as out-of-scope. Recommend documenting the exclusion list in `blocks/mod.rs` so a future audit doesn't re-discover the same set.

---

## Completeness Checks (bundle-wide)

- [ ] **Group A** doc fixes land in a single PR titled `chore(nif): fix stale comments`
- [ ] **Group B** niche-version fixes have boundary regression tests at the exact gate version
- [ ] **Group C** telemetry fixes confirmed via `nif_stats --drift-histogram` showing the previously-invisible signal
- [ ] **Group D** importer fixes have fixture tests pinning the cascade-vs-resolved outcomes
- [ ] **Group E** mod-content corner has at least one regression test with a synthetic mixed-property mesh
- [ ] **Group F** future-proofing fixes are documented as such (not pretending to fix a live bug)
- [ ] **Group G** alias-loss fix adds the discriminator BEFORE the M-something work that activates the semantic
- [ ] **Group H** won't-fix decision documented in code comment + this issue closed

## Audit reference

`docs/audits/AUDIT_NIF_2026-05-12.md` § Findings → LOW / INFO (all 20).

