# #3731 — NIFAL-2026-08-30-D1-01: Material::sanitize_finite never descends into effect_falloff / shader_type_fields

**Severity**: MEDIUM · **Location**: `crates/core/src/ecs/components/material.rs` — `Material::sanitize_finite`, `EffectFalloff`, `ShaderTypeFields`
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-30.md` (NIFAL-2026-08-30-D1-01)

`sanitize_finite`'s macro list covers every directly-declared float field of
`Material` (all 33), so #3373's specific hole is closed — but `Material`
carries two further float payloads behind indirection that the macro list
cannot reach: `effect_falloff: Option<EffectFalloff>` (5 f32) and
`shader_type_fields: Option<Box<ShaderTypeFields>>` (13
`Option<f32>`/`Option<[f32; N]>`) — 22 scalar slots outside both save-path
gates. Both are live on the GPU path (`static_meshes.rs` reads them into
`DrawCommand`/`GpuMaterial`), and the parser applies no finiteness guard, so
a non-finite authored/corrupted value reaches the fragment shader unrepaired
and survives a save/load round trip silently — the pre-save probe reports
the material clean.

## Fix implemented

Per the issue's own suggested fix: gave `EffectFalloff` and
`ShaderTypeFields` their own `sanitize_finite(&mut self) -> bool` methods,
called from `Material::sanitize_finite`:

```rust
if let Some(falloff) = self.effect_falloff.as_mut() {
    changed |= falloff.sanitize_finite();
}
if let Some(fields) = self.shader_type_fields.as_mut() {
    changed |= fields.sanitize_finite();
}
```

`EffectFalloff::sanitize_finite` resets each of its 5 plain `f32` fields
independently to `EffectFalloff::default()`'s value — same `fix_scalar!`
semantics as `Material`'s own method (one poisoned field doesn't take down
its siblings).

`ShaderTypeFields::sanitize_finite` resets each `Option` field **wholesale**
to `None` when it carries any non-finite value (scalar or any array
component) — matching the issue's own "No new constants — reset to each
type's Default" instruction (`Option::default() == None`) and this struct's
own "unset means this variant doesn't use it" convention, rather than
inventing a `Some(0.0)` that would falsely claim a shader variant authored a
payload it didn't. A finite `Some` field, or an already-`None` field,
survives untouched.

**SIBLING** (issue's own checklist item): grepped the whole `Material`
struct for every `Option<...>` carrier — `effect_falloff` and
`shader_type_fields` are the only two; no third indirect float carrier
exists today. `ShaderTypeFields` itself has no further nested `Option`
carriers of its own.

**CANONICAL-BOUNDARY** (issue's own checklist item): both new methods live
on the canonical types themselves (`crates/core`), no per-game branching, no
render-time re-derivation — the repair stays exactly where
`Material::sanitize_finite` already lives.

**TESTS** (issue's own checklist item): extended
`sanitize_finite_leaves_no_non_finite_float_anywhere` to also poison
`effect_falloff` and `shader_type_fields` and assert every indirect field is
repaired (falloff fields finite, shader_type_fields fields reset to `None`).
Added two new isolated tests,
`effect_falloff_sanitize_finite_repairs_independently` and
`shader_type_fields_sanitize_finite_resets_poisoned_fields_to_none`,
mirroring the existing `sanitize_finite_repairs_the_bgem_glass_optics_fields`
pattern — each also pins the already-clean no-op case.

Also fixed a latent self-reference bug this change exposed in the existing
`every_material_float_field_is_covered_by_sanitize_finite` structural scan
(#3438): it searched the whole file (via `include_str!`) for the first
occurrence of the repair-method's signature text to isolate `Material`'s own
`sanitize_finite` body. Once `EffectFalloff`/`ShaderTypeFields` gained their
own same-named methods *earlier* in the file, that search silently matched
the wrong one, and (after a first attempted fix using `rfind`) the test's
own comments/messages describing the search turned out to contain the exact
searched-for text themselves — since the whole test body is embedded in
`src` too. Rewrote the search to scope it to start only after the real
`impl Material {` block opens, and used a `\u{69}`-escaped literal (resolves
to `i` at compile time, but doesn't match `include_str!`'s raw file bytes)
so the search description in this test's own source can never satisfy its
own search. Updated the scan's own doc comment to no longer claim the
indirect-carrier hole is "still open" — it's closed behaviorally by this
fix, though the structural scan's own claim stays limited to `Material`'s
directly-declared fields by design.

Full workspace: `cargo test --no-fail-fast` 7058 passing, 0 failing (+2 new
tests).
