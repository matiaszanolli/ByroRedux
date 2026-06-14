# REN2-17: Procedural water-noise hash degrades at large |world| (absolute-UV lattice) — currently unreachable fallback path

## Finding REN2-17 — Renderer Audit 2026-06-11

- **Severity**: LOW
- **Dimension**: Water
- **Location**: `crates/renderer/shaders/water.frag:139` (`hash21`), fallback gate at `:164` (`normalMapIndex == 0xFFFFFFFFu`), absolute-world UVs from `:359` (`uvWorld = vWorldPos.xz`)
- **Status**: NEW. Validated CONFIRMED at HEAD `1e8a25ab` (reachability caveat verified: only the procedural no-normal-map fallback path is affected).

## Description

`hash21` (consumed by `valueNoise`, `:148-156`) operates on absolute world XY coordinates, which genuinely band at 176k-class coordinates (sin-fract hash precision collapse). However the path runs only when no water normal map is bound; the textured path — which covers all shipping content at ~30× sub-texel margin — never reaches it. Flag for when procedural foam/noise paths activate.

## Suggested Fix

When the procedural path becomes reachable: feed the hash render-origin-relative coordinates (or a wrapped/fract-reduced lattice) instead of absolute world XY. No action needed today beyond a code comment marking the bound.

## Completeness Checks
- [ ] **SIBLING**: Check caustic/foam procedural noise for the same absolute-coordinate lattice pattern
- [ ] **TESTS**: N/A until the path is reachable; add the comment so it isn't rediscovered

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`

