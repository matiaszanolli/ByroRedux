## Finding REN2-03 — Renderer Audit 2026-06-11

- **Severity**: HIGH
- **Dimension**: Caustic Splat / Water (canonical Dim 13; independently found by Dims 3, 6, 17)
- **Location**: `crates/renderer/shaders/caustic_splat.comp:339` and `crates/renderer/shaders/water.frag:585`
- **Status**: NEW — regression introduced by `36f66493` (PR #1485). Validated CONFIRMED at HEAD `1e8a25ab`.

## Description

Both caustic writers correctly lift their unprojected G-buffer position by `+renderOrigin` (absolute, `caustic_splat.comp:143`) and trace against the absolute TLAS, but then re-project the landing point with the now-**relative** `viewProj`:

- `caustic_splat.comp:339` — `vec4 clip = viewProj * vec4(P, 1.0);` where `P = G + refr * hitT` is absolute; NDC guard at `:340-342` silently `continue`s.
- `water.frag:585` — `vec4 floorClip = viewProj * vec4(floorWorld, 1.0);` with absolute `floorWorld = vWorldPos + refractDir * floorT`; guard at `:586-590`.

Since `VP_rel·x ≡ VP_abs·(x + o)`, the projected point is displaced by the full origin `o`. `water.frag:113` even carries a comment that `renderOrigin` is "Unused here".

## Impact

With `|o| ≥ 4096`, NDC almost always falls outside ±1 and the guards drop every splat — glass caustics (#321) and water floor caustics (#1210 Phase E) silently disappear in nearly all real game content; only the `[0,4096)³` origin cell still works (why the cascade's manual checks missed it). When `o` aligns with the view direction the misprojection can pass the guard and deposit ghost caustics at wrong pixels. No corruption; pure feature-loss regression under realistic conditions.

## Suggested Fix

Subtract the origin before projecting at both sites (`viewProj * vec4(P - renderOrigin.xyz, 1.0)`), fix the `water.frag:113` "Unused here" comment, recompile both `.spv` (plain `-V`).

## Related

REN2-01, REN2-04 (same camera-relative cascade fix branch).

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files — every other shader that projects an absolute point through `viewProj` (sweep all `viewProj *` sites)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer
- [ ] **TESTS**: Regression test added for this specific fix

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
