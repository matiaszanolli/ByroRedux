# #2268: TD8-003: Dead NIF particle-modifier back-compat shims whose own 'few internal call sites' premise is no longer true

Labels: bug, nif-parser, low, tech-debt

---

**Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
**Location**: `crates/nif/src/blocks/particle.rs:322-329` (`parse_color_modifier`), `:607-613` (`parse_simple_color_modifier`)
**Status**: NEW (the code itself predates the current audit window by several weeks; it carries no `#[allow(dead_code)]` because both are `pub fn`, which suppresses rustc's dead-code lint even though nothing calls them — invisible to the standard `allow(dead_code)`-grep discovery method, found here by cross-checking call sites directly)

**Description**: Both functions are explicitly documented as "Back-compat shim — earlier dispatch returned a `NiPSysBlock` for every modifier subtype. Kept so the few internal call sites that only need byte-correct stream advancement still compile." That claim is false today: the block dispatcher (`crates/nif/src/blocks/mod.rs`) calls `NiPSysColorModifier::parse`/`BSPSysSimpleColorModifier::parse` directly, exactly as the shims' own doc comments recommend "new code" do.

**Evidence**:
```rust
/// Back-compat shim — earlier dispatch returned a `NiPSysBlock` for
/// every modifier subtype. Kept so the few internal call sites that
/// only need byte-correct stream advancement still compile, but new
/// code should call [`NiPSysColorModifier::parse`] directly.
pub fn parse_color_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _modifier = NiPSysColorModifier::parse(stream)?;
    Ok(NiPSysBlock { original_type: "NiPSysColorModifier".to_string() })
}
```
`grep -RIn "parse_color_modifier(\|parse_simple_color_modifier(" crates/nif/src` finds no call sites at all outside the two function definitions.

**Impact**: Cosmetic/maintenance only — dead `pub fn` surface in a parser crate with zero external consumers. Because they're `pub` (not `pub(crate)`), `cargo check`/clippy don't flag them, so this class of rot is invisible to the compiler and will persist indefinitely unless someone greps for call sites directly (as done here).

**Suggested Fix**: Delete both functions. Nothing depends on the `NiPSysBlock`-returning shape for these two block types anymore.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable




# #2367: PERF-REGRESSION-3a02b02d..28155b79: FO4 scenes (MedTek/Dugout) ~33-34% slower at flat entity count; Prospector (FNV) ~2x faster — needs bisection

Labels: bug, medium, performance, game:fnv, game:fo4

---

**Severity**: MEDIUM
**Dimension**: Performance / Bench-of-record (ROADMAP #2279 refresh)
**Location**: unknown — needs bisection across `3a02b02d..28155b79` (119 commits, spans Session 60-62 including procedural volumetric fog, clustered local fog volumes, material-aware path-traced GI extensions, materials-pipeline refactor `ImportedMaterial`/`MaterialTextureSet<T>`, streaming-resumability mitigations)

## Description

Refreshing the ROADMAP bench-of-record (#2279) surfaced large swings against the prior record (`3a02b02d`, 2026-07-26). Per this project's own standing methodology (documented in ROADMAP's Bench-of-record section), a same-session same-machine worktree rebuild of the prior commit was run as a control before drawing any conclusion — this is what separated PERF-REGRESSION-6c56e311 (#2161-adjacent) from machine noise previously, and does the same here.

Control (`3a02b02d` rebuilt in a worktree) vs HEAD (`28155b79`), same session/machine, TAA config, median of 3 runs x 300 frames:

| Scene | Entities (ctrl→HEAD) | TAA frame ms (ctrl→HEAD) | Verdict |
|---|---|---|---|
| Prospector (FNV) | 3626→3626 (flat) | 14.69→7.33 | **Real ~2x improvement** |
| Cornell (synthetic control) | 25→27 (flat) | 2.76→3.32 | Real but mild slowdown (~20%) |
| Whiterun (Skyrim SE) | 3406→5150 (+51%) | 9.99→15.37 | Confounded by entity growth — not conclusive |
| MedTek Research 01 (FO4) | 31495→31400 (flat) | 40.17→53.58 | **Real ~33% regression, flat content** |
| Dugout Inn (FO4) | 6978→6978 (flat) | 30.44→40.79 | **Real ~34% regression, flat content** |

The control run reproduces the original `3a02b02d` ROADMAP figures closely (e.g. Prospector 65.3→68.1 FPS, Dugout 31.9→32.9 FPS — within normal same-machine noise), which is what makes the HEAD deltas trustworthy rather than contention artifacts.

## Evidence

Full control-run report and HEAD report available in this session's bench output (`target/fsr-bench/raw.tsv` at both commits). Both regressed scenes are Fallout 4 content; the dramatically improved scene is FNV; the synthetic control is only mildly affected — this points at something FO4-specific rather than a universal engine regression, but that is a pattern observation, not a root cause.

## Impact

Two real Fallout 4 interior scenes are ~33-34% slower in frame time at byte-identical entity counts. Whiterun's entity count grew +51% over the same range (3406→5150) for reasons not yet understood — worth separately investigating since it could itself be either a content-loading behavior change or a symptom of the same underlying cause.

## Suggested Fix

Bisect `3a02b02d..28155b79` using `scripts/fsr-bench-matrix.sh` restricted to Dugout (smaller/faster-loading of the two regressed FO4 scenes, better bisection candidate than MedTek) at TAA-only to narrow the commit range efficiently. Prime suspects given the commit range's content: the procedural volumetric fog / clustered local fog volumes work and the material-aware path-traced GI extensions (both Session 62), since those are the kind of per-fragment cost additions that would hit FO4's higher-poly interiors harder — but this is a hypothesis, not yet verified.

## Completeness Checks
- [ ] **BISECT**: Narrow `3a02b02d..28155b79` to the actual introducing commit(s)
- [ ] **WHITERUN-ENTITY-COUNT**: Understand why Whiterun's loaded entity count grew 3406→5150 (+51%) over the same range
- [ ] **PROSPECTOR-IMPROVEMENT**: The ~2x Prospector improvement is also unexplained — confirm it's real engine work and not a measurement artifact before citing it as a win
- [ ] **TESTS**: N/A until root-caused — this is a measurement/bisection issue, not a code-fix issue yet



# #2383: Report of found (potentially applicable to upstream) issues while trying out engine with non-Bethesda titles

Labels: bug, nif-parser, renderer, import-pipeline

---

Found while porting a non-Bethesda title (The Guild 2 Renaissance) onto ByroRedux as a rendering substrate, on an AMD RX 590 (Polaris, no RT hardware). All five have a working fix in our fork at commit `04befb4` (branch `main`, forked from your `main` at `9f61935`) — happy to open PRs instead of/alongside issues if you'd rather review a diff than a description.

---

## 1. Loose-file (non-archive) NIF/mesh loads never call `flush_pending_uploads` — real textures silently stay on the placeholder forever

**Symptom:** any content loaded via the loose-NIF/mesh/tree path renders either pure black (non-alpha-test materials) or fully invisible (alpha-test materials discard every fragment against the placeholder's alpha channel). Looks exactly like a missing-content or bad-decode bug — it isn't. Material data, alpha state, mip levels, DDS decode are all correct the whole time.

**Root cause:** `TextureRegistry::enqueue_dds_with_clamp` reserves a bindless slot and points it at the checkerboard placeholder immediately, then queues the real decode+upload for a later `flush_pending_uploads()` call. That call exists on the cell-loading path (`cell_loader/references/mod.rs`) and the exterior-streaming path (`streaming_helpers.rs`) — but the loose-file loading path in `byroredux/src/scene.rs` never calls it. The upload is queued and then simply never happens; nothing errors.

**Repro:** load any standalone NIF with real (non-placeholder) textures via the loose-file CLI path, e.g. a barrel/prop mesh with a diffuse texture. Expect to see it textured; instead it's either invisible (alpha-test) or flat near-black (opaque).

**Fix (in our fork, `byroredux/src/scene.rs`):** after the loose-NIF load completes, check `pending_dds_upload_count()` and call `flush_pending_uploads` if nonzero — same call the cell-loader path already makes, just added to the third loading path that was missing it.

---

## 2. `shaderInt64` and `bufferDeviceAddress` device features incorrectly gated on ray-query support

**Symptom:** on any GPU without ray-query hardware, pipelines fail to build for shaders that declare a 64-bit type or buffer-reference pointer even though nothing about them requires ray tracing — e.g. `ui.vert`'s shared `GpuInstance` struct layout.

**Root cause:** in `crates/renderer/src/vulkan/device.rs`, `shaderInt64` was never enabled at all (`shader_int64(false)` effectively, unconditionally), and `bufferDeviceAddress` was enabled via `.buffer_device_address(caps.ray_query_supported)` — both wired to ray-query availability. Both are Vulkan 1.2 core features, independent of RT hardware. Confirmed via `vulkaninfo` on this machine's AMD RADV Polaris10 (RX 590): `bufferDeviceAddress = true`, `rayQuery` not exposed at all — a real, common combination for pre-RDNA2 AMD and older NVIDIA/Intel parts, not just a hypothetical.

**Fix (in our fork, `crates/renderer/src/vulkan/device.rs`):** probe both features independently at device-suitability time (`shader_int64_supported`, `buffer_device_address_supported` added to `DeviceCapabilities`) and enable each based on its own probed support instead of `ray_query_supported`.

---

## 3. Loose texture path resolution breaks on Linux for Windows-backslash-authored paths

**Symptom:** on the `--loose-textures` fallback path, any texture reference authored with a Windows-style directory prefix (`textures\clutter\barrel01.dds` — the common Bethesda-tooling convention, and also present in some Guild II content) fails to resolve on Linux/macOS hosts, even though the file is present and correctly indexed.

**Root cause:** the original stem extraction used `std::path::Path::file_stem()`, which only recognizes `/` as a separator on Unix-like hosts. A backslash-joined path like `textures\clutter\barrel01` isn't split at all — `file_stem()` returns the whole string minus `.dds` (`textures\clutter\barrel01`) instead of just `barrel01`, so it never matches the (correctly stem-keyed) index. Content authored with bare filenames only (no directory prefix) happened to dodge this entirely, which is likely why it hadn't surfaced before — but any Bethesda-convention full relative path hits it on Linux.

**Fix (in our fork, `byroredux/src/asset_provider/texture.rs`):** split on both `\` and `/` by hand (`path.rsplit(['\\', '/']).next()`) before taking the stem, rather than relying on `Path`.

---

## 4. Camera spawn orientation bakes in unwanted roll via `Quat::from_rotation_arc`

**Symptom:** the very first mouse touch after spawning visibly snaps/rolls the camera, most noticeably when the initial look direction is steep (looking mostly up or down at spawn).

**Root cause:** the spawn camera's initial orientation was built with `Quat::from_rotation_arc(-Z, forward)` — a shortest-arc rotation with no "up" constraint, so it bakes in real roll whenever `forward` is close to the poles. Meanwhile `fly_camera_system` drives every subsequent frame from separate `yaw`/`pitch` scalars (defaulting to `0`), with no roll term at all. The instant the mouse moves, the camera snaps from the rolled spawn orientation to the yaw/pitch-only convention every later frame uses — reads as a jarring "camera spins on touch" bug.

**Fix (in our fork, `byroredux/src/scene.rs`):** build the initial rotation the same way every later frame does — decompose `forward` into `yaw`/`pitch` (`Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch)`) and seed `InputState.yaw`/`.pitch` to match, instead of a separate roll-prone shortest-arc rotation.

---

## 5. `cam_center` is never computed on the loose-NIF/mesh spawn path, defaults to world origin

**Symptom:** the default spawn camera pitch for a loose-file load is arbitrary and often steep/wrong, seemingly unrelated to where the loaded content actually sits — e.g. a building loaded standalone can end up staring at the ceiling.

**Root cause:** `cam_center` is only ever assigned a real value on the ESM/cell-loading path and the `--cornell` harness path. The loose-NIF/mesh/tree path never sets it, so it silently keeps its `Vec3::ZERO` default. The spawn camera aims "at `cam_center`" regardless of path, so a loose load ends up aiming at literal world origin `(0,0,0)` — correct only by coincidence when the model happens to be authored near the origin, arbitrarily wrong otherwise (e.g. a building translated away from the origin, or authored tall relative to it).

**Fix (in our fork, `byroredux/src/scene.rs`):** for the loose-NIF path when `cam_center` is still at its zero default, aim level/forward from `cam_pos` instead of at the origin.

---

## Borderline — reporting, less confident these need action

### A. `StagingGuard` never flushes non-coherent host-visible memory before GPU reads

`StagingGuard` (`crates/renderer/src/vulkan/buffer.rs`) writes through a mapped pointer but had no equivalent of `Buffer::flush_if_needed` — per the Vulkan spec, a host write to non-`HOST_COHERENT` mapped memory isn't guaranteed visible to the device without an explicit `vkFlushMappedMemoryRanges` call before the device reads it. We never actually observed a visible artifact from this on our hardware (this GPU's staging memory allocations happen to come back coherent), so this is a "found by reading the allocator/spec contract," not a reproduced bug — but it's a real gap that would only show up on a device/driver combination that hands back non-coherent host-visible memory for staging buffers, which is allowed by spec. Fix added defensively in our fork (`StagingGuard::flush_if_needed`, called at the one call site that writes a DDS texture through a `StagingGuard`) — worth a look, not worth losing sleep over if it doesn't fit your priorities.

### B. `--loose-textures` indexer only scans `.dds`, silently skips `.tga`

The loose-texture indexer (`TextureProvider::add_loose_texture_root`) only walks `.dds` files. Some titles/mods ship (or reference) `.tga` textures directly rather than pre-converted DDS — those are silently invisible to the loose-file fallback with no warning that they exist but were skipped. Minor/feature-shaped rather than a correctness bug — we didn't fix this in our fork (Guild II's `.tga` files happen not to be on the hot path we needed), flagging in case it's useful, not proposing a specific fix.




