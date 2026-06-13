## Finding NIF-NEW-06 — NIF Audit 2026-06-13

- **Severity**: LOW (no mis-parse today; maintenance / regression-surface foot-gun)
- **Dimension**: Version Handling
- **Location**: `crates/nif/src/version.rs` — `has_dedicated_shader_refs`, `uses_bs_lighting_shader`, `uses_fo4_shader_flags`, `uses_fo76_shader_flags`, `has_dynamic_effect_fields`, `has_legacy_binary_extra_data`.
- **Status**: NEW — validated CONFIRMED at HEAD `8d191d7d`.

## Description

These six `NifVariant` feature-flag helpers have **zero production call sites** — every reference is inside `version.rs`'s own `#[cfg(test)]` module. Every real parser queries `stream.bsver()` directly (deliberately — the helpers key off the coarse variant and mis-classify transitional `v20.2.0.7/bsver≤26` and `bsver==0` exports). The #938 cleanup already deleted three zero-call-site helpers for this exact reason; these six are the residual of the same pattern.

Worse, adopting some of them would be actively wrong: `uses_fo4_shader_flags()` returns true for the entire `Fallout4` variant including in-file bsver 132/139, while the real parser switches to CRC arrays at `bsver >= 132` (`shader.rs:411`, `if bsver < bsver::FO4_CRC_FLAGS`) — so wiring the helper in would invert the FO4-DLC shader-flag layout.

## Evidence (validated)

- `grep -rn '.<helper>()' crates/nif/src/` for each of the six → all call sites are `assert!(...)` lines inside `version.rs:958-1027` (the test module). Production call count = 0 for all six. (`has_dedicated_shader_refs` and `has_dynamic_effect_fields`/`has_legacy_binary_extra_data` show 5–10 grep hits but every one is a test assertion.)
- `shader.rs:411` is the authority for the FO4 CRC boundary; `uses_fo4_shader_flags()` disagrees for bsver 132/139.

## Impact

None at runtime (helpers unused). The risk is a future "migrate raw bsver to helpers" refactor silently mis-gating FO4-DLC / transitional-export content — the exact failure modes the per-call-site comments warn about. The foot-gun is that a contributor can't tell which helper family is "blessed" (`has_culling_mode`/`has_material_crc`/`has_shader_alpha_refs`/`has_properties_list` ARE called; these six are not) without grepping.

## Suggested Fix

Either delete the six unused helpers (matching the #938 precedent), or annotate each loudly (`// NO production call site — parsers MUST use stream.bsver(); see #938`) with a `#[cfg_attr(not(test), allow(dead_code))]`-style marker so the "unused" status is explicit. Do NOT add call sites for `uses_fo4_shader_flags` / `uses_fo76_shader_flags` — they're variant-coarse and would mis-gate the 132 CRC boundary.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Re-confirm the "blessed" helper family (`has_culling_mode`/`has_material_crc`/`has_shader_alpha_refs`/`has_properties_list`) genuinely has production call sites before deleting the unused six, so the delete doesn't remove a live one
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **CANONICAL-BOUNDARY**: N/A
- [ ] **TESTS**: If deleting, remove the corresponding `version.rs` test assertions; if keeping, the existing assertions stay

---
Source: `docs/audits/AUDIT_NIF_2026-06-13.md` · Filed by `/audit-publish` · NIF-D2-NEW-03
