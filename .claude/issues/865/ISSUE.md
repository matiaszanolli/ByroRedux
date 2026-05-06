## FNV-D3-NEW-06: fog_clip / fog_power parsed for FNV but no shader code consumes them

## Source Audit
`docs/audits/AUDIT_FNV_2026-05-05.md` — Dim 3 LOW

## Severity / Dimension
LOW / Renderer × Cell lighting — parser-to-shader propagation gap

## Location
`crates/plugin/src/esm/cell/tests.rs:1683-1740` (parser test confirming FNV XCLL tail parses correctly)
`crates/renderer/src/vulkan/shaders/` (no consumer)

## Description
Strictly a propagation gap from the broader XCLL drop in `FNV-D3-NEW-02`. `fog_clip` (cubic-fog clip plane) and `fog_power` (cubic-fog falloff exponent) are FNV's fog-shaping fields per UESP / nif.xml. They've been parsed since #379 and have unit-test coverage at the plugin layer. **Neither field reaches the renderer.**

## Evidence
```bash
$ grep -rn "fog_clip\|fog_power" crates/renderer/ byroredux/src/render.rs
# (no output)
```

The parse-side fixture at `cell/tests.rs:1683-1740` confirms FNV XCLL tail decodes both fields; the renderer just never sees them.

## Impact
FNV's authored interior fog renders as plain linear fog (clip-at-`fog_far`, no power curve). Visible as too-much fog on close camera approaches in cells like Doc Mitchell's House and the Goodsprings Source Pump. Severity: LOW because the visual delta vs vanilla is "subtly off" rather than "broken".

## Suggested Fix
Plumb through with the umbrella `FNV-D3-NEW-02` (`CellLightingRes` extension). Add a `fog_curve` uniform on the geometry / sky-composite shaders; sample `pow(distance / fog_clip, fog_power)` instead of linear `(distance - fog_near) / (fog_far - fog_near)` when `fog_clip.is_some()`.

## Related
- `FNV-D3-NEW-02` (parent — XCLL extended fields drop). Fix this once the umbrella plumbs `CellLightingRes`.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: N/A
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Capture a screen ROI of a fixture FNV interior at `fog_clip=4096, fog_power=2.0` and pixel-diff against `fog_clip=8192, fog_power=1.0`. The pre-fix path would render identical pixels in both cases (`fog_far=8192` clamps both equally).
