# 4580: REN-D1-2026-09-21-02: `TRIANGLE_FACING_CULL_DISABLE` is inert — no ray query sets a facing-cull flag, so #416's premise and `tlas.rs`'s "RT honors two_sided / ~2× ray cost" comment are false

State: OPEN  Labels: ['documentation', 'renderer', 'low', 'vulkan', 'doc-rot']

**Severity**: LOW (documentation/premise drift; no runtime change is wanted) · **Dimension**: AS Correctness
**Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs` (~:591-604): the `TRIANGLE_FACING_CULL_DISABLE` gate on `draw_cmd.two_sided` and its comment. Also every `rayQueryInitializeEXT` under `crates/renderer/shaders/`.
**Status**: NEW
**Verified against**: HEAD `f97775ca8`

## Description

`VK_GEOMETRY_INSTANCE_TRIANGLE_FACING_CULL_DISABLE_BIT_KHR` matters only when a ray asks for facing culling with `gl_RayFlagsCullBackFacingTrianglesEXT` or `gl_RayFlagsCullFrontFacingTrianglesEXT`. No ray query in the shader tree does. Every query uses `gl_RayFlagsOpaqueEXT`, sometimes with `| gl_RayFlagsTerminateOnFirstHitEXT`. So every ray already sees both faces of every triangle, whether or not the instance sets the bit.

The comment on the gate makes two claims, and neither is true:
- that pre-#416 "every instance disabled backface culling, so shadow / GI rays hit the interior backfaces of closed single-sided meshes … ~2× ray cost";
- that "the RT path now honors the same bit".

Toggling the bit changes nothing. Both-face visibility is also why single-sided walls block light from behind, which bears on the `f97775ca8` light-leak investigation.

## Evidence

- `grep -rhoE "gl_RayFlags[A-Za-z]+EXT" crates/renderer/shaders/` finds only `gl_RayFlagsOpaqueEXT` (17 sites) and `gl_RayFlagsTerminateOnFirstHitEXT` (6 sites). `CullBackFacing` and `CullFrontFacing` appear nowhere.
- In `tlas.rs`, `let instance_flags = if draw_cmd.two_sided { TRIANGLE_FACING_CULL_DISABLE } else { 0 };` sits under the comment quoted above.

## Impact

This is documentation and audit-premise drift only. Anyone reasoning from the comment, or from #416, about RT back-face behaviour, ray cost, or two-sided leak mechanics gets the wrong model. The audit-renderer skill repeats the premise ("a blanket enable is the regression (~2× ray cost)").

## Related

- #416 (closed): introduced the gate on this premise.
- REN-D1-2026-09-21-01 (#4576): the shadow-mask policy change whose investigation leaned on this premise.

## Suggested Fix

Correct the `tlas.rs` comment, and the Dim-1 premise in `.claude/commands/audit-renderer/SKILL.md`. Both should say the bit is inert: no ray query requests facing culling, so RT always sees both faces. Do **not** add cull flags to the ray queries. That would open leaks through single-sided room shells, which today block light from both sides.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D1-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: the audit-renderer skill's Dim-1 line (`TRIANGLE_FACING_CULL_DISABLE … ~2× ray cost`) corrected in the same sweep
- [ ] **TESTS**: optional — a source-shape pin that no shader requests `gl_RayFlagsCull*FacingTrianglesEXT` without the comment being revisited


---

# 4581: REN-D1-2026-09-21-03: `fill_shadow_mask_census`'s #4518 comment describes an `AlphaBlend` divert cause and a pending field removal that no longer exist

State: OPEN  Labels: ['documentation', 'renderer', 'low', 'vulkan', 'doc-rot']

**Severity**: LOW (stale comment) · **Dimension**: AS Correctness
**Location**: `crates/renderer/src/vulkan/context/telemetry.rs` `fill_shadow_mask_census` (~:429-435)
**Status**: NEW
**Verified against**: HEAD `f97775ca8`

## Description

The #4518 comment in `fill_shadow_mask_census` reads:

> `actor_diverted_alpha_blend` is no longer published: the divert returns `AlphaBlend` only for non-Actors (`mask_divert_cause`) while this census's breakdown is Actor-guarded, so the counter was structurally zero … Dropping the field itself spans the renderer census + core `ShadowMaskCensus` and is tracked separately.

Both halves are now stale:
- `f97775ca8` removed the blend divert. `MaskDivertCause` (`acceleration/predicates.rs`) has only `RefractiveGlass`, `EffectShader` and `FireRefraction`, and `mask_divert_cause` never returns an `AlphaBlend` cause.
- The field is already gone: `actor_diverted_alpha_blend` appears nowhere in the tree except this comment.

## Evidence

- `grep -rn "actor_diverted_alpha_blend" crates/renderer/src byroredux/src` matches only `context/telemetry.rs`, in the comment itself.
- `grep -n "AlphaBlend" crates/renderer/src/vulkan/acceleration/predicates.rs` finds no match.

## Impact

The comment at the census site is misleading. It tells a reader about a live divert cause and an open follow-up, and neither exists. No runtime effect.

## Related

- #4518 (closed): wrote this comment.
- REN-D1-2026-09-21-01 (#4576): `f97775ca8`'s divert removal, which made the comment's cause obsolete.

## Suggested Fix

Replace the comment with one line saying that both the alpha-blend divert and its census field were removed (`f97775ca8`, #4518), or delete it.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D1-2026-09-21-03)

## Completeness Checks
- [ ] **SIBLING**: other comments naming the retired blend divert (`mask_divert_cause` doc, `rt.masks` display, the `tlas.rs` census block) checked in the same sweep


---

# 4582: REN-D2-2026-09-21-01: the RESTIR_LIGHT debug block re-binds `triangle.frag`'s legacy-WRS `#if ENABLE_LEGACY_WRS else` from `if (useRestir)` to `if (viewRestirLight)`

