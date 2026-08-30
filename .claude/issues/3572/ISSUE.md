# #3572 — REN-2026-08-30-D13-01: TAA resolves only the pre-composite direct HDR — sky, denoised indirect, volumetrics, caustics and bloom bypass the resolve entirely (FSR, the default, does not)

**Labels**: `medium,renderer,pipeline,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3572 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs` (`record_post_passes`, lines 264–271: `record_taa_pass` → `record_composite_pass` → `record_bloom_pass` → `record_upscale_pass`); `crates/renderer/shaders/composite.frag` (`has_surface` / `is_sky`, lines 536–537); `crates/renderer/src/vulkan/taa.rs` (`TaaPipeline::write_descriptor_sets`, `curr_hdr` ← `hdr_views[f]`)
- **Status**: OPEN — architectural, root cause of the symptom `#2760` patched around from inside the constraint
- **Description**: The TAA resolve is wired to `composite.hdr_image_views[f]` — the **raw** main-render-pass HDR attachment, i.e. direct lighting only — and writes `history[f]`, which `composite` then samples as binding 0 (`hdrTex`, `composite.frag:497`). Everything composite *adds* after that point is never seen by TAA: the analytically-synthesised sky (`compute_sky`), the SVGF-denoised indirect, volumetrics, water caustics, and (since `#2796`) bloom. The FSR path is the mirror image: `record_upscale_pass` takes `scene_color = composite.scene_image(frame)` (`post_passes.rs:967–971`) — the fully composited, post-bloom scene — so FSR 3.1 temporally resolves *all* of it. Since `UpscalerMode::default()` is `Fsr3(Quality)` (`upscaling.rs`), the lower-coverage path is the one selected by `--upscaler taa`.
  The concrete visible consequence is the geometry/sky silhouette. `composite.frag:536` classifies each pixel with a hard binary `depth < 1.0` against the **jittered** depth buffer, downstream of the resolve. Sub-pixel Halton jitter flips which side of a silhouette a pixel centre lands on every frame, so that pixel alternates between "TAA-resolved geometry colour" and "freshly-computed, never-temporally-filtered sky", and no filter in the chain ever averages the two. `#2760` correctly diagnosed the mechanism in `taa.comp`'s own comment ("sky is synthesised later in composite.frag and never exists in this HDR attachment") and softened the *history-acceptance* half of it (`disocclusionFromSky` → `gamma = 0.0` box filter) — but it can only improve the frames on which the pixel is geometry. The frames on which jitter makes the pixel sky bypass `hdrTex` for the sky branch entirely.
- **Evidence**:
  - `post_passes.rs:264` `self.record_taa_pass(cmd, frame);` then `:266` `self.record_composite_pass(cmd, frame);` — TAA strictly precedes composite.
  - `taa.rs::write_descriptor_sets`: `curr_hdr = ... .image_view(hdr_views[f])`, and `resize.rs:968–971` / `init.rs:1287–1338` show `hdr_views` is `composite.hdr_image_views` (the render-pass colour attachment), while composite's binding 0 is rebound to `taa.output_view(i)` at `init.rs:1338` / `resize.rs:1001` / `resize.rs:1372`.
  - `post_passes.rs:967–971`: `let scene_color = self.composite... .scene_image(frame);` handed to `FrameUpscaler::record` — post-composite, and `record_bloom_pass` (`:895–903`) has already written into that same `scene_images[frame]`.
  - `composite.frag:535–537`: `float depth = texelFetch(depthTex, ivec2(gl_FragCoord.xy), 0).r; bool has_surface = depth < 1.0; bool is_sky = !has_surface && ...` — no temporal term, no coverage term.
  - `taa.comp:150–165` (the `#2760` comment block) states the constraint explicitly.
- **Impact**: On `--upscaler taa`, exterior geometry/sky silhouettes crawl every frame with a parked camera — the exact artefact TAA exists to remove — and indirect/volumetric/caustic edges receive no temporal antialiasing at all. Because the default upscaler hides it, the TAA fallback path (FSR context creation failure at startup promotes to `UpscalerMode::Taa`, per `upscaling.rs`'s `Taa` doc / `#2480`) silently delivers materially worse image stability than the mode it replaced, in a way no `cargo test` observes.
- **Suggested Fix**: Move the TAA resolve to the same tap FSR uses — dispatch it on `composite.scene_images[frame]` after `record_composite_pass` (and after or before bloom, matching FSR's tap) instead of on the raw HDR attachment, so one code path feeds both temporal reconstructors the same image. That makes `#2760`'s `disocclusionFromSky` special case unnecessary rather than merely mitigated, since the sky would then be present in the resolved image on both jitter phases. **Needs RenderDoc verification** for the resulting layout/barrier sequence (`scene_images` is `COLOR_ATTACHMENT | SAMPLED | TRANSFER_SRC | STORAGE` and already changes layout twice in the tail of the frame) — do not ship the barrier reshuffle on test evidence alone. If the move is judged too large, the narrower fix is to feed composite a temporally-stable sky/geometry coverage instead of the binary `depth < 1.0` (e.g. resolve coverage in TAA and pass it through), but that is a second temporal history to validate.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D13-01

> **Policy note (publish-time):** per the project's standing rule, no speculative Vulkan render-pass / pipeline / barrier restructure is proposed here. The observation is filed with its evidence; any scope change needs a `BYRO_VALIDATION=1` sync-validation run or a RenderDoc capture first.


## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
