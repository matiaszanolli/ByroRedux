## Finding REN2-09 — Renderer Audit 2026-06-11

- **Severity**: LOW
- **Dimension**: Command Recording (Dims 4 + 5)
- **Location**: `byroredux/src/render/camera.rs:97` and `crates/renderer/src/vulkan/context/draw.rs:581`
- **Status**: NEW. Validated CONFIRMED at HEAD `1e8a25ab`.

## Description

`RENDER_ORIGIN_SNAP = 4096.0` is defined independently in both crates, coupled only by a comment (`draw.rs:581` — `// MUST match render/camera.rs`). No shared constant and no cross-crate equality test exists (grep finds only the two definitions plus their usage sites at `camera.rs:156` and `draw.rs:583,585`). Drift would silently desync the CPU-side origin from the renderer's expectations.

## Suggested Fix

Hoist into a shared constant (e.g. `shader_constants_data.rs` or a core export) or add a cross-crate equality test.

## Completeness Checks
- [ ] **SIBLING**: Check for other comment-coupled cross-crate constants introduced by the cascade
- [ ] **TESTS**: Cross-crate equality test (if not hoisting to a single definition)

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