State: OPEN  Labels: ['bug', 'renderer', 'low', 'shaders']

**Severity**: LOW (evaluation build only: `ENABLE_LEGACY_WRS == 1`; the default build compiles the `else` out) · **Dimension**: Ray Queries
**Location**: `crates/renderer/shaders/triangle.frag`:
- the ReSTIR finalize `if (useRestir) { … }` (~:3414);
- the RESTIR_LIGHT view block `if (viewRestirLight) { … return; }` (~:3831-3842);
- then `#if ENABLE_LEGACY_WRS` / `else {` legacy pass 2 (~:3843-3982), closed by `} // end legacy WRS pass-2 (else of useRestir)`.

**Status**: NEW (introduced by `f97775ca8`)
**Verified against**: HEAD `f97775ca8`

## Description

`f97775ca8` put the RESTIR_LIGHT debug block after the ReSTIR finalize and before the preprocessor-gated legacy pass 2:

```glsl
        if (useRestir) { /* finalize */ }
        if (viewRestirLight) { …; return; }
#if ENABLE_LEGACY_WRS
        else {
        // ── Pass 2: shadow rays for sampled reservoirs ──
        …
        } // end legacy WRS pass-2 (else of useRestir)
#endif
```

In a build with `ENABLE_LEGACY_WRS == 1` (the documented A/B evaluation build), that `else` now binds to `if (viewRestirLight)`, not `if (useRestir)`. So legacy pass 2 runs whenever the view is off, including on ReSTIR pixels. The closing comment "else of useRestir" is false.

**Publisher's validation note (impact correction).** The audit report says this double-counts direct light. The code does not support that today:
- The legacy reservoir arrays are written only on the legacy streaming branch. `resLight[s] = i` (~:3406) sits inside the `else` of pass 1's `if (useRestir)`.
- On ReSTIR pixels every `resLight[s]` therefore stays `0xFFFFFFFF`. The re-bound pass 2 hits `continue` on all 16 slots and subtracts nothing.

The defect is real but latent:
- 16 wasted loop iterations per ReSTIR pixel in the A/B build, a small skew in the timing that build exists to compare;
- a false structural comment;
- a trap: any future change that fills the legacy reservoirs on ReSTIR pixels would silently double-apply shadowing.

## Evidence

- The source layout above.
- `grep -n "resLight\[" triangle.frag` shows writes only in the init loop (~:3136) and the legacy stream (~:3406), and a read in pass 2 (~:3856).
- `ENABLE_LEGACY_WRS` defaults to 0 (`shader_constants.glsl`), so the shipped SPIR-V is unaffected.

## Impact

There is no visible change in the shipped build. The `ENABLE_LEGACY_WRS == 1` evaluation build has wasted pass-2 iterations on ReSTIR pixels, a false comment, and the latent double-shadowing hazard described in the validation note.

## Related

