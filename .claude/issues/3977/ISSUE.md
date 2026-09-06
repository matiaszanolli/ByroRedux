# #3977 — REN-2026-09-06-D11-01: the water pipeline coverage-blends two G-buffer attachments `water.frag` never writes

**Labels**: high, pipeline, renderer, shaders, water, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D11-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: HIGH
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/water.rs` (`create_water_pipeline`'s `attachments` array + the `masked_off` / `auxiliary_blend` locals; module doc), `crates/renderer/shaders/water.frag` (its fragment-output declarations), `crates/renderer/src/vulkan/pipeline.rs` (`auxiliary_blend_attachment`)
- **Status**: NEW
- **Description**: `d9e61ead` (bundling #3821 / REN-WD-D8-01) changed the water
  pipeline's blend table from `[hdr_blend, masked_off × 5, fsr_mask_max × 2]`
  to `[hdr_blend, masked_off, masked_off, masked_off, auxiliary_blend,
  auxiliary_blend, fsr_mask_max, fsr_mask_max]`, giving colour attachments 4
  (`raw_indirect`) and 5 (`albedo`) a `color_write_mask = RGBA` with
  `blend_enable = true` (`SRC_ALPHA` / `ONE_MINUS_SRC_ALPHA`). It did **not**
  add the matching fragment outputs. `water.frag` declares exactly three
  outputs — `layout(location = 0) out vec4 outColor`,
  `layout(location = 6) out float outFsrReactive`,
  `layout(location = 7) out float outFsrTransparency` — and nothing at 4 or 5.
  Per the Vulkan fragment-output-interface rules, an attachment enabled for
  writing that the fragment shader's interface does not include receives
  **undefined** values; with blending on, the undefined source colour *and*
  source alpha both feed the blend equation, so the destination (the opaque
  receiver's demodulated GI and albedo) is destroyed by an undefined amount
  rather than attenuated by water's coverage.
  The three attachments that stay `masked_off` (1 normal, 2 motion, 3 mesh_id)
  are fine — a zero write mask discards the undefined value.
  The previous shape was correct precisely *because* every non-declared
  attachment was masked off; the shared-state extraction inherited the
  ordinary blend pipeline's states without inheriting `triangle.frag`'s
  outputs, and `triangle.frag` *does* declare 4 and 5.
- **Evidence**:
  - `water.rs`: `let attachments = [hdr_blend, masked_off, masked_off, masked_off, auxiliary_blend, auxiliary_blend, fsr_mask_max, fsr_mask_max];`
  - `pipeline.rs::auxiliary_blend_attachment` → `.color_write_mask(vk::ColorComponentFlags::RGBA).blend_enable(true).src_color_blend_factor(SRC_ALPHA).dst_color_blend_factor(ONE_MINUS_SRC_ALPHA)`, pinned by `auxiliary_blend_attachment_is_a_src_alpha_coverage_blend`.
  - `grep -n ") out " crates/renderer/shaders/water.frag` → only locations 0, 6, 7.
  - `water.frag`'s own comment above those declarations still reads "the
    intermediate G-buffer attachments stay masked off as before" — written
    for the pre-#3821 table and now false for 4/5.
  - `attachment_doc_pin_tests::module_doc_matches_the_blend_table` pins the
    table string and the module doc against each other, but has no
    shader-side limb, so it passed through this change.
- **Impact**: Every pixel a water plane covers writes undefined values into
  the `raw_indirect` and `albedo` G-buffer attachments. `svgf_temporal.comp`
  consumes `raw_indirect` and `composite.frag` re-multiplies
  `indirect * albedo`, so the blast radius is every exterior/interior cell
  with water — lake and river beds, waterfalls, sewers, the FNV/FO4 water
  interiors. Symptoms would read as unstable colour/brightness in the water
  column, potentially with per-driver and per-frame variation (an unwritten
  output typically retains whatever the shader last left in that register —
  commonly the HDR colour — so it can look plausible-but-wrong rather than
  obviously broken). Not a crash; a correctness violation with a whole
  content-class blast radius. Note this is the *opposite* direction from what
  #3821 set out to fix: the receiver's GI is not "attenuated by water's
  coverage", it is replaced by an undefined value.
- **Related**: #3821 / REN-WD-D8-01 (`docs/audits/AUDIT_RENDERER_2026-09-04.md`,
  the finding this regressed out of), `d9e61ead` (the commit), #2745 (why
  refractive glass preserves attachment 3 but not 4/5), #3604 (the doc-pin
  test that has no shader limb).
- **Suggested Fix**: Two viable directions, both `cargo test`-pinnable.
  (a) Declare `outRawIndirect` (location 4) and `outAlbedo` (location 5) in
  `water.frag` and write the values #3821 actually wanted — the water
  surface's own demodulated indirect + albedo with `finalAlpha` in the alpha
  lane, mirroring `triangle.frag`'s `auxiliaryAlpha = isAlphaBlend ?
  finalAlpha : 1.0` convention (#883f57cd). This is the fix that delivers
  #3821's intent. (b) If (a) is more shader work than wanted right now,
  revert 4/5 to `masked_off` and reopen #3821 — that restores the previously
  correct (if un-attenuated) behaviour instead of an undefined one.
  Either way, add a limb to `attachment_doc_pin_tests` that scans
  `include_str!("../../shaders/water.frag")` and asserts *every* attachment
  index whose blend state is not `masked_off` has a matching
  `layout(location = N) out` — the class of bug the existing pin cannot see.
  `water.rs` and `presentation.rs` are also the only two graphics pipelines
  in the crate that do not run `reflect::validate_set_layout`; a
  fragment-output arm on that helper would generalise the guard.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
