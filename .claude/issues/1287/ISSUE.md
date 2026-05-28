Surfaced by the 2026-05-28 renderer audit (`docs/audits/AUDIT_RENDERER_2026-05-28.md` Dim 14). Sibling of [#1200 / REN-DIM15-02](https://github.com/matiaszanolli/ByroRedux/issues/1200) and [#1233 / REN-D16-NEW-04](https://github.com/matiaszanolli/ByroRedux/issues/1233) (audit-skill spec rot).

## Issue

`.claude/commands/audit-renderer.md:253` (Dim 14 checklist) says:

> `GpuMaterial` is exactly **260 bytes** (`gpu_material_size_is_260_bytes` test pins it; was 272 B until #804 / R1-N4 dropped the unread `avg_albedo_r/g/b` field).

Actual size today is **300 B** per `crates/renderer/src/vulkan/material.rs:1157`:

```rust
assert_eq!(std::mem::size_of::<GpuMaterial>(), 300);
```

Growth history is documented inline at `material.rs:1148-1154`:

> 260 → 268 (#1248 IOR) → 284 → 296 (#1249 sheen/sheen_tint/subsurface) → 300 (#1250 anisotropic). Function and test name kept as "260" so a future size shift updates them in lockstep with the assertion.

The test name's deliberate stale-by-design contract is documented. The audit-skill text predates the Disney BSDF additions and was never refreshed.

## Risk

None on the runtime side — the actual size pin asserts 300 correctly. Audit-skill text drift means future `/audit-renderer` runs against Dim 14 produce reports that look like a regression on first inspection ("audit says 260, code says 300 — what's wrong?"). This audit's report flagged exactly that, finding M14-2.

## Suggested fix

Two pieces:

1. **`.claude/commands/audit-renderer.md`** — update Dim 14 checklist:

   ```markdown
   - `GpuMaterial` is exactly **300 bytes** after the Disney BSDF additions
     (#1248 ior, #1249 sheen/sheen_tint/subsurface, #1250 anisotropic).
     Size pin at `material.rs:1157`; test is still named
     `gpu_material_size_is_260_bytes` per the in-code rationale at
     material.rs:1148-1154 — a future size shift updates the test name + body
     in lockstep.
   ```

2. (Optional, separate cleanup) Rename the test from `gpu_material_size_is_260_bytes` to `gpu_material_size_pin` so the magic number drops out of the name forever. The in-code comment justifies keeping the name, but a value-free name is cleaner for the future.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: scan other dimensions in `.claude/commands/audit-renderer.md` for similar size / line-number pins that may have drifted (Dim 11 / TAA, Dim 17 / Water, Dim 21 / Disney BSDF are recent enough to have grown)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: N/A — skill-spec fix only (if Optional 2 lands, run cargo test and confirm `gpu_material_size_pin` passes)
