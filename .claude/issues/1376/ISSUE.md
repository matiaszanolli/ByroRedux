# PERF-D10-NEW-01: build_debug_ui_snapshot clones metrics every frame even when the overlay is hidden

**Severity**: MEDIUM · **Dimension**: Per-frame Translation & UI Overlay (PERF-D10-NEW-01)
**Location**: `byroredux/src/main.rs:1321-1325` (unconditional call) + `:2245-2284` (body); resource `crates/core/src/ecs/metrics.rs:28-79`
**Status**: NEW

`render_one_frame` calls `build_debug_ui_snapshot` before and independent of any `debug_ui.visible` check. It deep-clones two `BTreeMap<String,f32>` + a `Vec<(String,f32)>` (re-allocating every pass-name String) every frame, then `ui.run` discards it when `!visible` (the boot default). The exact "debug overlay that costs when hidden" pattern. The egui GPU/Vulkan path IS correctly gated (verified clean) — only the CPU snapshot build is ungated.

**Fix**: gate the snapshot build on `self.debug_ui.visible` — return `PanelSnapshot::default()` (metrics:None, entities:None) when hidden; `ui.run` already early-returns on `!visible` and ignores the snapshot. (`debug-ui` has no dedicated label — using `renderer` per the domain-gap note.)

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: If fix touches translate_material / Material::resolve_pbr / emitter params, keep per-game logic at the NIFAL boundary
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-05-31.md` (/audit-performance, deep)._
