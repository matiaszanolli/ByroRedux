# Issue #472

FNV-REN-M1: Stale composite.rs comment claims 'once TAA is wired up' — TAA is already wired

---

## Severity: Medium (misleading docs)

**Location**: `crates/renderer/src/vulkan/composite.rs:967-973`

## Problem

Doc comment on `rebind_hdr_views`:

> Used to switch composite's input from raw HDR to TAA output **once TAA is wired up**.

TAA is already wired up — two live callers exist:
- `crates/renderer/src/vulkan/context/mod.rs:792` — init path, rebinds to TAA storage image at `GENERAL` layout
- `crates/renderer/src/vulkan/context/resize.rs:347` — resize path, same

A reader debugging tone-map lag, temporal flicker, or TAA pipeline ordering will read this comment and conclude TAA is not yet in the pipeline — chasing a nonexistent bug.

## Impact

Future debugging friction. Misleads new contributors about pipeline state.

## Fix

Update the comment to reflect current state:

```rust
/// Rewrite binding 0 (HDR sampler) across every per-frame descriptor set
/// to point at a different set of views. Called from init and resize to
/// switch composite between raw HDR and TAA output.
///
/// `hdr_layout` must match the current image layout:
///   - `SHADER_READ_ONLY_OPTIMAL` for raw HDR from the render pass
///   - `GENERAL` for TAA storage-image output (current usage)
```

## Completeness Checks

- [ ] **DOCS**: Comment updated
- [ ] **SIBLING**: Check for other stale "once X is wired up" comments in the renderer tree

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-REN-M1)
