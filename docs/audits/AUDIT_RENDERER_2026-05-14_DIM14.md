# Renderer Audit — Dimension 14: Material Table (R1) — 2026-05-14

**Auditor**: Claude Sonnet 4.6 (1M context)  
**Scope**: `--focus 14 --depth deep`  
**Prior audit refs**: 2026-05-03 (`R1-N4/N5/N6/N7`), 2026-05-13 (`REN-D14-NEW-01/02/03`)  
**Open-issue baseline**: `/tmp/audit/renderer/issues.json` — no open issues with D14 / R1 / MaterialTable labels

---

## Executive Summary

**0 CRITICAL · 0 HIGH · 0 MEDIUM · 3 LOW · 1 INFO**

All material-table structural invariants hold. The R1 closeout is sound. Four new findings are documentation/comment drift or missing guard tests — none affect rendering correctness.

| Sev | Count | IDs |
|-----|------:|-----|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 0 | — |
| LOW | 3 | REN-D14-NEW-04 (carryover), REN-D14-NEW-05, REN-D14-NEW-06 |
| INFO | 1 | REN-D14-NEW-07 |

---

## Status of 2026-05-13 Findings

| ID | Status |
|----|--------|
| **REN-D14-NEW-01** — dedup-ratio console off-by-one | **FIXED** (#1032). `unique_user_count()` live, pinned by `unique_user_count_excludes_seeded_slot` test. Prospector baseline now reads 87 unique (not 88). |
| **REN-D14-NEW-02** — first-frame redundant 260 B re-upload | **ACCEPTED / DOCUMENTED** (`material.rs:523-533`). Hash dirty-gate in `upload_materials` suppresses all subsequent identical-frame re-uploads; the single first-frame upload is below any measurement threshold. Not a regression. |
| **REN-D14-NEW-03** — dead defence-in-depth warn | **STILL OPEN** → carried forward as REN-D14-NEW-04 below. |

---

## RT Pipeline Assessment — Material Table

Not applicable to this dimension (no ray-tracing pipeline involvement).

## Rasterization Assessment

The R1 material table feeds the geometry pass and UI overlay:

- `triangle.frag` — the only shader that declares and reads `struct GpuMaterial` via `materials[inst.materialId]`
- `triangle.vert`, `ui.vert`, `water.vert/frag`, `caustic_splat.comp` — do NOT read `MaterialBuffer`
- Build-time grep guard in `gpu_instance_layout_tests.rs` enforces `ui.vert` isolation; water shaders currently correct but unguarded (see REN-D14-NEW-07)

---

## Findings

### LOW

#### REN-D14-NEW-04 — Dead overflow warn in `upload_materials`

- **Severity**: LOW
- **Dimension**: Material Table (R1)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:272-278`
- **Status**: CARRYOVER (was REN-D14-NEW-03, 2026-05-13)
- **Description**: The block `if materials.len() > MAX_MATERIALS { log::warn!("Material table overflow…") }` is statically unreachable. The only production call site passes `material_table.materials()`, whose length is bounded to `<= MAX_MATERIALS` by `MaterialTable::intern()`'s cap gate at `material.rs:607`. The condition can never be true.
- **Evidence**:
  ```rust
  // upload.rs:271-278
  let count = materials.len().min(MAX_MATERIALS);
  if materials.len() > MAX_MATERIALS {          // always false
      log::warn!(
          "Material table overflow: {} materials submitted, capped at {} …",
          materials.len(), MAX_MATERIALS,
      );
  }
  ```
  `material.rs:607`: `if self.materials.len() >= MAX_MATERIALS { return 0; }` — the Vec never grows past `MAX_MATERIALS`.
- **Impact**: Dead code. The `let count = …min(…)` clamp stays a valid no-op safety net; the associated warn is misleading.
- **Suggested Fix**: Remove the unreachable `if` block (3 lines). Retain the `.min(MAX_MATERIALS)` clamp.

---

#### REN-D14-NEW-05 — `ui.vert` header comment falsely claims `materialId` lookup

- **Severity**: LOW
- **Dimension**: Material Table (R1)
- **Location**: `crates/renderer/shaders/ui.vert:11-14`
- **Status**: NEW
- **Description**: The R1 Phase 6 migration comment reads: _"UI vertex stage reads `materialId` to look up the texture in the `MaterialBuffer` SSBO; other per-material fields live exclusively in `materials[materialId]` now."_ This is factually wrong. The `main()` at line 48 reads `fragTexIndex = inst.textureIndex` (per-instance, not per-material), which is the contracted path. `ui.vert` deliberately does NOT read `MaterialBuffer` — the `ui_vert_reads_texture_index_from_instance_not_material_table` guard enforces this. The comment contradicts both the code and the inline explanation at lines 42–46.
- **Evidence**:
  ```glsl
  // ui.vert:11-14 (WRONG):
  // R1 Phase 6 — UI vertex stage reads `materialId` to look up
  // the texture in the `MaterialBuffer` SSBO …
  
  // ui.vert:47-48 (CORRECT code):
  GpuInstance inst = instances[gl_InstanceIndex];
  fragTexIndex = inst.textureIndex;   // NOT materialId / MaterialBuffer
  ```
- **Impact**: A developer reading the comment might add a `materials[inst.materialId]` read — the exact #776/#785 regression. The guard test would catch it, but the comment creates the wrong mental model first.
- **Suggested Fix**:
  ```glsl
  // R1 Phase 6 — GpuInstance collapsed to per-DRAW data only. The UI
  // vertex stage reads `textureIndex` directly from the per-instance
  // struct — NOT from the MaterialBuffer SSBO. See #776 / #785 for why
  // this is intentional: the UI quad's `materialId = 0` would alias
  // the first scene material. Layout mirror of triangle.{vert,frag}.
  ```

---

#### REN-D14-NEW-06 — `ScratchTelemetry` doc has inverted dedup-ratio formula

- **Severity**: LOW
- **Dimension**: Material Table (R1)
- **Location**: `crates/core/src/ecs/resources.rs:333-334`
- **Status**: NEW
- **Description**: The `materials_interned` field doc says `"Dedup ratio = materials_unique / materials_interned"`. The actual computation in `commands.rs:350` is `materials_interned / materials_unique` (placements ÷ unique = multiplier > 1). The documented formula is the reciprocal — it would yield a fraction < 1 for any scene with dedup hits.
- **Evidence**:
  ```rust
  // resources.rs:333-334 (WRONG formula in doc):
  /// Dedup ratio = `materials_unique / materials_interned`.

  // commands.rs:350 (CORRECT):
  let ratio = tlm.materials_interned as f64 / tlm.materials_unique.max(1) as f64;
  ```
- **Impact**: Documentation only. A reader computing the ratio by hand from the struct definition gets the wrong number (e.g. 0.07× instead of 14×). No runtime effect.
- **Suggested Fix**:
  ```rust
  /// Dedup ratio = `materials_interned / materials_unique`. A value > 1
  /// means dedup is saving SSBO space; near 1.0 means nearly every draw
  /// uses a unique material. A *drop* signals a dedup regression.
  ```

---

### INFO

#### REN-D14-NEW-07 — No static guard preventing `water.vert`/`water.frag` from acquiring a `GpuMaterial` binding

- **Severity**: INFO
- **Dimension**: Material Table (R1)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` (absence)
- **Status**: NEW
- **Description**: The `ui_vert_reads_texture_index_from_instance_not_material_table` test guards `ui.vert` from ever declaring `struct GpuMaterial` or a `MaterialBuffer` binding. No equivalent guard exists for `water.vert` or `water.frag`. Currently correct — zero hits on `GpuMaterial`, `MaterialBuffer`, or `materials[` in either water shader. But as the water shader matures (underwater caustics, TSM interaction), there is no test to catch an accidental `materialId`-based lookup being added.
- **Impact**: No current regression. Risk surface: future water feature work adds a `MaterialBuffer` binding without realising the water pipeline uses push constants, not the material-table path.
- **Suggested Fix**:
  ```rust
  #[test]
  fn water_shaders_do_not_read_material_buffer() {
      for (name, src) in [
          ("water.vert", include_str!("../../../shaders/water.vert")),
          ("water.frag", include_str!("../../../shaders/water.frag")),
      ] {
          assert!(!src.contains("struct GpuMaterial"),
              "{name}: must NOT declare GpuMaterial");
          assert!(!src.contains("buffer MaterialBuffer"),
              "{name}: must NOT bind MaterialBuffer");
          assert!(!src.contains("materials["),
              "{name}: must NOT index materials[]");
      }
  }
  ```

---

## Verified-Clean List

| # | Invariant | Evidence |
|---|-----------|---------|
| 1 | **GpuMaterial = 260 B** | `gpu_material_size_is_260_bytes` test present; 64 scalars × 4 B + `_pad_falloff` = 260 B. Post-#804 (avg_albedo removed). |
| 2 | **65-field offset pin** | `gpu_material_field_offsets_match_shader_contract` asserts all 65 fields. Within-vec4 swaps caught. |
| 3 | **No `[f32; 3]` fields** | grep confirms zero `[f32; 3]` / `[u32; 3]` in `material.rs`. |
| 4 | **Hash + Eq use raw bytes** | `as_bytes()` via `from_raw_parts`; f32 fields use `to_bits()`; deterministic for all reachable values. |
| 5 | **`_pad_falloff` zeroed** | `Default` sets `_pad_falloff: 0.0`; excluded from hash; offset pinned at 256. |
| 6 | **Slot 0 sentinel (#807)** | `seed_neutral_default()` pre-pushes neutral GpuMaterial at slot 0. User materials start at slot 1. Over-cap interns → slot 0 (neutral, not first-user-material alias). Pinned by 3 tests. |
| 7 | **intern() cap** | `materials.len() >= MAX_MATERIALS` returns 0; `Once`-gated warn. |
| 8 | **SSBO upload sizing** | `min(len, MAX_MATERIALS)`; hash dirty-gate skips re-upload on unchanged frames. |
| 9 | **MAX_MATERIALS = 4096** | `scene_buffer/constants.rs:103`. |
| 10 | **GpuInstance.material_id: u32** | Present at offset 88, asserted by offset-pin test. |
| 11 | **triangle.frag shader reads** | `GpuMaterial mat = materials[inst.materialId]` at :854; key fields roughness, normalMapIndex, materialFlags, textureIndex verified at correct offsets. |
| 12 | **Only triangle.frag mirrors GpuMaterial** | triangle.vert, ui.vert, water.vert/frag, caustic_splat.comp confirmed GpuMaterial-free. |
| 13 | **ui.vert grep guard** | `ui_vert_reads_texture_index_from_instance_not_material_table` still wired; guards 4 regression patterns. |
| 14 | **Identity invariant** | `identical_materials_dedup_to_same_id` + `distinct_materials_get_distinct_ids` pin the dedup path. |
| 15 | **Phase 6 closeout** | `gpu_instance_does_not_re_expand_with_per_material_fields` guards against R1-reversal. GpuInstance = 112 B. |
| 16 | **F-WAT-07** | Water bypasses MaterialTable by design (push constants). No GpuMaterial data read on water pipeline. Tracking issue stays in Water dimension. |
| 17 | **Dedup-ratio telemetry** | `mat.stats` command formula `interned / unique` yields correct N× multiplier. REN-D14-NEW-01 confirmed FIXED. |
| 18 | **GpuInstance byte-identical across 4 shaders** | triangle.vert/frag, ui.vert, water.vert, caustic_splat.comp all 112 B; layout pinned by 2 tests. |
| 19 | **GLSL field-name pin** | `gpu_material_glsl_field_names_pinned` asserts 54 field-name needles present in triangle.frag. GLSL renames fail `cargo test`. |
| 20 | **#890 Stage 2c forward-compat** | `_pad_falloff` repack planned; size pin will catch premature addition of field without consuming the pad. Sound. |

---

## Prioritized Fix Order

| Priority | Finding | Effort |
|----------|---------|--------|
| 1 | **REN-D14-NEW-05** — `ui.vert` header comment wrong (misleads about Phase 6 contract; same mental model as #776 regression) | 5-min comment edit |
| 2 | **REN-D14-NEW-06** — `ScratchTelemetry` inverted ratio formula | 3-min doc edit |
| 3 | **REN-D14-NEW-04** — dead `upload_materials` overflow warn (carryover) | 3-line deletion |
| 4 | **REN-D14-NEW-07** — add `water_shaders_do_not_read_material_buffer` guard test | ~15-min test addition |

All four are zero-risk doc/comment/dead-code fixes. No CRITICAL or HIGH findings. The R1 material table is structurally sound.

---

## Methodology Notes

- Direct main-context audit (no sub-agent dispatch per established methodology for focused single-dimension runs).
- Read all primary entry points: `material.rs`, `scene_buffer/constants.rs`, `gpu_types.rs`, `upload.rs`, `buffers.rs`, `render.rs`, all 5 shaders, `gpu_instance_layout_tests.rs`.
- Cross-referenced against `issues.json`; no open issues overlap with findings above.
- Dedup baseline vs 2026-05-03 and 2026-05-13 reports: three 2026-05-13 findings resolved (two fixed, one carried).

---

*Generated by `/audit-renderer 14` on 2026-05-14. To file findings as GitHub issues: `/audit-publish docs/audits/AUDIT_RENDERER_2026-05-14_DIM14.md`*
