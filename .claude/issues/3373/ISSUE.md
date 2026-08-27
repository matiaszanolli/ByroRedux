# SAFE-2026-08-27-02: `Material::sanitize_finite` misses the four BGEM glass-optics fields, leaving both save-path NaN gates holed

- **Issue**: [#3373](https://github.com/matiaszanolli/ByroRedux/issues/3373)
- **Finding ID**: `SAFE-2026-08-27-02`
- **Source report**: `docs/audits/AUDIT_SAFETY_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `medium,save-load,nifal,safety,bug`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3373 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: 9 (NIFAL boundary — NaN/Inf on the GPU)
- **Location**: `crates/core/src/ecs/components/material.rs:1032-1088` (`sanitize_finite`); uncovered fields declared in the same file's `Material` struct; consumers `crates/save/src/validate.rs:445-458` and `crates/save/src/driver.rs:142-150`; destination `crates/renderer/src/vulkan/material.rs:361-364` (`GpuMaterial` offsets 364-388)
- **Status**: NEW
- **Description**:

  `sanitize_finite`'s documented contract is "Reset **every** non-finite
  (NaN / ±inf) scalar to its `Material::default()` value"
  (`material.rs:1010-1011`). It is the single implementation both halves of the
  save/load NaN defence depend on: `validate.rs:456` probes a clone with it as
  the **pre-save** gate, and `driver.rs:148` calls it as the **post-restore**
  repair — deliberately, so the field list is not duplicated.

  A mechanised diff of the `Material` struct's float fields against the
  `fix_scalar!` / `fix_vec!` calls shows 33 float fields, of which **four are
  not covered**:

  | Field | Type | GpuMaterial offset |
  |---|---|---|
  | `glass_fresnel_color` | `[f32; 3]` | 364-372 |
  | `glass_refraction_scale` | `f32` | 376 |
  | `glass_blur_scale` | `f32` | 380 |
  | `glass_blur_scale_factor` | `f32` | 384 |

  All four were added on 2026-08-25 (`d9d4a6d7`, BGEM v21+ glass optics), after
  the `sanitize_finite` field list was authored under #2687. Every *other*
  scalar added in the same era — `lighting_effect_1/2`, `subsurface_rolloff`,
  `rimlight_power`, `backlight_power`, `fresnel_power`,
  `grayscale_to_palette_scale` — **is** covered, which is what makes this a
  slip rather than a deliberate exclusion.

  There is no enumerating test: the `sanitize_finite` tests
  (`material.rs:1799-1856`) each poison one or two hand-picked fields, so a
  newly-added field is invisible to them by construction.
- **Evidence**:
  ```rust
  // crates/core/src/ecs/components/material.rs — the tail of sanitize_finite
          fix_scalar!(grayscale_to_palette_scale);
          fix_scalar!(ior);
          fix_scalar!(subsurface);
          fix_scalar!(sheen);
          fix_scalar!(sheen_tint);
          fix_scalar!(anisotropic);
          // ← no fix_vec!(glass_fresnel_color)
          // ← no fix_scalar!(glass_refraction_scale / glass_blur_scale /
          //                  glass_blur_scale_factor)
          changed
  ```
  The four fields reach the GPU unchanged
  (`byroredux/src/material_translate.rs:544-546` → `GpuMaterial` →
  `crates/renderer/shaders/triangle.frag:1500`, `:1731-1734`), and the shader's
  apparent clamp is not a rescue: GLSL `clamp`/`min`/`max` are explicitly
  **undefined** when an operand is NaN — the same NaN-transparency trap
  SAFE-2026-08-20-01 documented for `f32::clamp` on the WATR path.

  The upstream parser offers no gate either: `crates/bgsm/src/reader.rs:62-64`
  is a bare `f32::from_bits(self.read_u32()?)` with no finiteness filter, and
  `bgem.rs:136-142` reads all four fields straight through it.
- **Impact**: A world holding a non-finite glass scalar (a hostile or corrupt
  BGEM in a mod archive is the realistic source) **passes** the pre-save
  validation gate and **survives** restore, putting NaN/±Inf into `GpuMaterial`
  — undefined behaviour on the GPU per this project's own severity rules, not
  merely a visual artefact. Scoped to BGEM-bearing content (FO4 and later),
  which is exactly where the glass path is live.
- **Related**: #2687 / SAFE-D9-01 (the finding that created `sanitize_finite`),
  SAFE-2026-08-20-01 (the same NaN-transparency class on WATR), `d9d4a6d7`
- **Suggested Fix**: Add `fix_vec!(glass_fresnel_color)` plus the three
  `fix_scalar!` calls, and — more durably — add a test that constructs a
  `Material` with every float field poisoned via a macro-generated list and
  asserts `sanitize_finite` returns a fully finite struct, so the next field
  addition cannot silently reopen the hole.

---
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_SAFETY_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `SAFE-2026-08-27-02`._
