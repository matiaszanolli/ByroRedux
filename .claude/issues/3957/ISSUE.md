# LC-2026-09-06-D5-02: Skyrim/FO4/FO76 fog falloff power is decoded and dropped at translate_exterior_cell_lighting, into a canonical field that exists and reaches the GPU

Issue: #3957 · Filed from `docs/audits/AUDIT_LEGACY_COMPAT_2026-09-06.md` (base `a8233f2f`)
Labels: low, legacy-compat, terrain-exterior, bug, game:skyrim, game:fo4, game:fo76

Reported by `/audit-legacy-compat` — `docs/audits/AUDIT_LEGACY_COMPAT_2026-09-06.md` (base `a8233f2f`).

- **Severity**: LOW (capture-completeness; the canonical field is currently shader-unconsumed, so filling it changes nothing visible today)
- **Dimension**: 5 — EXAL exterior environment → renderer
- **Location**: `byroredux/src/env_translate.rs:1045` (inside `translate_exterior_cell_lighting`, which hardcodes `fog_power: None`); decode at `crates/plugin/src/esm/records/weather.rs:490-491` (FO4/FO76) and `:894-895` (Skyrim); canonical field at `byroredux/src/components.rs:470`
- **Status**: NEW

## Description

`WeatherRecord::fog_day_power` / `fog_night_power` are decoded from `FNAM` on Skyrim, FO4 and FO76.

`CellLightingRes::fog_power` exists as the canonical landing site, is copied into the frame (`byroredux/src/components.rs:528`), and is uploaded to the GPU as `fog_params[3]` (`crates/renderer/src/vulkan/context/draw.rs:793`).

`translate_exterior_cell_lighting` nonetheless writes `fog_power: None` unconditionally, under a comment that explains only the *XCLL* source ("the extended XCLL tail applies to interior cells (and not-yet-wired exterior lighting overrides). #861") and does not mention that WTHR carries its own authored value for exteriors.

## Evidence

`byroredux/src/env_translate.rs:1045` — inside `translate_exterior_cell_lighting` (`:1023`):

```rust
// WTHR-driven exterior lighting; the extended XCLL tail applies to
// interior cells (and not-yet-wired exterior lighting overrides). #861.
directional_fade: None,
fog_clip: None,
fog_power: None,
```

`crates/plugin/src/esm/records/weather.rs:894-895` — the Skyrim 32-byte `FNAM` arm:

```rust
record.fog_day_power = r.f32().unwrap_or(1.0);
record.fog_night_power = r.f32().unwrap_or(1.0);
```

pinned at `weather.rs:1257-1258` (0.45 / 0.25).

## Impact

**None visible today.** `crates/renderer/src/vulkan/context/draw.rs:1508-1523` documents `fog_clip` and `fog_power` as "**Currently unconsumed** (#1926, #1927)" — the `composite.frag` branch that read the curve was removed once `VOLUMETRIC_OUTPUT_CONSUMED` made it unreachable, and the fields are "reserved for a future interior-scoped composite branch that mixes toward `fog_color`".

So this is a gap in what the boundary *captures*, not in what renders. It is filed separately from #3956 precisely so the two are not conflated: that one has a live sink, this one does not.

## Related

- #3956 (LC-2026-09-06-D5-01) — same subrecord tail, live sink, MEDIUM
- #1926, #1927 — the removed composite fog branch that made `fog_power` unconsumed

## Suggested Fix

Fill `fog_power` from `wthr.fog_day_power` in `translate_exterior_cell_lighting` when the game decodes it, so the value is already canonical if and when the interior-scoped composite branch lands.

One line. The alternative — leave it, and say why — is equally defensible, in which case the `#861` comment should state "WTHR's own power is intentionally not forwarded because the field is shader-unconsumed", which it currently does not. Either outcome closes this; silently leaving the comment misleading does not.

## Completeness Checks
- [ ] **SIBLING**: `fog_clip` sits on the same line group with the same unconsumed status — decide both together
- [ ] **CANONICAL-BOUNDARY**: the per-game decision stays at the EXAL parser→canonical boundary in `byroredux/src/env_translate.rs`
- [ ] **TESTS**: If the value is forwarded, a regression test pins that an authored Skyrim/FO4 `fog_*_power` reaches `CellLightingRes::fog_power`
