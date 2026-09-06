# #3861: TD2-2026-09-05-04: `ImageSpaceModifierFrame` and `ImageSpaceModifierView` are the same 14-field struct in two crates, joined by a hand-written field-by-field copy

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD2-2026-09-05-04) via `/audit-publish`, 2026-09-05. Labels: `low,renderer,scripting,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3861 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD2-2026-09-05-04), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 2 — Logic Duplication
- **Location**:
  - `crates/scripting/src/cinematic.rs` — `ImageSpaceModifierFrame` (+ its `impl Default`)
  - `crates/renderer/src/vulkan/presentation.rs` — `ImageSpaceModifierView` (+ its `impl Default`)
  - `byroredux/src/app_frame.rs` — the 14-line copy inside `.map_or_else(ImageSpaceModifierView::default, |state| { … })`
- **Status**: NEW
- **Description**: The two structs are identical field-for-field *and*
  default-for-default: `blur_radius_pixels`, `double_vision_strength`,
  `motion_blur_strength`, `radial_blur_strength`, `radial_blur_ramp_up`,
  `radial_blur_start`, `radial_blur_ramp_down`, `radial_blur_down_start`,
  `radial_blur_center: [f32; 2]`, `saturation`, `brightness`, `contrast`,
  `tint_color: [f32; 4]`, `fade_color: [f32; 4]`, with the same non-obvious
  identity defaults (`radial_blur_down_start: 1.0`,
  `radial_blur_center: [0.5, 0.5]`, `tint_color: [1.0, 1.0, 1.0, 0.0]`). They
  are bridged by an explicit 14-assignment literal in `app_frame.rs`. Nothing
  checks that the three stay in step.
- **Evidence**: both struct bodies and both `impl Default` bodies are
  reproducible side by side with no textual difference beyond the type name;
  `byroredux/src/app_frame.rs` spells out `blur_radius_pixels: frame.blur_radius_pixels`
  through `fade_color: frame.fade_color`. Adding a 15th IMAD field (the
  cinematic slice is M47.2 and still growing) requires three edits, and omitting
  the third is silent — the field just never reaches the GPU.
- **Impact**: The same lockstep-drift shape that
  `feedback_shader_struct_sync.md` documents for the GPU structs, one tier up
  and with no size assertion to catch it. Blast radius is the whole IMAD /
  cinematic post-process path.
- **Related**: #3327 (the IMAD channel work); `feedback_shader_struct_sync.md`.
- **Suggested Fix**: Hoist one definition into `crates/core` (both
  `byroredux-scripting` and `byroredux-renderer` already depend on
  `byroredux-core`, and neither depends on the other — verified against both
  `Cargo.toml`s), e.g. `crates/core/src/imagespace.rs`, and have both crates
  re-export it. The `app_frame.rs` copy then deletes outright. This is the same
  move `crates/core/src/ecs/components/water.rs` already made for the shared
  water components.
- **Effort**: small (≤2 h)

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
