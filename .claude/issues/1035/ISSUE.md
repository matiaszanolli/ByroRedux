# Issue #1035

**Title**: R16-01: DBG_FORCE_NORMAL_MAP (0x20) is orphan dead code after default-on flip

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — R16-01
**Severity**: LOW (dead code / surprise)
**File**: `crates/renderer/shaders/triangle.frag:723`

## Issue

`DBG_FORCE_NORMAL_MAP = 0x20` is declaration-only after the perturbNormal default-on flip (#787/#788). No read site exists; `BYROREDUX_RENDER_DEBUG=0x20` is silently a no-op. Confusing for anyone trying to bisect normal-map behaviour.

## Fix

Either repurpose (e.g. force the perturb path to run on materials lacking a normal map for basis-only diagnostics) or rename to `DBG_RESERVED_20`.

## Completeness Checks
- [ ] **SIBLING**: Update permanent diagnostic-bit catalog comment at triangle.frag:628-686 + audit checklist

