# #3862: TD2-2026-09-05-05: `FloatTarget` / `ColorTarget` are duplicated verbatim across the `nif` → `core` boundary, bridged by a 20-arm identity match

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD2-2026-09-05-05) via `/audit-publish`, 2026-09-05. Labels: `low,nif-parser,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3862 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD2-2026-09-05-05), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 2 — Logic Duplication
- **Location**:
  - `crates/core/src/animation/types.rs` — `FloatTarget` (13 variants), `ColorTarget` (7 variants)
  - `crates/nif/src/anim/types.rs` — `FloatTarget` (13 variants), `ColorTarget` (7 variants)
  - `byroredux/src/anim_convert.rs` — the closures `convert_float_target` and `convert_color_target`
- **Status**: NEW
- **Description**: Both enums have the same variants in the same order with the
  same payloads (`MorphWeight(u32)` included) and the same derive set
  (`Debug, Clone, Copy, PartialEq, Eq, Hash`). The bridge between them is a pure
  identity map — 13 + 7 arms of `na::FloatTarget::X => FloatTarget::X`. Unlike
  `KeyType` (whose NIF side is `blocks::interpolator::KeyType`, a genuine
  wire-format enum decoded from the file), these two are *already* the
  post-translation semantic vocabulary on both sides: `crates/nif/src/anim/channel.rs`
  maps the raw `operation` / `target_color` discriminators onto the NIF-side
  enum, so the second enum adds no translation, only a second place to add a
  variant.
- **Evidence**: `byroredux/src/anim_convert.rs` contains
  `na::FloatTarget::Alpha => FloatTarget::Alpha` … `na::FloatTarget::RefractionStrength => FloatTarget::RefractionStrength`
  and `na::ColorTarget::Diffuse => ColorTarget::Diffuse` …
  `na::ColorTarget::LightAmbient => ColorTarget::LightAmbient`, with no arm doing
  anything other than renaming the path. `crates/nif/Cargo.toml` already lists
  `byroredux-core = { workspace = true }`, so the re-export direction is
  available today. Adding `EmissiveMultiple` and `RefractionStrength` under
  #3327 required editing both enums and both match closures.
- **Impact**: Four edits per new animation-channel target instead of one, with
  the identity map as the only thing between a forgotten variant and a
  non-exhaustive-match compile error (which is at least loud) — but also 20
  lines of pure ceremony that read as if translation were happening. This is
  the *unconverged* sibling of the `crates/nif/src/anim/coord.rs` re-export the
  discovery recipe cites as the finished example.
- **Related**: #2304 (CLOSED, NIFAL-D7-03) — a different defect: that one
  covered the `operation`→`FloatTarget` / `target_color`→`ColorTarget`
  *discriminator tables* duplicated between the KF and embedded-animation arms
  *inside* `crates/nif`; this is the enum *type* duplicated across the crate
  boundary. Also `canonical_translation_layer.md` ("promote, don't add a third
  type").
- **Suggested Fix**: Make `crates/nif/src/anim/types.rs` re-export the core
  enums —
  `pub use byroredux_core::animation::types::{ColorTarget, FloatTarget};` —
  exactly as `crates/nif/src/anim/coord.rs` re-exports `byroredux_core::math::coord`,
  then delete `convert_float_target` / `convert_color_target` from
  `byroredux/src/anim_convert.rs`. Leave `KeyType` alone: its NIF side is a wire
  enum and its conversion is real.
- **Effort**: trivial (≤30 min)

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