- REN-D12-2026-09-21-01 (#4577): the RESTIR_LIGHT view itself renders magenta until `composite.frag.spv` is rebuilt.
- #1799 (closed): introduced the `ENABLE_LEGACY_WRS` compile-time gate.

## Suggested Fix

Make the `else` bind to `if (useRestir)` again. Either move the `if (viewRestirLight) { … return; }` block after the legacy pass-2 `#endif` (it reads only `useRestir`/`restirY`, which finalize has already settled), or turn the legacy arm into an explicit `if (!useRestir) { … }` instead of a dangling `else`. Optionally, extend the `ENABLE_LEGACY_WRS` source-shape guard in `crates/renderer/src/shader_constants.rs` to assert that the gated legacy arm is keyed on `useRestir`.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D2-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: the FACING_RATIO block and the other debug early-returns checked for the same `#if … else` re-binding
- [ ] **SIBLING**: `triangle.frag.spv` recompiled (plain `-V`, keeps OpName for the reflection test) if default-build code moves
- [ ] **TESTS**: source-shape pin that the legacy pass-2 arm is keyed on `useRestir`


---

# 4583: REN-D2-2026-09-21-02: the FACING_RATIO debug view paints every two-sided back face "inverted-normal" red

State: OPEN  Labels: ['bug', 'renderer', 'low', 'shaders']

**Severity**: LOW (a debug view that gives false positives) · **Dimension**: Debug/Telemetry
**Location**: `crates/renderer/shaders/triangle.frag`: the `geometricNormal` orientation (~:232-239), the `!gl_FrontFacing` flip that applies only to `N` (~:541-543), and the `viewFacingRatio` branch (~:1845-1860)
**Status**: NEW (introduced by `f97775ca8`)
**Verified against**: HEAD `f97775ca8`

## Description

The FACING_RATIO view colours solid red wherever `dot(geometricNormal, toCamera) < 0`, as the "inverted-normal class". But `geometricNormal` is oriented into the hemisphere of `fragNormalEffective`, the interpolated vertex normal, and that normal is not flipped for back faces. Only the shading normal `N` gets the `if (!gl_FrontFacing) N = -N;` flip. So on a two-sided draw (foliage, banners, cloth; the cull-off pipeline), every back-facing fragment has a geometric normal pointing away from the camera and reads red.

These pixels are not leak candidates. Shadow rays go through `offsetRayOriginForDirection`, which orients the offset normal to the ray direction (`dot(n, direction) >= 0 ? n : -n`), so two-sided back faces offset correctly.

## Evidence

- `triangle.frag`: `if (dot(geometricNormal, fragNormalEffective) < 0.0) geometricNormal = -geometricNormal;`, with no `gl_FrontFacing` term.
- `if (!gl_FrontFacing) { N = -N; }` flips only `N`.
- The view: `float facing = dot(geometricNormal, facingViewDir); … facing < 0.0 ? vec3(1.0, 0.04, 0.04) : …`.
- `include/ray_origin.glsl` `offsetRayOriginForDirection` is the direction-aware offset.

## Impact

The view was added for the single-sided-wall light-leak hunt. It floods every two-sided asset with false "inverted normal" red, which hides the real single-sided inversions it was built to find. Debug surface only.

## Related

- REN-D12-2026-09-21-01 (#4577): this view renders full-frame magenta until `composite.frag.spv` is rebuilt, so this defect only shows after that fix.
- REN-D2-2026-09-21-01 (#4582): the sibling RESTIR_LIGHT view from the same commit.

## Suggested Fix

In the view only, flip the tested normal on back faces (`gl_FrontFacing ? geometricNormal : -geometricNormal`), or give two-sided back faces a distinct neutral hue. Red then means a single-sided winding or normal inversion. Leave the production `geometricNormal` as is, because the ray-origin code depends on it.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D2-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: other `geometricNormal`-based debug views checked for the same two-sided back-face mislabel
- [ ] **SIBLING**: `triangle.frag.spv` recompiled (plain `-V`, keeps OpName for the reflection test)


---

# 4584: REN-D3-2026-09-21-01: `presentation.frag` selects AgX with a hand-written `tonemapOp == 1u` that is neither generated from `TONEMAP_OP_*` nor pinned GLSL-side

State: OPEN  Labels: ['bug', 'renderer', 'low', 'shaders', 'test-gap']

**Severity**: LOW (latent wire-contract drift) · **Dimension**: GPU-Struct
**Location**: `crates/renderer/shaders/presentation.frag` `tonemap()` (~:116-118); `crates/renderer/src/tonemap.rs` `TONEMAP_OP_ACES` / `TONEMAP_OP_AGX` (~:28-29) and `operator_ids_are_stable`
**Status**: NEW (introduced by `c5663fe39`)
**Verified against**: HEAD `f97775ca8`

## Description

`tonemap()` dispatches with `params.tonemapOp == 1u ? agx(x) : aces(x)`. The `1u` is typed into the shader by hand. The values `TONEMAP_OP_ACES = 0` and `TONEMAP_OP_AGX = 1` exist only in `tonemap.rs`. They are not emitted into the generated `shader_constants.glsl`, which already carries the other host↔shader enums (debug modes, visibility layers, material kinds).

Two tests touch this and neither covers the value:
- `operator_ids_are_stable` asserts only the Rust values.
- The presentation source-shape test checks that the `uint tonemapOp;` field exists, not which value the shader compares against.

## Evidence

- `grep -rn "TONEMAP_OP" crates/renderer/shaders/ crates/renderer/build.rs crates/renderer/src/shader_constants*.rs` finds only two comments in `presentation.frag`.
- `presentation.frag` (~:117): `return params.tonemapOp == 1u ? agx(x) : aces(x);`.

## Impact

If the Rust side renumbers or adds an operator (a third curve, a reorder), the GPU silently applies a different curve and no test fails. No runtime effect today.

## Related

- REN-D11-2026-09-21-01 (#4578): the AgX curve this literal selects.
- REN-D3-2026-09-21-02 (#4585): the other Stage-1 GPU-contract pin gap.

## Suggested Fix

Emit `TONEMAP_OP_ACES` and `TONEMAP_OP_AGX` into the generated `shader_constants.glsl` (via `shader_constants_data.rs` + `build.rs`, with the usual value pin), and compare against `TONEMAP_OP_AGX` in `presentation.frag`. Failing that, add a source-shape pin that the literal equals `TONEMAP_OP_AGX`.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D3-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: other host enums compared by literal in the Stage-1 shaders (e.g. `params.mode.x < 0.5` in `exposure_meter.comp`) checked
- [ ] **SIBLING**: `presentation.frag.spv` recompiled; `scripts/check-shader-artifacts.sh` green
- [ ] **TESTS**: a generated-header value pin (or a source-shape pin) for the operator ids


---

# 4594: SAFE-D2-2026-09-21-02: `read_pod_vec_from`'s SAFETY argument rests on a false `io::Read` contract

State: OPEN  Labels: ['bug', 'nif-parser', 'medium', 'safety', 'nif']

**Severity**: MEDIUM (a stated safety invariant that is false; the code is sound today only because of how it is instantiated) · **Dimension**: 2 — Memory corruption / UB
**Location**: `crates/nif/src/stream.rs`: `read_pod_vec_from` (~:805-838). This is the single POD `unsafe` site behind `NifStream::read_pod_vec` and `header::read_pod_vec_from_cursor`.
**Status**: NEW. The code has been like this since #3062; earlier audit runs passed it on the stated contract.
**Verified against**: HEAD `f97775ca8`

## Description

- `read_pod_vec_from` builds a `&mut [u8]` over the uninitialised capacity of `Vec::with_capacity(count)` and passes it to `reader.read_exact`.
- Its SAFETY comment justifies this with: "`read_exact` only WRITES through the slice — `Read::read_exact` is a pure writer interface per its contract — so no uninitialised byte is ever read."
- std documents the opposite. `Read` is a safe trait. Implementations "can make no assumptions about the contents of `buf`", and "*callers* of this method in unsafe code must not assume any guarantees about how the implementation uses `buf`. The trait is safe to implement, so it is possible that the code that's supposed to write to the buffer might also read from it. … Calling `read` with an uninitialized `buf` … is not safe, and can lead to undefined behavior." `read_exact`'s docs defer to that text.
- The function is generic over the reader (`reader: &mut impl io::Read`), so nothing limits it to a reader whose behaviour would make the argument true.

## Evidence

```rust
pub(crate) fn read_pod_vec_from<T: AnyBitPattern>(reader: &mut impl io::Read, count: usize, byte_count: usize) -> io::Result<Vec<T>> {
    let mut out: Vec<T> = Vec::with_capacity(count);
    // SAFETY: … `read_exact` only WRITES through the slice — `Read::read_exact` is a pure writer interface per its contract …
    let byte_slice: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(out.as_mut_ptr().cast::<u8>(), byte_count) };
    reader.read_exact(byte_slice)?;
```

- Both callers pass a `Cursor<&[u8]>`, whose `read_exact` is a `memcpy`. That instantiation is the only reason the code is sound today.
  - `NifStream::read_pod_vec` passes `&mut self.cursor`, a `Cursor<&'a [u8]>`.
  - `header::read_pod_vec_from_cursor` passes `cursor: &mut Cursor<&[u8]>`.
- Publish-time addition: the same block also misses `std::slice::from_raw_parts_mut`'s own precondition, "`data` must point to `len` consecutive properly initialized values of type `T`". Uninitialised `Vec` capacity does not meet it, whatever the reader does. Both std quotes were checked against the installed toolchain's `library/std/src/io/mod.rs` and `library/core/src/slice/raw.rs`.

## Impact

- There is no observed UB today: every instantiation reads from an in-memory cursor.
- The generic signature and the false comment invite a future caller to pass a decompressing, buffered or custom reader, and get undefined behaviour without touching the `unsafe` block.
- Every NIF geometry parse goes through this POD bulk-read path.

## Related

- #3062 (closed) removed the zero pre-fill that had made this sound by construction.
- #1439 (closed) added the `AnyBitPattern` bound.
- #4165 (closed) added `#[must_use]` to the cursor twin.
- `docs/audits/AUDIT_NIF_2026-09-21.md` re-confirms this finding without re-filing it.

## Suggested Fix

- Preferred: copy the cursor's remaining bytes (`cursor.get_ref()[pos..pos + byte_count]`, bounds-checked) into `out.spare_capacity_mut()` with `copy_nonoverlapping`, then advance the cursor and `set_len`. This never forms a `&mut [u8]` over uninitialised memory, so it satisfies both contracts, and it keeps #3062's no-pre-fill win.
- Minimum: narrow the parameter to `&mut Cursor<&[u8]>`, the only instantiation, and restate the SAFETY argument against that concrete impl. This alone does not address the `from_raw_parts_mut` precondition.

Source: docs/audits/AUDIT_SAFETY_2026-09-21.md (SAFE-D2-2026-09-21-02)

## Completeness Checks
- [ ] **UNSAFE**: the rewritten block's `// SAFETY:` names only invariants that hold (concrete reader, bounds check, bytes initialised before `set_len`)
- [ ] **SIBLING**: no other `from_raw_parts_mut` over uninitialised `Vec` capacity in `crates/nif` or the other untrusted-input readers
- [ ] **TESTS**: the existing `read_pod_vec` round-trip and oversized-count tests still pass, and the short-input (`UnexpectedEof`) path of the new copy is covered


---

# 4595: SAFE-D4-2026-09-21-01: The CI gate for `#![deny(clippy::undocumented_unsafe_blocks)]` has not run since at least 2026-09-15

State: OPEN  Labels: ['bug', 'medium', 'safety', 'tech-debt', 'ui']

**Severity**: MEDIUM (a defence-in-depth gap around the MEDIUM-floor "unsafe without a SAFETY comment" rule) · **Dimension**: 4 — Unsafe-block discipline
**Location**:
- `.github/workflows/ci.yml`: job `cargo-test` ("Test + Check + Clippy"), step order `cargo test` → `cargo clippy` (~:169-172)
- `crates/renderer/src/lib.rs`: `#![deny(clippy::undocumented_unsafe_blocks)]` (~:21)

**Status**: NEW
**Verified against**: HEAD `f97775ca8`. CI runs were read with `gh api` and `gh run view --log-failed`.

## Description

- The renderer's `#![deny(clippy::undocumented_unsafe_blocks)]` is present, with no `allow` escape. It is a Clippy tool-lint, inert under `cargo build` and `cargo test`. CI enforces it only through `cargo clippy --workspace -- -D warnings`.
- That step runs after `cargo test --workspace` in the same job and has no `if: always()` or `if: success() || failure()`. Any `cargo test` failure skips it.
- `cargo test` has been red on main for weeks, so the clippy step has not run:
  - The report sampled 10 main runs from 2026-09-15 to 2026-09-21; all show `cargo clippy = skipped`. In 9 of them `cargo test` failed. In the tenth (`d54382415`, run 35650571724), `cargo check` failed first, because `exposure_meter.comp.spv` was not yet committed.
  - Publish-time re-check: 45 sampled main runs, from 2026-09-05 through HEAD's run 35658431384, all show `cargo clippy = skipped`.
- At HEAD the first failing test binary is `byroredux_ui`. Eight tests build a real Ruffle player and panic with "Failed to create wgpu device: Ruffle requires hardware acceleration, but no compatible graphics device was found supporting Vulkan":
  - five `navigator::tests::*` tests;
  - two `player::resource_loads_tests::*` tests;
  - `player::render_failure_tests::render_leaves_dirty_set_and_returns_none_on_a_size_mismatch`.
- The same 8 failed on 2026-09-05 (run 33982391005) and 2026-09-15 (run 34967746751).

## Evidence

- HEAD run 35658431384 (`gh api …/actions/runs/35658431384/jobs`): `cargo check = success`, `cargo test = failure`, `cargo clippy = skipped`. The test log shows `test result: FAILED. 74 passed; 8 failed` in `byroredux_ui`.
- The gate is inert in practice, not just in theory. `dc306a6a0` (2026-09-18) landed `with_one_time_commands(device, queue, command_pool, |cmd| unsafe {` in `Texture::overwrite_rgba_pixels` with no SAFETY comment. CI never flagged it; the 2026-09-21 tech-debt audit fixed it inline (#4567).
- `cargo test` runs without `--no-fail-fast`, so it stops at the first failing binary. The UI failures are the first red, not necessarily the only one. On 2026-09-11 (run 34597216311) a different binary failed first: `tools/byro-launcher`'s `engine::tests::supervision::a_crash_carries_its_code_and_the_last_thing_the_engine_said`.

## Impact

- Renderer `unsafe` without a SAFETY comment can merge unseen, and already did once this week.
- Every other `-D warnings` lint in the workspace is unenforced in the same way.
- Nothing is outstanding at HEAD. The audit ran `cargo clippy -p byroredux-renderer --no-deps -- -A clippy::all -D clippy::undocumented_unsafe_blocks` locally, and the renderer is clean.

## Related

- #4567 (closed) treats the gate as "red on the current toolchain" and does not record that CI skips it. #4090 and #4130 (closed) were earlier clippy-red episodes.
- SAFE-D5-2026-09-21-01 (#4596): the `vulkan-validation` lane is inert too.
- CONC-D3-2026-09-21-01 (`docs/audits/AUDIT_CONCURRENCY_2026-09-21.md`): the same red `crates/ui` tests keep the `lock-order-check` lane red. That is a separate consequence, tracked separately.
- NIF-D3-2026-09-21-01 (`docs/audits/AUDIT_NIF_2026-09-21.md`): the nightly real-data lane has never executed, another inert CI gate found today.

## Suggested Fix

- Decouple clippy from the test result: give it its own job, add `if: success() || failure()` to the step, or run it before `cargo test`.
- Make `cargo test` able to go green: gate the adapter-dependent `crates/ui` tests behind a wgpu adapter probe or `#[ignore = "needs a Vulkan adapter"]`. Then run once with `--no-fail-fast` to see whether any later binary is also red.

Source: docs/audits/AUDIT_SAFETY_2026-09-21.md (SAFE-D4-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: every other step sequenced after a fail-fast `cargo test` checked for the same skip-on-failure masking (the `lock-order-check` job is CONC-D3-2026-09-21-01)
- [ ] **TESTS**: a main-branch run after the change shows the `cargo clippy` step executing (success or failure, not `skipped`)


---

# 4596: SAFE-D5-2026-09-21-01: The CI `vulkan-validation` (lavapipe) gate has never reached Vulkan since at least 2026-08-31, and cannot by design since 2026-09-17

State: OPEN  Labels: ['bug', 'medium', 'vulkan', 'safety', 'tech-debt']

**Severity**: MEDIUM (a defence-in-depth gap for the HIGH-floor Vulkan-spec class) · **Dimension**: 5 — Vulkan spec compliance
**Location**:
- `.github/workflows/ci.yml`: job `vulkan-validation` (~:280-359)
- `crates/renderer/src/vulkan/device.rs`: `is_hardware_render_device` (~:311-313) and its use in `pick_physical_device` (~:351-361)

**Status**: NEW
**Verified against**: HEAD `f97775ca8`. CI runs were read with `gh run view --log-failed`.

## Description

- `vulkan-validation` is the only CI lane that boots the engine under `VK_LAYER_KHRONOS_validation`. It is also the only lane that runs a live world under `BYRO_LOCK_ORDER_CHECK=1`.
- Every sampled run dies in "Run 5-frame bench under VK_LAYER_KHRONOS_validation" before any `VkInstance` exists: `thread 'main' panicked at …/xkbcommon-dl-0.4.2/src/x11.rs:59:28: Library libxkbcommon-x11.so could not be loaded.` The apt step installs `mesa-vulkan-drivers`, `vulkan-validationlayers`, `libvulkan-dev`, `libasound2-dev` and `xvfb`, but not `libxkbcommon-x11-0`.
  - The report sampled runs from 2026-08-31 (33451855702), 09-11 (34546452899), 09-21 (35622136101) and HEAD (35658431384).
  - Publish-time re-check: 33451855702 (`06c0ccc1c`, `bench exit status: 101`) and 35658431384 show the same panic.
- So the job is red for a non-Vulkan reason on every run, and a real `[Vulkan]` error would look the same.
- Independently, `33a99ed94` (2026-09-17) made `pick_physical_device` skip `VK_PHYSICAL_DEVICE_TYPE_CPU` devices (`is_hardware_render_device`, which logs "Rejecting Vulkan CPU device …"). Lavapipe is a CPU device. Installing the library would only turn the panic into the "No suitable GPU found" bail that the job tolerates. The next step, `scripts/check-bench-determinism.sh`, would then fail on the non-zero exit.
- No other workflow enables validation:
  - The self-hosted GPU workflows (`rt-correctness.yml`, `playable-smoke.yml`) are `workflow_dispatch`-only and enable no validation layer.
  - The nightly `real-data-gates.yml` enables none either, and has never run (NIF-D3-2026-09-21-01).

## Evidence

```rust
// crates/renderer/src/vulkan/device.rs
fn is_hardware_render_device(device_type: vk::PhysicalDeviceType) -> bool {
    device_type != vk::PhysicalDeviceType::CPU
}
```

- The CI log lines are quoted above.
- `grep -n 'BYRO_VALIDATION\|VK_INSTANCE_LAYERS\|VK_LAYER_KHRONOS' .github/workflows/*.yml` matches only the `vulkan-validation` job in `ci.yml`.

## Impact

- No CI lane can see a Vulkan validation error. That includes SAFE-D1-2026-09-21-01 (#4592) and SAFE-D2-2026-09-21-01 (#4593), and #4510's VUID pair, which only a manual run found on 2026-09-20.
- The lock-order detector's only live-world lane is inert in the same way. That half belongs to the concurrency audit (CONC-D3-2026-09-21-01 covers the test-suite lane).

## Related

- #2138 (closed): the job used to swallow exit codes.
- SAFE-D4-2026-09-21-01 (#4595): the clippy gate is inert for a related reason.
- CONC-D3-2026-09-21-01 (`docs/audits/AUDIT_CONCURRENCY_2026-09-21.md`) and NIF-D3-2026-09-21-01 (`docs/audits/AUDIT_NIF_2026-09-21.md`): other inert CI lanes found today.

## Suggested Fix

- Add `libxkbcommon-x11-0` to the apt step.
- Then do one of two things:
  - add a CI-only switch that admits CPU devices for the 5-frame validation boot, on a Mesa new enough to expose ray query;
  - or move the gate to the self-hosted RT runner with `BYRO_VALIDATION=1`.
- Until one of those lands, the `/audit-safety` Dim-5 first step should say the gate is inert.

Source: docs/audits/AUDIT_SAFETY_2026-09-21.md (SAFE-D5-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: the `Assert renderer-static scene-state determinism` step re-checked against the chosen device policy
- [ ] **TESTS**: a main-branch run shows the bench reaching Vulkan device selection under the validation layer (a clean `[Vulkan]` log, or a validation error that fails the job)


---

# 4597: SAFE-D7-2026-09-21-01: Auto-exposure divides the 64-thread log-luminance sum by one thread's sample count

State: OPEN  Labels: ['bug', 'renderer', 'medium', 'shaders']

**Severity**: MEDIUM (correctness in an opt-in mode; the safety audit routes it to renderer Dim 11) · **Dimension**: 7 — GPU-fed data & shader loop bounds
**Location**: `crates/renderer/shaders/exposure_meter.comp`, the `main()` log-luminance reduction (~:57-94). The committed `exposure_meter.comp.spv` is in source/artifact parity at HEAD.
**Status**: NEW (introduced by `c5663fe39`, 2026-09-21)
**Verified against**: HEAD `f97775ca8`

## Description

- Each of the 64 invocations accumulates its own `log_sum` and its own `int count` over its share of the 64×64 = 4,096-sample grid (every 64th index).
- The shared tree reduction sums `log_sum` across all 64 invocations into `shared_log_sum[0]`. Invocation 0 then divides that total by its *own* `count` (about 64), not by the total sample count (about 4,096).
- The result is `avg_luminance = exp2(64 × mean(log2 L))` instead of `exp2(mean(log2 L))`: the geometric mean raised to the 64th power.

## Evidence

```glsl
float log_sum = 0.0;
int count = 0;                                                     // per invocation
for (int idx = int(gl_LocalInvocationID.x); idx < side * side; idx += 64) { … log_sum += log2(max(l, 1.0e-6)); count += 1; }
shared_log_sum[gl_LocalInvocationID.x] = log_sum;                  // then a tree reduction across 64
…
if (gl_LocalInvocationID.x == 0) {
    float samples = max(float(count), 1.0);                        // invocation 0's count only
    float avg_luminance = exp2(shared_log_sum[0] / samples);
```

- At HEAD, CI's shader-parity job reports drift only for `composite.frag.spv`, so the shipped `exposure_meter.comp.spv` matches this source.

## Impact

- Metering is effectively bang-bang: exposure ends up at one limit or the other.
  - Host limits are `MIN_AUTO_EXPOSURE` (1/256) and `MAX_AUTO_EXPOSURE` (16).
  - With zero compensation, a scene geometric-mean luminance below about 0.93 (essentially any linear-HDR interior) pins exposure at `MAX_AUTO_EXPOSURE`. Above about 1.06, it pins at `MIN_AUTO_EXPOSURE`.
  - Only the narrow band between meters to an intermediate value. The correct target is `0.15 / L`.
  - The output stays finite only because of the final clamp to `params.limits`.
- Tests cannot see the bug. The host `auto_exposure(average_luminance, …)` in `crates/renderer/src/vulkan/exposure.rs`, and its tests, take the average as an input and never model the shader's reduction.
- Scope: this affects only `--auto-exposure` and the console command `exposure auto`. Fixed mode is the default (`byroredux/src/cli_args.rs`).

## Related

- REN-D11-2026-09-21-02 (#4590): the same shader adapts each per-frame-in-flight slot from its own two-frame-old value, so the effective time constant is 2τ. That is a separate defect, and the adaptation-history fix belongs there.
- REN-D11-2026-09-21-01 (#4578): AgX clamps linear input before `log2` in the presentation pass that this meter feeds. A separate defect.
- REN-D11-2026-09-21-03 (#4591): the same pass lacks the raw-debug gate.
- REN-D3-2026-09-21-02 (#4585): `MeterParams` ↔ `Params` is missing from the UBO block-size table.
- `docs/audits/AUDIT_PERFORMANCE_2026-09-21.md` cites this finding without re-reporting it.

## Suggested Fix

- Reduce a `shared_count[64]` alongside `shared_log_sum` and divide by the reduced total. Alternatively, divide by the known in-bounds sample total.
- Add a guard that can see the reduction, because the host model cannot. Either a source pin that forbids a per-invocation divisor after the shared reduction, or a GPU readback test on a constant-luminance image that expects `0.15 / L` (clamped), not a limit.

Source: docs/audits/AUDIT_SAFETY_2026-09-21.md (SAFE-D7-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: the other single-workgroup reductions in `crates/renderer/shaders/` checked for a per-invocation divisor after a shared reduction
- [ ] **SIBLING**: `exposure_meter.comp.spv` recompiled; `scripts/check-shader-artifacts.sh` green
- [ ] **TESTS**: a constant-luminance scene meters to `0.15 / L` (clamped), not to a limit


---

# 4598: SAFE-D1-2026-09-21-02: Launcher preflight leaks the `VkInstance` when `enumerate_physical_devices` fails

State: OPEN  Labels: ['bug', 'low', 'safety', 'tech-debt']

**Severity**: LOW · **Dimension**: 1 — FFI lifetime
**Location**: `tools/byro-launcher/src/preflight.rs`: `probe()` (~:186-202)
**Status**: NEW (unchanged since `e05b4a9f8`, 2026-08-30; never reported)
**Verified against**: HEAD `f97775ca8`

## Description

In `probe()`, `instance.enumerate_physical_devices().map_err(|_| Blocker::NoAdapter)?` returns before `instance.destroy_instance(None)`, the function's only destroy call. The `VkInstance` created a few lines earlier leaks on that error path. This fails the safety audit's own check that an instance is destroyed on every path.

## Evidence

```rust
let instance = entry.create_instance(&create_info, None).map_err(|_| Blocker::NoVulkan)?;
let devices = instance
    .enumerate_physical_devices()
    .map_err(|_| Blocker::NoAdapter)?;   // returns without destroying `instance`
…
instance.destroy_instance(None);
best.ok_or(Blocker::NoAdapter)
```

## Impact

A one-shot `VkInstance` leak on the launcher's preflight error path, reached when a driver creates an instance but then fails enumeration. The process continues or exits. There is no per-frame cost.

## Related

None.

## Suggested Fix

Destroy the instance before propagating the error. Either match on the enumeration result, or wrap the instance in a small drop guard that calls `destroy_instance`.

Source: docs/audits/AUDIT_SAFETY_2026-09-21.md (SAFE-D1-2026-09-21-02)

## Completeness Checks
- [ ] **UNSAFE**: `probe()`'s SAFETY comment covers the destroy on the error path
- [ ] **SIBLING**: other `create_instance` users outside the renderer checked. Publish-time note: `crates/fsr3-sys/examples/vulkan_context_smoke.rs` has the same shape. `create_debug_utils_messenger(…)?` returns between `create_instance` and `destroy_instance` on its opt-in validation path (dev-only example).


---
