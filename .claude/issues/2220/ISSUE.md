# #2220 — REN-COMPAT-2026-07-28-01: authored CELL/WTHR fog is uploaded every frame but consumed by no shader

_Filed from `docs/audits/AUDIT_RENDERER_2026-07-28.md` by `/audit-publish` on 2026-07-28. Immutable snapshot of the issue **as filed** — GitHub is authoritative for current state (`gh issue view 2220 --json state`)._

---

**Severity:** MEDIUM · **Dimensions:** 8 (composite), 16 (volumetrics), 18 (sky/weather/exterior)
**Source:** `docs/audits/AUDIT_RENDERER_2026-07-28.md` — REN-COMPAT-2026-07-28-01
**Status when filed:** NEW — newly consolidated compatibility defect

## Description

Authored fog reaches the GPU and then dies there. CELL/WTHR fog color, near/far distance,
and FNV cubic-fog parameters are resolved and uploaded every frame, but **no shader
consumes them**, and the volumetric medium that would be the natural consumer runs on
hardcoded constants instead.

Existing tests prove CPU-side propagation, not shader consumption — which is why this
survived.

## Evidence

`byroredux/src/render/mod.rs` resolves and uploads CELL/WTHR fog color, near/far
distance, and the FNV cubic-fog parameters.

`crates/renderer/shaders/composite.frag:31-45` marks both fields reserved and
explicitly unconsumed:

```glsl
    // Reserved-and-unconsumed (#1926 / REN-D8-01): the aerial-perspective
    // fallback that read these two fields was removed once
    // VOLUMETRIC_OUTPUT_CONSUMED made it permanently dead. …
    vec4 fog_color;      // xyz = RGB, w = enabled (1.0 = yes)
    // Formula (currently unconsumed by any shader — see fog_color's
    // note above and #1927 / REN-D8-02) …
    vec4 fog_params;
```

`crates/renderer/src/vulkan/context/post_passes.rs:361-378` — the volumetric pass uses
`DEFAULT_SCATTERING_COEF`, `DEFAULT_PHASE_G`, and `DEFAULT_VOLUME_FAR`. Authored fog
color/density never drives the medium; `fog_far` is carried only as reach.

Closed #1926 / #1927 removed the dead composite branch. They did **not** connect the
authored data to the surviving volumetric path — that is the gap this issue tracks.

## Impact

Interior XCLL fog, WTHR haze, FNV cubic falloff, FO4 far-tint/max fields, and Starfield
height-fog semantics all parse successfully and produce no authored image. Cross-game,
and it affects the atmospheric read of essentially every cell that authors fog.

## Suggested Fix

Translate the canonical fog model into volumetric density, extinction, phase, and tint
inputs. Two guardrails from the closed-issue history:

- Preserve interior behaviour and authored curve semantics (the FNV cubic curve targets
  interiors — the removed consumer was exterior-gated and mixed toward sky-haze, which is
  why it was meaningless there).
- Do **not** simply resurrect the removed exterior-only composite mix.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Fog translation belongs at the EXAL boundary
      (`byroredux/src/env_translate.rs`), not re-derived per-game at render time — see
      `/audit-nifal` and `docs/engine/exal.md`
- [ ] **SIBLING**: Interior (XCLL) and exterior (WTHR) paths both covered, and the FNV
      cubic curve is exercised, not just the linear blend
- [ ] **TESTS**: A regression test pins shader *consumption*, not only CPU propagation
- [ ] Update `crates/renderer/shaders/composite.frag`'s "reserved-and-unconsumed" comment
      once the fields have a live consumer
