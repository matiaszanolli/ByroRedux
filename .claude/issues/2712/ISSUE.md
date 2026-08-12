# #2712: REN-D7-02: Three of the twelve supplemental role lanes are produced, uploaded and hashed but sampled by no shader, with the deferral recorded only in a one-off audit report

- **Severity**: LOW
- **Dimension**: Material Table / GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/material.rs`
  (`GpuMaterial::lighting_map_index` / `flow_map_index` / `wrinkle_map_index`
  and `supplemental_texture_slot::{LIGHTING, FLOW, WRINKLE}`); GLSL mirror in
  `crates/renderer/shaders/include/bindings.glsl`.
- **Status**: NEW (documentation/telemetry gap — the behaviour itself is a
  deliberate deferral; see the disprove attempt)
- **Description**: Of the twelve supplemental roles added in the 300 → 348 B
  growth, nine are read by `crates/renderer/shaders/triangle.frag`. Three are
  read by nothing: `lightingMapIndex`, `flowMapIndex` and `wrinkleMapIndex`
  appear in the struct declaration and nowhere else under
  `crates/renderer/shaders/`. They are fully live on the producer side:
  - `lighting` is populated from `BSEffectShaderProperty.lighting_texture` by
    `MaterialInfo::texture_set` (`crates/nif/src/import/material/mod.rs`);
  - `lighting`, `flow` and `wrinkle` are all populated by
    `merge_external_material` from `bgsm.lighting_texture` /
    `bgsm.flow_texture` / `bgsm.wrinkles_texture`, and `lighting` again from
    `bgem.lighting_texture` (`byroredux/src/asset_provider/material.rs`);
  - all three are resolved to bindless handles by `map_secondary_texture_handles`
    (`byroredux/src/asset_provider/texture.rs`), so the referenced DDS is loaded
    and uploaded for a map nothing samples;
  - all three are hashed into the dedup key by both `hash_gpu_material_fields`
    and `DrawCommand::material_hash`, so two otherwise-identical materials
    differing only in an unsampled lane occupy two `MaterialTable` slots and
    render byte-identically.

  Neither the Rust struct comment ("Supplemental semantic texture roles …
  source-format agnostic") nor the GLSL one ("Common supplemental semantic
  texture roles (offsets 300-344). Source-game slot numbering has already been
  translated away.") flags the three dead lanes.
- **Evidence**:
  ```
  $ grep -rl lightingMapIndex crates/renderer/shaders/   → include/bindings.glsl        (only)
  $ grep -rl flowMapIndex     crates/renderer/shaders/   → include/bindings.glsl        (only)
  $ grep -rl wrinkleMapIndex  crates/renderer/shaders/   → include/bindings.glsl        (only)
  $ grep -rl tintMapIndex     crates/renderer/shaders/   → include/bindings.glsl, triangle.frag
  ```
- **Disprove attempt (partly successful — narrowed, not dropped)**: the
  deferral IS deliberate and IS written down — but only in a prior audit
  report. `docs/audits/AUDIT_RENDERER_2026-07-28.md` states that these three
  "are imported, uploaded, hashed, and mirrored in GLSL but deliberately
  unsampled pending coordinate/actor-control semantics." So this is not a
  silent content drop, and the severity is LOW rather than MEDIUM. What remains
  is that a one-off report is not a code contract: the sibling FO4 audit
  (`docs/audits/AUDIT_FO4_2026-08-12.md`) tabulates all three as fully wired
  BGSM→`GpuMaterial` lanes with a blank remarks column — the deferral has
  already failed to propagate once, today.
- **Impact**: Per authored lane, one otherwise-unused DDS upload (VRAM +
  archive decompress at cell load) and one dedup-key lane that can split an
  otherwise-shared material. No visual corruption. The bookkeeping risk is the
  real one: the repo files issues for exactly this shape when the deferral
  comment is missing (#2642, "parsed with no `MaterialTextureSet` role **and no
  deferral comment**").
- **Related**: #2627 (BGSM `inner_layer_texture` — a populated role never wired
  by `merge_external_material`, the mirror-image gap); #2642; #2594;
  `docs/audits/AUDIT_RENDERER_2026-07-28.md`.
- **Suggested Fix**: Put the deferral in the code, not just the report — a note
  on the three Rust fields and the matching GLSL block naming the blocking work,
  mirroring how `Material::fresnel_power` records "captured, not yet shaded
  (#2284)". If the upload cost is unwanted meanwhile, gate the three `slot(…)`
  calls in `map_secondary_texture_handles` behind the same note rather than
  resolving handles no shader can reach.

---

---
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-12.md` (finding `REN-D7-02`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

