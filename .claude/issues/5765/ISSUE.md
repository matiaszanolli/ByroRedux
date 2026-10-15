# REN-D2-2026-10-14-01: `triangle.frag` loads from `reservoirsCurr`, declared `writeonly` (SPIR-V `NonReadable`); the nearer-fragment-keeps-the-claim RMW has no memory dependency

**Labels**: high,renderer,shaders,bug
**Source**: docs/audits/AUDIT_RENDERER_2026-10-14.md

Found by `/audit-renderer` (docs/audits/AUDIT_RENDERER_2026-10-14.md). Finding ID: `REN-D2-2026-10-14-01`. Status: NEW, confirmed against HEAD `adbd468e2`.

**Location**: `crates/renderer/shaders/triangle.frag` (ReSTIR finalize tail, `claimed` read + conditional `reservoirsCurr[pixelIdx] = rc` under `stableTemporalSurface`), from `c2647d468`; `crates/renderer/shaders/include/bindings.glsl` (`ReservoirCurrBuffer`, set 1 binding 16)

## Description
#5062 made binding 16 `writeonly` so sync validation would not classify it as a fragment `SHADER_STORAGE_READ` interface. `c2647d468` adds a load from it to implement "nearer fragment keeps the claim". glslang accepts it, but the SPIR-V decorates the block member and variable `NonReadable` and still emits an `OpLoad` through them; reading a `NonReadable` object is undefined. Independently of the qualifier, the claim is a non-atomic load-then-store: no pipeline barrier between one draw's store and the next draw's load, the binding is not `coherent`, and overlapping invocations on one pixel can interleave between load and store, so nearer-wins is only probabilistic.

## Evidence
- `bindings.glsl`: `layout(std430, set = 1, binding = 16) writeonly buffer ReservoirCurrBuffer { Reservoir reservoirsCurr[]; };` with a comment saying `triangle.frag` "only ever stores this frame's reservoirs".
- `spirv-dis triangle.frag.spv`: `OpMemberDecorate %ReservoirCurrBuffer 0 NonReadable`, `OpDecorate %__12 NonReadable`, plus an `OpAccessChain`/`OpLoad` at the `claimed` read. `triangle_early.frag.spv` carries the same decorations. The baseline (`5d31fd637`) SPIR-V has only the store. Independently confirmed by the Dim 7 agent.
- Both `.spv` files are byte-identical to a fresh `glslangValidator -V` compile of HEAD (not a stale artifact).
- No `shader_contract`/`restir` test mentions `claimed`, `reservoirsCurr` or binding 16's access mode.

## Impact
The commit measured a real win on the dev NVIDIA card (FO4 Third Rail: 26% of lit pixels no longer lose history every frame), so current visible behaviour is good. The risk is portability: another vendor/driver may compile the `NonReadable` load to garbage or a stale line, flipping the claim test at random and re-creating the history loss the commit removed, or leaving a farther hidden surface owning the slot. `BYRO_VALIDATION=1` is probably blind to it because of the `NonReadable` decoration, so the validation lane cannot clear it. Related: #5062 (closed), CONC-D2-2026-09-29-01 (predicted this RAW). Needs RenderDoc to verify the claim is stable under overdraw.

## Suggested Fix
Do not just delete `writeonly` (that reintroduces the #5062 validation red and leaves the race). Resolve ownership with a real atomic: a per-pixel `uint` claim word resolved with `atomicMin` on a (packedDist, order) key in a correctly qualified binding cleared by the same `begin_frame` fill, then write the full reservoir only for the winner; or resolve ownership from depth after the geometry pass. Confirm with RenderDoc on an overdraw-heavy interior and a `BYRO_VALIDATION=1` run, update the `bindings.glsl` comment, and add a source-shape pin for the chosen access mode (together with REN-D2-2026-10-14-03).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
