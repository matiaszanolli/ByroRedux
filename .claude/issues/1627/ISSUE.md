# TD5-002: GpuMaterial::glass() transmission TODO names a CLOSED issue; preset unused

_Filed as #1627 from `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md`. Immutable snapshot as-filed; GitHub is authoritative for live state._

**Severity**: LOW · **Dimension**: Stale Marker · **Effort**: small (retarget) / large (implement) · **Age**: commit c09d63a6f, 2026-05-23
**Source audit**: `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md` (TD5-002)
**Status**: Active marker referencing CLOSED #1248 with no replacement tracker (this issue is that tracker)

## Description
`GpuMaterial::glass()` (`crates/renderer/src/vulkan/material.rs:601-604`) carries a TODO: "spec_trans = 1.0 … left as a TODO for when the transmission lobe lands (#1248-followup)." **#1248 is CLOSED** with no replacement tracker, and `GpuMaterial::glass` has **zero call sites**. The sibling `car_paint()` / `metal()` presets carry the same "Disney extension not yet on GpuMaterial" note (clearcoat / transmission lobes).

## Evidence
`material.rs:601-603` — `/// Transmission spec_trans = 1.0 is a Disney-BSDF extension … when the transmission lobe lands (#1248-followup).`; `grep "GpuMaterial::glass\|.glass()"` → no call sites.

## Impact
Closed driver + unused preset = marker-outlived-its-driver. A reader follows `#1248-followup` to a closed issue with no continuation.

## Suggested Fix
File/track one issue for the missing Disney lobes (transmission + clearcoat) — this issue — and retarget the three preset comments (`glass`/`car_paint`/`metal`), or drop the `#1248-followup` parenthetical. **Do not delete** the comments — they correctly document GpuMaterial's missing fields.

## Related
#1248 (CLOSED).

## Completeness Checks
- [ ] **SIBLING**: All three presets (`glass` / `car_paint` / `metal`) retargeted to a live tracker, not just `glass`
- [ ] **DROP**: No `GpuMaterial` layout change implied by a comment-only retarget (size pins unaffected)
