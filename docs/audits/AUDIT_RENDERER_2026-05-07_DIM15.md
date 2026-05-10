# Renderer Audit — Dimension 15 Focused — 2026-05-07

**Scope**: Sky / Weather / Exterior Lighting (M33 / M33.1 / M34) — focused single-dimension audit
**Coverage**: Dim 15 only (`--focus 15`)
**Repo state**: `main` @ HEAD (commit `386aabb`)
**Prior baseline**: `docs/audits/AUDIT_RENDERER_2026-05-07.md` (full audit, today). Dim 15 in that report deferred to documented prompt contracts; this focused audit walks the actual code paths.

## Executive Summary

**Verdict: 4 INFO findings, no CRITICAL / HIGH / MEDIUM / LOW.**

The sky/weather/exterior pipeline is structurally correct, well-instrumented, and pinned by 5+ unit tests on the CPU-side directional upload. The four findings below are **comment-vs-math honesty**, **design-scope assumptions**, and **cosmetic visual variety** — none are correctness bugs.

| Finding | Severity | Class |
|---------|----------|-------|
| REN-D15-01 | LOW | Fog distance breakpoints hardcoded; color breakpoints CLMT-driven |
| REN-D15-02 | INFO | Cumulative interior-fill scale 0.6 × 0.4 = 0.24× doesn't match shader-comment claim |
| REN-D15-03 | LOW | Cloud layers 2/3 share scroll rate with 0/1 (cross-cuts #541) |
| REN-D15-04 | INFO | Sun south tilt hardcoded `z = -0.15` (acceptable for all targeted games) |

**Audit-prompt correction**: The 2026-05-07 full-audit dim_15 incorrectly called the prompt's "Interior fill at 0.6× ambient + `radius=-1`" claim "stale". The CPU-side 0.6 IS still in place at `render.rs:159`; the audit-prompt is **correct**. The shader-side 0.4 (`triangle.frag:2053`) is **in addition to**, not in place of, the 0.6× CPU multiplier. See REN-D15-02 for the cumulative impact.

## Sky / Weather / Exterior Lighting Assessment

**Overall: spec-correct, well-tested.** The pipeline:

1. `weather_system` (`systems.rs:1409-1777`) advances the game clock, picks TOD slot pair via CLMT-driven `build_tod_keys`, lerps 7 sky color groups + fog distance, supports cross-fade via `WeatherTransitionRes`, computes sun arc + intensity, and updates `SkyParamsRes` + `CellLightingRes` (only on exterior cells, per #782 fix).
2. `compute_directional_upload` (`render.rs:153-179`) projects the cell's authored directional color into the per-frame `GpuLight` SSBO with `radius == -1` flag for interior fill (skips RT shadow rays) or sun_intensity-ramped for exterior. Pinned by 5 unit tests covering noon / midnight / sunrise / out-of-range / interior independence.
3. `triangle.frag` (`:2042-2057`) consumes `radius < 0.0` as the interior-fill signal, accumulates `lightColor × atten × albedo × 0.4` (isotropic, BRDF skipped), and `continue;`s past the WRS shadow-ray streaming block.

**M33.1 + M34 fixes intact**:
- All 4 cloud layers active with `rem_euclid(1.0)` scroll wrap (`systems.rs:1709-1731`).
- `CloudSimState` survives cell transitions (#803 lift).
- Interior cells preserve XCLL/LGTM authored values (#782 — `weather_system` only writes when `!cell_lit.is_interior`).
- Default exterior sun intensity peaks at 4.0 hours 7..=17, fades at sunrise/sunset (`systems.rs:1661-1669`).
- Sun-disc render gated on `dir.y > 0` (#800, commit 0a10ec1) so the down-pointing night vector doesn't paint below-horizon ground.
- Weather cross-fade independently TOD-samples each side (correct cross-midnight behaviour) before per-channel lerp on `transition_t`.

**RT integration**:
- Interior-fill `radius == -1` shader gate skips shadow rays — sealed-wall leak protection.
- `!isInteriorFill` belt-and-suspenders guard on the WRS shadow-ray streaming block (`triangle.frag:2113`) prevents accidental shadow rays for fill lights if a future edit relaxes the early `continue`.

## Findings

### [LOW] REN-D15-01 — Fog distance breakpoints hardcoded; color breakpoints CLMT-driven

**Dimension**: Sky / Weather / Exterior Lighting
**File**: `byroredux/src/systems.rs:1531-1541`

The `night_factor` for fog distance interpolation uses hardcoded hours (6, 18, 20, 4):

```rust
let night_factor = if (6.0..=18.0).contains(&hour) { 0.0 }
                   else if hour >= 20.0 || hour <= 4.0 { 1.0 }
                   else if hour > 18.0 { (hour - 18.0) / 2.0 }
                   else { (6.0 - hour) / 2.0 };
let fog_near = wd.fog[0] + (wd.fog[2] - wd.fog[0]) * night_factor;
let fog_far  = wd.fog[1] + (wd.fog[3] - wd.fog[1]) * night_factor;
```

But the **color** breakpoints come from `build_tod_keys(wd.tod_hours)` (`:1456`) which IS CLMT TNAM-driven (#463). For a CLMT shipping non-default sunrise (FO3 Capital Wasteland is the documented case — sunrise ~0.3 h earlier than the 6.0 default per ROADMAP context), the **palette** smoothly transitions at the climate's hours while the **fog distance** snaps at the hardcoded breakpoints.

**Symptom**: ~0.3-2.0 h window where palette says "day" but fog interpolates as "transitioning" (or vice versa). Visual continuity glitch on non-default-hour CLMTs. Not a NaN / pitch-black regression.

**Fix sketch** (~10 lines): Derive `night_factor` from the same `keys` table that drives color interpolation. Cleanest contract: factor from `slot_b` (`TOD_NIGHT → 1.0`, `TOD_DAY → 0.0`, others → 0.5 weighted by `t`).

**Repro**: Static analysis only. Visual repro: load a CLMT with non-default sunrise hours, watch fog distance lerp vs sky palette during sunrise.

### [INFO] REN-D15-02 — Cumulative interior-fill scaling 0.6 × 0.4 = 0.24× doesn't match shader-comment claim

**Dimension**: Sky / Weather / Exterior Lighting (interior fill cross-cut)
**File**: `byroredux/src/render.rs:159` (CPU `INTERIOR_FILL_SCALE = 0.6`) + `crates/renderer/shaders/triangle.frag:2053` (`INTERIOR_FILL_AMBIENT_FACTOR = 0.4`)

The interior-fill brightness has TWO independent multipliers:

1. **CPU upload** (`compute_directional_upload`): `directional_color × 0.6` (`render.rs:159`).
2. **Shader accumulation** (post-`98d644c`): `lightColor × atten × albedo × 0.4` (`triangle.frag:2053-2055`).

Net: `directional × 0.6 × 0.4 × albedo = directional × 0.24 × albedo` (`atten ≈ 1` for fill).

The shader comment at `:2045-2052` claims the 0.4 "rebalances the dropped Lambert term so a fragment with `NdotL ≈ 0.5` (the half-Lambert midpoint) receives **roughly the same brightness** it did pre-isotropic." Per arithmetic:

| Path | Diffuse @ NdotL=0.5 | Net fill |
|------|---------------------|----------|
| Pre-isotropic (half-Lambert) | `0.5 × 0.5 + 0.5 = 0.75` | `directional × 0.6 × 0.75 × albedo ≈ 0.45 × directional × albedo` |
| Post-isotropic | constant `0.4` | `directional × 0.6 × 0.4 × albedo = 0.24 × directional × albedo` |

**0.24 / 0.45 ≈ 53%** — interiors are ~47% dimmer at the half-Lambert midpoint, **not "roughly the same."**

The dim-down may be **the user's intent** (the corrugated-metal stripe pathology was the regression class; isotropic uniformly-darker beats banded brighter). But the comment's perceptual claim is calibrated to a metric the math doesn't bear out.

**Symptom**: Interiors visibly dimmer when A/B'd against the legacy half-Lambert path via `BYROREDUX_RENDER_DEBUG=0x200`. Not a correctness regression — the value was user-tuned. Comment-vs-math gap only.

**Fix sketch** (preferred): Update the shader comment to reflect actual semantic: "0.4 was tuned by visual judgment to land in the perceived range of the legacy half-Lambert at typical XCLL directional magnitudes." (preserves honesty about the tuning rather than asserting a perceptual equivalence the math doesn't bear out).

**Alternative** (only if the user wants closer parity): bump `INTERIOR_FILL_AMBIENT_FACTOR` to ~0.7 — `0.6 × 0.7 = 0.42` net midpoint vs old 0.45 → ~7% gap, well inside perceptual indistinguishability. Visual judgment required; defer to user.

**Repro**: A/B with `BYROREDUX_RENDER_DEBUG=0x200` on a Megaton-class interior. Pre-isotropic looks brighter; spec banding returns on metallic walls (the regression class the new path fixes).

### [LOW] REN-D15-03 — Cloud layers 2/3 share scroll rate with layers 0/1

**Dimension**: Sky / Weather / Exterior Lighting
**File**: `byroredux/src/systems.rs:1721-1731`
**Status**: cross-cuts open #541 ("unused WTHR fields" — ONAM/INAM decode)

Layer 2 (WTHR ANAM texture) scrolls at `cloud_scroll_rate × dt` — identical to layer 0 (DNAM). Layer 3 (BNAM) scrolls at `-cloud_scroll_rate × 1.35 × dt` — identical to layer 1 (CNAM). The visible parallax depends entirely on layers 2/3 sampling different **textures** from layers 0/1; the **velocity** is duplicated.

When ANAM/BNAM textures resemble DNAM/CNAM (common case — many WTHR records ship the same path strings), layers 2/3 add no visual variety beyond a 1-texture-lookup difference at the same scroll velocity.

This is a **known simplification** documented at `:1672-1681`: "the real per-weather scroll source stays unknown" pending UESP-authoritative byte sampling on ONAM (4 B) / INAM (304 B). #541 covers the path forward.

**Fix sketch**: Wait for #541. Interim cosmetic mitigation: add a `+/-` direction flip and stride bump on layers 2/3 so the 4 layers have 4 distinct velocities (e.g. layer 2 at 0.85×, layer 3 at -1.15×).

### [INFO] REN-D15-04 — Sun south tilt hardcoded `z = -0.15`

**Dimension**: Sky / Weather / Exterior Lighting
**File**: `byroredux/src/systems.rs:1650`

The sun arc applies a constant `z = -0.15` south tilt. Acceptable for all currently-targeted games (Oblivion / Fallout 3 / FNV / Skyrim / Fallout 4 / Starfield Earth-analogues — all northern-hemisphere settings). A hypothetical future Tamriel-like world with a different solar latitude would inherit this constant.

**Fix sketch** (defer): If/when Starfield off-world cells are wired up, surface as per-worldspace constant on the WRLD record or as `f32` on `SkyParamsRes`. Not actionable today.

## Prioritized Fix Order

1. **REN-D15-01** (LOW) — fog/color breakpoint asymmetry. ~10-line fix; visual-continuity improvement on non-default CLMTs. Worth filing as a tracker issue.
2. **REN-D15-02** (INFO) — shader comment honesty. 1-3 line comment edit; no behaviour change. Worth filing as a docstring fix.
3. **REN-D15-03** (LOW) — cloud variety; covered by #541. Add cross-reference comment if not already present.
4. **REN-D15-04** (INFO) — design-scope assumption; defer until Starfield off-world.

**No correctness fixes required.** All findings are continuity / honesty / cosmetic.

## Cross-Dimension Notes

- **REN-D15-02 cross-cuts Dim 6 (Shader Correctness)**: the shader comment at `triangle.frag:2045-2052` is the artifact at issue, not the math itself. Filing the fix in either dimension is fine; flagged here because the upstream CPU multiplier (`render.rs:159`) is what makes the comment claim drift from the math.
- **REN-D15-01 cross-cuts Dim 10 (Composite)**: composite applies fog to direct lighting; the WRONG fog distance from REN-D15-01 propagates through composite. Severity stays LOW because the asymmetry window is bounded and visual-continuity, not numerical-stability.
- **The 2026-05-07 full audit's claim that the prompt's "0.6× ambient" was stale was incorrect.** The CPU 0.6 × shader 0.4 is the actual chain; that audit's hasty correction stands corrected here. The audit prompt's checklist line 9 ("Interior fill at 0.6× ambient + `radius=-1` (unshadowed)") is **accurate**.

## Verifications

- ✅ `weather_system` clock advancement (monotonic, wraps at 24)
- ✅ TOD breakpoints CLMT-driven via `build_tod_keys` (#463)
- ✅ Sun arc east → up → west, slight south tilt, `(0,-1,0)` at night
- ✅ Sun intensity 4.0 peak hours 7..=17, fade at sunrise/sunset, 0 at night (matches `SUN_INTENSITY_PEAK = 4.0` per render.rs unit test)
- ✅ Weather cross-fade independent TOD-sample per side (cross-midnight correct)
- ✅ Cloud scroll wrap-at-1.0 via `rem_euclid(1.0)`
- ✅ `CloudSimState` lift (#803) — survives cell transitions
- ✅ Interior cells preserved (#782) — `weather_system` only writes on exterior
- ✅ Sun-disc gated on `dir.y > 0` (#800)
- ✅ Sky gradient + cloud layers + fog channelisation per Dim 10 invariant (fog → direct only)
- ✅ Interior fill `radius < 0.0` shader gate intact at `triangle.frag:2042`
- ✅ Belt-and-suspenders `!isInteriorFill` on WRS shadow-ray streaming at `:2113`
- ✅ Specular-AA debug bit `0x100` intact at `:719`
- ✅ Half-Lambert-fill debug bit `0x200` intact at `:2044`
- ✅ Glass-passthru debug bit `0x80` intact at `:1591, :1642, :1791`
- ⚠ Fog distance breakpoint asymmetry (REN-D15-01) — LOW
- ⚠ Cumulative 0.6 × 0.4 = 0.24× scaling vs shader comment claim (REN-D15-02) — INFO
- ⚠ Cloud layer 2/3 scroll rate duplicates 0/1 (REN-D15-03) — LOW (covered by #541)
- ⚠ Sun south tilt hardcoded (REN-D15-04) — INFO (out-of-scope today)

---

**Report generation**: Direct line-walk audit using the project's 1M-context model — same approach that completed the 2026-05-07 full audit when delegated subagents truncated.

Suggested next step: `/audit-publish docs/audits/AUDIT_RENDERER_2026-05-07_DIM15.md` if REN-D15-01 + REN-D15-02 are worth filing as tracker issues. The other two (D15-03 covered by #541, D15-04 defer) are no-action-today.
