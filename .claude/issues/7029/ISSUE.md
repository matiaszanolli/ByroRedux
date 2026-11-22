# REN-D11-2026-11-16-01: nothing ties the settings-registry upscaler default to `UpscalerMode::default()`

Labels: low, renderer, test-gap, bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/7029

**Severity**: LOW
**Source**: `docs/audits/AUDIT_RENDERER_2026-11-16.md` (renderer audit, delta since 2026-11-09)
**Location**: `register_builtin_settings` default literal `"fsr3/quality"` (`crates/core/src/settings/builtin.rs`) vs `impl Default for UpscalerMode` (`crates/renderer/src/vulkan/upscaling.rs`), consumed by `install_universal_settings` (`byroredux/src/main.rs`)
**Status**: NEW (validated against HEAD; no matching open issue)

## Description
On every launch without `--upscaler`, the registry value wins over `RendererConfig::default()`. `core` cannot depend on `renderer`, so the literal is hand-copied; the only guard (`renderer_config_defaults_to_fsr_quality`) covers the bare CLI path. Code is correct today.

**Impact**: test gap; a future change to one default silently diverges the two paths (cf. closed #4975, #5030).

## Suggested Fix
A bin-crate test asserting `parse_upscaler_spec(registry default) == UpscalerMode::default() == parse_renderer_config(&[]).upscaler`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
