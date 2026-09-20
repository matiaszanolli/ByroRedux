# REN-D5-2026-09-20-04: no memory-budget.md ledger row for the 3 swapchain-extent HUD overlay textures (~24.9 MB @1080p); Scaleform section's #3429 prose covers only one of the two HUD drivers

- **ID**: REN-D5-2026-09-20-04
- **Labels**: low,renderer,memory,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: Memory/Lifecycle
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D5-2026-09-20-04)

**Location**: `docs/engine/memory-budget.md` (Scaleform/HUD section) vs the MenuXml 3-buffer rotation (`byroredux/src/hud.rs`, dc306a6a0)

**Description**
Every resource owner is supposed to carry a ledger row; the window's biggest new per-FIF ownership shape has none, and the existing row's prose describes the pre-HUD-driver-split world.

**Evidence**
Audit D5, 2026-09-20 (extent × 3 buffers × 8 B/px arithmetic).

**Impact**
VRAM accounting drifts silently as the overlay set grows with resolution.

**Suggested Fix**
Add the row (per-extent formula + the FIF+1 rotation rationale); update the #3429 prose to name both drivers.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
