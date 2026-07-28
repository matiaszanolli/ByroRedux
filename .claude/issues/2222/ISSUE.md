# #2222 — REN-DOC-2026-07-28-01: shader-pipeline.md + memory-budget.md trail the live GPU contract (GpuMaterial 300 B vs 348 B, surface_id, FSR reset flag, scene SSBO totals)

_Filed from `docs/audits/AUDIT_RENDERER_2026-07-28.md` by `/audit-publish` on 2026-07-28. Immutable snapshot of the issue **as filed** — GitHub is authoritative for current state (`gh issue view 2222 --json state`)._

---

**Severity:** LOW · **Dimension:** 3 (GPU struct layout) — documentation only
**Source:** `docs/audits/AUDIT_RENDERER_2026-07-28.md` — REN-DOC-2026-07-28-01
**Status when filed:** NEW documentation cluster. **The runtime ABI is correct** — this is
doc rot, not a live defect.

## Description

Both authoritative renderer reference documents trail the live, test-pinned GPU contract.
Contributors and audits reading them make wrong ABI and VRAM-budget assumptions — this is
the exact class of drift the project's shader-struct-sync rule exists to prevent.

## Evidence

**`docs/engine/shader-pipeline.md`**

| Doc says | Live |
|---|---|
| `### GpuMaterial — 300 bytes` (`:198`), table stops at offset 296 | 348 bytes, twelve supplemental role indices at offsets 300–344 |
| `MAX_MATERIALS … 300 B each` (`:290`) | 348 B each |
| `GpuInstance` offset 108 = padding | `surface_id` |
| Opaque mesh IDs = instance index + 1 | stable surface identity |
| `GpuCamera.render_origin.w` = reserved | FSR history-reset flag |

Live size is pinned by `crates/renderer/src/vulkan/material.rs:1272`:

```rust
assert_eq!(std::mem::size_of::<GpuMaterial>(), 348);
```

**`docs/engine/memory-budget.md`**

| Doc says | Live |
|---|---|
| Material SSBO `300 B` → `4.9 MB` / `9.8 MB` (`:21`) | 348 B → ≈ 11.4 MiB |
| Total resident scene buffers ≈ `213 MB` (`:35`, `:373`) | ≈ 214.6 MiB |
| Material overflow is "silent" | `MaterialTable::intern_by_hash` emits a one-shot warning and exposes overflow telemetry via `ctx.scratch` |

## Impact

No runtime corruption. The risk is second-order: a wrong number in a GPU layout contract
is what lets a real `#[repr(C)]` desync slip through review, and a wrong VRAM budget
misprices future feature work.

## Suggested Fix

Refresh both documents from the test-pinned layout and constants — `GpuMaterial` 348 B
with the twelve supplemental role offsets, `GpuInstance.surface_id` at 108, stable
surface-ID mesh IDs, `GpuCamera.render_origin.w` as the FSR history-reset flag, the
corrected material-SSBO and scene-total figures, and the overflow-telemetry correction.

## Completeness Checks
- [ ] **SIBLING**: The GLSL mirror in `crates/renderer/shaders/include/bindings.glsl` is
      checked field-for-field (not just in size) while the layout is being transcribed
- [ ] **TESTS**: Numbers are transcribed from the pinning tests
      (`gpu_material_size_is_348_bytes` and siblings), not re-counted by hand
- [ ] Backticked symbols/paths introduced by the edit pass
      `.claude/commands/_audit-validate.sh`
