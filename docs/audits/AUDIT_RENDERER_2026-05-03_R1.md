# Renderer Audit — R1 MaterialTable Focus — 2026-05-03

**Auditor**: Claude Opus 4.7 (1M context)
**Scope**: `--focus 14` (Material Table / R1 Refactor). The 6-phase R1 closeout landed 2026-05-01 (`aa48d64` → `22f294a`). Since then four R1-residual fixes have shipped (#776 / #777 / #778 / #785, plus #797 / SAFE-22 from today's safety audit). This audit verifies the closeout invariants against the current code and surveys any per-instance fields that should now live on `GpuMaterial`.
**Reference reports**:
- `docs/audits/AUDIT_RENDERER_2026-05-01.md` — broad audit covering R1 alongside 14 other dimensions; `R1-N1`/`R1-N2`/`R1-N3` all closed since
- `docs/audits/AUDIT_SAFETY_2026-05-03.md` — surfaced #797 (`MaterialTable::intern` bounds cap), closed today
**Open-issue baseline**: 51 OPEN at audit start; 3 items mention R1 / MaterialTable / GpuMaterial:
  - `#781` PERF-N4 — `to_gpu_material()` allocates 272 B on every dedup-hit DrawCommand
  - `#780` PERF-N1 — no telemetry on dedup ratio
  - `#570` SK-D3-03 — pre-R1, `MaterialInfo::material_kind` truncated to u8

---

## Executive Summary

**0 CRITICAL · 0 HIGH · 0 MEDIUM · 4 LOW** — across 4 new findings. R1 is structurally sound. Every CRITICAL-class invariant the design relies on (272 B layout, all-scalar fields, byte-Hash dedup, materialId bounds, shader-struct sync) is pinned by tests. The 4 findings are all **hygiene-level** — stale comments, dead bytes, defense-in-depth gaps. None blocks any rendering or causes incorrectness.

The R1 closeout has actually IMPROVED past audit's invariants. The original `feedback_shader_struct_sync.md` contract said "GpuMaterial lives in 3 shaders (triangle.vert/frag, ui.vert) — all must be updated in lockstep." Post-#785 closeout, **only `triangle.frag` mirrors GpuMaterial**; `ui.vert` and `triangle.vert` were narrowed to per-instance reads only, with a build-time grep guard (`scene_buffer.rs:1604`) actively rejecting any future drift. Less surface = less drift risk. The widely-mirrored-struct-sync contract for `GpuMaterial` is no longer a real concern — only `GpuInstance` (112 B) is mirrored across 4 shaders, and that's pinned by per-field offset tests.

| Sev | Count | NEW IDs |
|--|--:|--|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 0 | — |
| LOW | 4 | R1-N4 · R1-N5 · R1-N6 · R1-N7 |

### Carryovers — open

| ID | Issue | State |
|---|---|---|
| `#781` | PERF-N4 — `to_gpu_material()` allocates 272 B per dedup-hit | OPEN — perf only, every DrawCommand calls intern() which builds the full GpuMaterial even when dedup will return an existing id. |
| `#780` | PERF-N1 — no dedup-ratio telemetry | OPEN — silent regression risk; dedup % visible only via heap inspection. |

### Confirmed closed (since 2026-05-01 broad renderer audit)

| ID | Closed by | Verification |
|---|---|---|
| `R1-N1` (#785) | `b19cef9` | `ui.vert:48` reads `inst.textureIndex`; build-time grep guard at `scene_buffer.rs:1604`. |
| `R1-N2` (#777) | `62a266f` | `gpu_instance_field_offsets_match_shader_contract` pins all 13 named offsets including `texture_index` and `avg_albedo_*` retentions. |
| `R1-N3` (#778) | `a2bb016` | Stale `inst.<field>` comment refs in `triangle.frag` cleaned. |
| `SAFE-22` (#797) | `c935775` | `MaterialTable::intern` caps at `MAX_MATERIALS - 1`; over-cap returns id 0 with `Once`-gated warn. |

---

## R1 Pipeline Assessment

**Layout invariants**: comprehensively pinned.
- `GpuMaterial` is 272 B (`gpu_material_size_is_272_bytes` at `material.rs:395`).
- `GpuInstance` is 112 B (`gpu_instance_is_112_bytes_std430_compatible` at `scene_buffer.rs:1403`) plus a per-field offset test (`gpu_instance_field_offsets_match_shader_contract` at `:1418`).
- All-scalar contract: 0 `[f32; 3]` fields in either struct (verified via grep).
- Named pad fields: only `_pad_falloff` (offset 268) on GpuMaterial and `_pad_id0` (92) + `_pad_albedo` (108) on GpuInstance. All explicitly zeroed in `Default::default()` so byte-Hash dedup is deterministic.
- R1 sentinel: `gpu_instance_does_not_re_expand_with_per_material_fields` at `scene_buffer.rs:1438` defends against R1-reversal.

**Shader-side mirroring**:

| Shader | `struct GpuInstance` | `struct GpuMaterial` | `materials[]` reads |
|---|:-:|:-:|:-:|
| `triangle.frag` | ✓ (full mirror) | ✓ (full mirror) | 5 (`materials[inst.materialId]`, `materials[hitInst.materialId]`, `materials[tInst.materialId]`) |
| `triangle.vert` | ✓ (full mirror) | — | 0 (uses only `inst.textureIndex`) |
| `ui.vert` | ✓ (full mirror) | — | 0 (uses only `inst.textureIndex` per #785) |
| `caustic_splat.comp` | ✓ (compact form, same offsets) | — | 0 (uses only `inst.avgAlbedoR/G/B`) |

GpuInstance mirrored across 4 shaders; GpuMaterial mirrored only on the consumer (triangle.frag). The build-time grep guard at `scene_buffer.rs:1604-1635` actively rejects any of:
- `fragTexIndex = materials[…]` in `ui.vert`
- `buffer MaterialBuffer` declaration in `ui.vert`
- `struct GpuMaterial` declaration in `ui.vert`
- `materials[inst.…]` indexing in `ui.vert`

This is structurally tighter than the original `feedback_shader_struct_sync.md` contract.

**`MaterialTable::intern` semantics** (post-#797):
- Returns existing id on dedup hit (HashMap O(1) amortised).
- New id = `materials.len() as u32` for the first `MAX_MATERIALS` distinct entries.
- Over-cap returns 0 (sharing the first-interned material's record) with `Once`-gated warn.
- 11 unit tests covering identity dedup, distinct-id assignment, overflow defaulting, clear-resets, etc.

**Identity invariant**: a scene with N copies of the same material renders byte-identical pre/post R1. Verified by:
- Same byte data: `to_gpu_material()` is deterministic on a fixed `DrawCommand`; intern() returns the same id for byte-equal inputs.
- Same shader access: `mat = materials[inst.materialId]` reads the deduped record at the same offsets the pre-R1 path read directly off `inst`.
- Pinning chain: layout offsets pinned (Rust + shader), dedup-correctness pinned (`identical_materials_dedup_to_same_id`).

---

## Findings

### LOW

#### R1-N4 — `GpuMaterial.avg_albedo_{r,g,b}` populated by `to_gpu_material()` but never read by any shader

- **Severity**: LOW
- **Dimension**: Material Table (R1)
- **Locations**:
  - `crates/renderer/src/vulkan/material.rs:117-119` — `avg_albedo_r/g/b` fields on `GpuMaterial` (offsets 144-152)
  - `crates/renderer/src/vulkan/context/mod.rs:298-300` — `to_gpu_material` populates them from `self.avg_albedo`
  - `crates/renderer/shaders/caustic_splat.comp:153` — caustic compute reads `instances[instIdx].avgAlbedoR/G/B` (NOT `materials[…]`)
  - `crates/renderer/shaders/triangle.frag:2113` — GI miss reads `hitInst.avgAlbedoR/G/B` (NOT `materials[…]`)
  - `grep` on all .frag/.vert/.comp returns zero hits for `mat.avgAlbedo` / `material*.avg_albedo*`
- **Status**: NEW
- **Description**: `GpuMaterial` allocates 12 B per material (3 × f32 at offsets 144-152) for `avg_albedo`, populated unconditionally by `to_gpu_material()`. **No shader reads `mat.avgAlbedoR` / `mat.avgAlbedoG` / `mat.avgAlbedoB`** — both consumers (`caustic_splat.comp` and `triangle.frag` GI miss) read from `GpuInstance.avgAlbedo*` instead.

  The retention is deliberate per the comment at `scene_buffer.rs:215-219`:
  > "Kept on the per-instance struct (not migrated) because `caustic_splat.comp` reads it from its own descriptor set (set 0 binding 5) and migrating that path requires adding a separate `MaterialBuffer` binding to the caustic compute pipeline — deferred to a follow-up R1 cleanup."

  But the `GpuMaterial` slot was kept too. Net effect: 12 B written to GPU per material entry, 0 readers.
- **Evidence**:
  ```rust
  // material.rs:117-119
  pub avg_albedo_r: f32, // offset 144
  pub avg_albedo_g: f32, // offset 148
  pub avg_albedo_b: f32, // offset 152
  ```

  ```rust
  // mod.rs:298-300 — populated unconditionally
  avg_albedo_r: self.avg_albedo[0],
  avg_albedo_g: self.avg_albedo[1],
  avg_albedo_b: self.avg_albedo[2],
  ```

  ```glsl
  // caustic_splat.comp:153 — reads from GpuInstance, not GpuMaterial
  vec3 avgAlbedo = vec3(instances[instIdx].avgAlbedoR, ...);
  ```

  ```glsl
  // triangle.frag:2113 — reads from GpuInstance, not GpuMaterial
  vec3 hitAlbedo = vec3(hitInst.avgAlbedoR, hitInst.avgAlbedoG, hitInst.avgAlbedoB);
  ```
- **Impact**: Two-pronged:
  1. **Wasted MaterialBuffer bytes**: 12 B × MAX_MATERIALS (4096) = ~49 KB per frame-in-flight slot of MaterialBuffer dedicated to fields no shader reads. Negligible, but ratchets up if MAX_MATERIALS grows.
  2. **Phantom dedup key**: byte-Hash dedup includes the avg_albedo bytes. Two materials byte-identical except for avg_albedo would get distinct `material_id`s despite being functionally identical from the shader's perspective. Today this is zero-impact because every DrawCommand hard-codes `avg_albedo: [0.5, 0.5, 0.5]` at `render.rs:822` (no per-mesh computation) — so the field never varies. But a future improvement that computes a real per-mesh avg_albedo (downsampled texture) would surface this as a duplicate-material problem on the dedup side.
- **Suggested Fix**: Either remove the slot from `GpuMaterial` (cleaner — 12 B reclaimed, dedup hash drops three fields), or add a `// SAFETY: kept for future migration of caustic_splat.comp to MaterialBuffer`-style comment and a sentinel test that the field stays unread. Removal is simpler given today's hardcoded constant input.

  The matching `to_gpu_material()` lines and the GLSL struct fields at `triangle.frag:110` would need to drop too, in lockstep. Total: 1 file change in Rust (`material.rs` struct + `mod.rs:298-300` removal), 1 file change in GLSL (`triangle.frag` struct comment block + struct field). Update the `gpu_material_size_is_272_bytes` test to its new size.
- **Related**:
  - `#781` PERF-N4 — same blast radius (`to_gpu_material` allocation).
  - The retention comment at `scene_buffer.rs:215-219` should be cross-referenced.

#### R1-N5 — Stale documentation in `material.rs:33-41` and `triangle.frag:1148` about "shader-side struct mirroring"

- **Severity**: LOW
- **Dimension**: Material Table (R1) × Documentation
- **Locations**:
  - `crates/renderer/src/vulkan/material.rs:33-41` — doc comment claims `materials[instance.material_id].foo` migration was "mechanical (no layout shuffling)" and references a `Phase 4–5 mechanical migration check` that no longer exists
  - `crates/renderer/src/vulkan/material.rs:41` — "Shader Struct Sync: matching `struct GpuMaterial` declarations in the GLSL shaders MUST be added in lockstep when Phase 3 lands." Phase 3 has long landed; today only `triangle.frag` mirrors the struct, and the build-time grep at `scene_buffer.rs:1604` enforces that ui.vert does NOT.
  - `crates/renderer/shaders/triangle.frag:1148` — comment says trailing payload "must ride on GpuInstance"; per Phase 6 those fields ride on `GpuMaterial` now.
- **Status**: NEW (documentation drift across the R1 closeout)
- **Description**: Three doc-comment sites lag the post-#785 reality:
  1. `material.rs:33-41` references the Phase 4-5 migration as if it were a current concern. R1 is closed; the reference is historical.
  2. `material.rs:41` says GLSL `struct GpuMaterial` declarations must be added "in lockstep when Phase 3 lands." Post-#785 the contract is narrower — only `triangle.frag` mirrors, others must NOT. The comment doesn't reflect that.
  3. `triangle.frag:1148` says skin/hair/sparkle/multilayer/eye fields "must ride on GpuInstance"; after Phase 6 they ride on GpuMaterial.
- **Evidence**:
  ```rust
  // material.rs:33-41 — stale historical narrative
  /// Mirrors the per-material fields of [`super::scene_buffer::GpuInstance`]
  /// at the same offsets within each vec4 group — this keeps the Phase 4–5
  /// shader-side migration mechanical (rename `instance.foo` to
  /// `materials[instance.material_id].foo`, no layout shuffling).
  ...
  /// **Shader Struct Sync**: matching `struct GpuMaterial` declarations
  /// in the GLSL shaders MUST be added in lockstep when Phase 3 lands.
  /// The `gpu_material_size_is_272_bytes` test below pins the layout
  /// invariant.
  ```

  ```glsl
  // triangle.frag:1148 — stale GpuInstance reference
  // The branches here cover the SKIN/HAIR/SPARKLE/MULTILAYER/EYE set
  // whose trailing payload (`skinTint*`, `hairTint*`, `sparkle*`,
  // `multiLayer*`, `eyeLeftCenter*`, …) can't be derived from
  // textures and must ride on GpuInstance. See plan in issue #562.
  ```
- **Impact**: Pure documentation drift. Code paths are correct; future refactor authors may waste time pursuing what they think is a still-active "mirror across all shaders" contract or trying to put fields on GpuInstance per the stale comment.
- **Suggested Fix**: One-pass sweep updating the three sites:
  - `material.rs:33-41`: rewrite as historical context (Phase 4-5 closed) + state the current contract (only triangle.frag mirrors GpuMaterial; ui.vert and triangle.vert are guarded by `ui_vert_reads_texture_index_from_instance_not_material_table`).
  - `material.rs:41`: remove the "when Phase 3 lands" tense; state the current narrower contract.
  - `triangle.frag:1148`: rename "ride on GpuInstance" → "ride on GpuMaterial (post-Phase 6)".
- **Related**: `feedback_shader_struct_sync.md` — the original memory note pre-dates the narrowed scope and should be updated separately.

#### R1-N6 — `GpuMaterial` lacks per-field offset tests (only the size invariant is pinned)

- **Severity**: LOW
- **Dimension**: Material Table (R1) × Test Coverage
- **Locations**:
  - `crates/renderer/src/vulkan/material.rs:395` — only `gpu_material_size_is_272_bytes` exists
  - `crates/renderer/src/vulkan/scene_buffer.rs:1418` — by contrast, `GpuInstance` has `gpu_instance_field_offsets_match_shader_contract` that asserts every named offset (model=0, texture_index=64, …, _pad_albedo=108)
- **Status**: NEW
- **Description**: A reorganization within the 17 vec4 slots that preserves total size (272 B) but reorders fields would not trip any test. The shader-side `struct GpuMaterial` (in `triangle.frag:83`) has matching offset comments **at vec4-group level only** ("vec4 #4: textureIndex, normalMapIndex, darkMapIndex, glowMapIndex"), not per-field. So a swap between two same-type fields within a vec4 (e.g., `textureIndex ↔ normalMapIndex`) would produce wrong shader reads with no test failure.

  `GpuInstance` is hardened against this by `gpu_instance_field_offsets_match_shader_contract` at `scene_buffer.rs:1418` — which asserts every named field's `offset_of!`. `GpuMaterial` has no equivalent.
- **Evidence**:
  ```rust
  // scene_buffer.rs:1418 — GpuInstance per-field offset test
  fn gpu_instance_field_offsets_match_shader_contract() {
      assert_eq!(offset_of!(GpuInstance, model), 0);
      assert_eq!(offset_of!(GpuInstance, texture_index), 64);
      assert_eq!(offset_of!(GpuInstance, bone_offset), 68);
      // ... 13 named fields
  }
  ```
  No equivalent exists in `material.rs` (only the size pin at `:395`).
- **Impact**: Defense-in-depth gap. No live bug today — current field layout is correct. Surfaces only when a refactor reorders fields within a vec4 slot. The size pin would not catch it; the shader would silently misread.
- **Suggested Fix**: Add a sibling test in `material.rs::tests`:
  ```rust
  use std::mem::offset_of;
  #[test]
  fn gpu_material_field_offsets_match_shader_contract() {
      // vec4 #1 — PBR scalars
      assert_eq!(offset_of!(GpuMaterial, roughness), 0);
      assert_eq!(offset_of!(GpuMaterial, metalness), 4);
      assert_eq!(offset_of!(GpuMaterial, emissive_mult), 8);
      assert_eq!(offset_of!(GpuMaterial, material_flags), 12);
      // ... pin all 56 fields against `triangle.frag:83-127`'s
      // GLSL struct order. A reorder within any vec4 slot fails
      // here before the shader silently misreads.
  }
  ```
- **Related**: SK-D5-04 / #615 (cross-game stream-alignment drift) — same pattern of "size matches, contents don't".

#### R1-N7 — `material_id == 0` overload: shared by "default-init instance" AND "first interned material"

- **Severity**: LOW
- **Dimension**: Material Table (R1)
- **Locations**:
  - `crates/renderer/src/vulkan/scene_buffer.rs:241-245` — `GpuInstance::default()` sets `material_id: 0` with comment `"`0` is a valid material id (the first slot in the per-frame table; also the neutral-lit default material when no real one was interned)."`
  - `crates/renderer/src/vulkan/material.rs:330` — `intern()` returns `0` for the first distinct material AND for over-cap entries (post-#797)
  - `crates/renderer/src/vulkan/context/draw.rs:987-990` — UI quad pushes `..GpuInstance::default()` (material_id = 0)
- **Status**: NEW
- **Description**: `material_id == 0` is overloaded to mean THREE distinct things:
  1. **"This instance was default-initialized"** — never went through `intern()`, e.g. UI quad path
  2. **"First interned material this frame"** — the standard intern path's first-call return value
  3. **"Over-cap fallback"** — post-#797, intern returns 0 for the 4097th distinct material

  Cases 2 and 3 actually share an SSBO entry (the first-interned material's bytes) — that's intentional. Case 1 ALSO ends up reading that entry if the shader reads `materials[0]` (which #785 and the ui.vert grep guard prevent in the UI overlay path).

  But for any FUTURE consumer that accidentally reads `materials[material_id]` on a default-initialized instance (e.g., a future debug-overlay quad, a test fixture, a synthetic mesh), the same #785-shape failure mode reappears. The overload makes "`material_id == 0`" semantically ambiguous.

  The mitigation today is the build-time grep guard. It's narrow — it covers only `ui.vert`. Other shaders that emerge later won't be covered automatically.
- **Evidence**: see #785 / R-N1 incident from 2026-05-01: a stale shader hunk reintroduced `materials[inst.materialId]` in `ui.vert` and was caught by manual audit, not by any test. After #785 a build-time grep guard was added at `scene_buffer.rs:1604` covering ui.vert specifically.
- **Impact**: Latent. Today no shader OUTSIDE ui.vert default-initializes `GpuInstance` and then reads `materials[id]`. The grep guard prevents ui.vert from regressing. A future shader that does both is a #785-class footgun.

  Severity is LOW because the existing build-time guard plus the rarity of the pattern keep it benign in practice; it's a structural concern about the type system rather than a live bug.
- **Suggested Fix**: Two options, both architectural:
  1. **Sentinel id**: reserve `material_id == u32::MAX` (or 0xFFFFFFFF) as the "no material assigned" sentinel. `GpuInstance::default()` uses the sentinel. Shaders gate `if (inst.materialId != 0xFFFFFFFFu) { mat = materials[…] }`. Trade-off: every shader that reads `materials[]` adds a branch.
  2. **Reserve slot 0**: `MaterialTable::new()` pre-pushes a "neutral-lit default" GpuMaterial at id 0 BEFORE any user intern fires. Default-initialized instances safely read the neutral material; user-interned materials start at id 1. Trade-off: 272 B per frame-in-flight is "wasted" on the neutral default if no instance defaults to it.

  Option 2 is simpler and matches the intent of the existing comment at `scene_buffer.rs:241-245`. Defer until a real second consumer of the `GpuInstance::default()` pattern emerges.
- **Related**:
  - `#785` (closed) — the canonical incident demonstrating the footgun
  - `#797` / SAFE-22 (closed) — the over-cap `intern` defaulting to 0 added a third overload to the `material_id == 0` semantic

---

## Prioritized Fix Order

1. **R1-N5** (LOW) — doc-comment sweep (3 sites, ~10 lines). Cheapest; no risk. Ship anytime.
2. **R1-N6** (LOW) — add `gpu_material_field_offsets_match_shader_contract` test. ~20 lines, mirrors the existing `GpuInstance` test exactly. Defense-in-depth.
3. **R1-N4** (LOW) — drop `avg_albedo` from `GpuMaterial` (3 fields + 12 B). Touches Rust + GLSL in lockstep; update the size pin to its new value. Cleanest after the offset test in R1-N6 lands (no per-field offsets to renumber).
4. **R1-N7** (LOW) — defer architectural change. Track as a future-work note; don't ship until a second non-ui.vert consumer of `GpuInstance::default()` emerges.

The HIGH and MEDIUM dimensions of the R1 audit are empty. The closeout is solid.

---

## Verified Working — No Gaps

- **Layout invariants**: 272 B GpuMaterial + 112 B GpuInstance, both pinned by tests.
- **Per-field GpuInstance offsets**: pinned (13 named fields, all asserted).
- **All-scalar contract**: zero `[f32; 3]` fields in either struct.
- **Named pad fields**: explicitly zeroed in `Default::default()`; byte-Hash dedup deterministic.
- **Shader struct mirroring**: GpuInstance across 4 shaders (triangle.vert/frag, ui.vert, caustic_splat.comp); GpuMaterial only in triangle.frag.
- **Build-time grep guard**: rejects 4 forms of ui.vert regression (the #785 family).
- **`MaterialTable::intern`**: O(1) amortised dedup, stable id assignment, post-#797 overflow cap, 11 unit tests covering every reachable path.
- **Identity invariant**: byte-equivalent rendering pre/post R1 — verified through layout pinning + dedup correctness.
- **R1 sentinel**: `gpu_instance_does_not_re_expand_with_per_material_fields` defends against R1-reversal.

---

## Methodology Notes

- The 4 findings cluster around `avg_albedo` retention, doc-comment lag, defense-in-depth in offset testing, and the `material_id == 0` overload. None is a live bug.
- The biggest signal from this audit is *negative*: looking for a major R1 bug, didn't find one. The 6-phase closeout + 4 residual fixes (#776 / #777 / #778 / #785 / #797) closed the design space tightly.
- Sub-agent dispatches deliberately not used per established methodology. Direct main-context delta audit produces a deterministic deliverable.

---

*Generated by `/audit-renderer --focus 14` on 2026-05-03. To file findings as GitHub issues: `/audit-publish docs/audits/AUDIT_RENDERER_2026-05-03_R1.md`.*
