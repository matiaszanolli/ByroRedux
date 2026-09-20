# EXT-D7-2026-09-19-04: composite_term is wired but no script uses it — cycle/m34 gate the CPU sun value, never the rendered sky

- **ID**: EXT-D7-2026-09-19-04
- **Labels**: medium,terrain-exterior,bug,test-gap
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4491

**Severity**: MEDIUM · **Dimension**: Acceptance harness · **Game Affected**: all (Skyrim fixture most exposed)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D7-2026-09-19-04)

**Location**: `docs/smoke-tests/m-exteriors.sh:437-489` (cycle); `docs/smoke-tests/m34-day-night.sh` (whole file); wiring verified end-to-end at `crates/renderer/src/vulkan/render_debug.rs:13-30` → `crates/renderer/shaders/composite.frag:752` → `byroredux/src/boot/mod.rs:171` + `commands/world_info.rs:898`

**Description**
skyal.md §4 prescribes the sky-acceptance protocol: fixed camera, `--render-debug-mode composite_term` (pre-bloom, pre-tonemap, linear) to separate sky assembly from bloom/tonemap, plus a two-capture noise floor. The plumbing exists end to end, yet cycle mode and m34 pin only `sun: intensity=4.000/0.000` — a value read from the `SkyParamsRes` CPU resource (`byroredux/src/commands/time.rs:132`) — plus a loose `image_health` floor on final images. A sky that ignores the sun, a repeat of the skyal §1.1 bloom-gain wash, or a sun-direction bug (direction is printed, never checked) all pass every gate. m34 captures zero pixels.

**Impact**
The engine's headline exterior claim (day/night + climate-driven sun) has no GPU-output gate anywhere; skyal.md's own harness remains a manual recipe.

**Related**: EXT-D7-2026-09-19-06; skyal.md §1.1/§4

**Suggested Fix**
In cycle mode (or a dedicated sky smoke), capture each phase under `render.debug composite_term` and gate a cheap pixel invariant — e.g. noon-vs-night mean luminance delta above a floor, or noon frame sd above the washout line — so "sky responds to the sun" is checked in pixels, not in a struct.

## Completeness Checks
- [ ] **TESTS**: The composite_term captures double as the noise-floor pair skyal §4 asks for
