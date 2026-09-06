# #3844: TD2-2026-09-05-01: the GENERAL-layout accumulator clear sandwich exists in four copies, and #3646/#3647 plus its pin test enumerate only three

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD2-2026-09-05-01) via `/audit-publish`, 2026-09-05. Labels: `medium,renderer,sync,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3844 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD2-2026-09-05-01), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: MEDIUM (promotion trigger: duplicated logic with divergent bug-fix history — one copy got a fix the others did)
- **Dimension**: 2 — Logic Duplication
- **Location**:
  - `crates/renderer/src/vulkan/caustic.rs` — `CausticPipeline::dispatch` (moving-camera `else` branch, `pre_clear_barrier`/`post_clear_barrier`)
  - `crates/renderer/src/vulkan/caustic.rs` — `CausticPipeline::clear_for_skip`
  - `crates/renderer/src/vulkan/volumetrics.rs` — `VolumetricsPipeline::record_neutral_frame` (`to_clear`/`to_sample`)
  - `crates/renderer/src/vulkan/water_caustic.rs` — `WaterCausticAccum::clear_pre_render_pass` (`pre_clear`/`post_clear`) ← the copy that was not enumerated
- **Status**: NEW
- **Description**: Four sites implement byte-for-byte the same contract —
  *barrier GENERAL→GENERAL into `TRANSFER_WRITE`; `vkCmdClearColorImage` with
  `uint32: [0,0,0,0]`; barrier back out of `TRANSFER_WRITE` to
  `SHADER_READ|SHADER_WRITE`* — on a per-FIF `R32_UINT` accumulator. The rule
  that makes the sandwich correct (the source scope must name `TRANSFER` so a
  *prior visit's* clear on the same slot chains into this one) was worked out
  once under #3646/#3647 and applied to three of the four. There is no shared
  helper, so the fourth copy simply was not in the author's field of view — and
  the source-shape guard test written to stop exactly this drift enumerates the
  same three files by name.
- **Evidence**:
  - `1889585a` (2026-08-30, "Fix #3646: carry skip-path clears into the next slot visit's barrier scope") touches **`caustic.rs` and `volumetrics.rs` only** (`git show --stat 1889585a`). `water_caustic.rs` was last touched by `c2336ee1` (2026-07-21), five weeks earlier.
  - After that commit, the three fixed sites read
    `.src_access_mask(SHADER_READ | SHADER_WRITE | TRANSFER_WRITE)` with
    `PipelineStageFlags::COMPUTE_SHADER | FRAGMENT_SHADER | TRANSFER` in the
    source stage. `WaterCausticAccum::clear_pre_render_pass` still reads
    `.src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)`
    with `vk::PipelineStageFlags::FRAGMENT_SHADER` — no `TRANSFER` on either axis.
  - The pin test `mod skip_clear_mask_pin_tests` (`crates/renderer/src/vulkan/caustic.rs`)
    has exactly two arms — `caustic_skip_clear_and_next_visit_agree_on_transfer`
    (`include_str!("caustic.rs")`) and
    `volumetrics_neutral_clear_and_next_visit_agree_on_transfer`
    (`include_str!("volumetrics.rs")`). There is no `water_caustic.rs` arm, so
    the fourth copy is unguarded as well as unfixed.
  - `clear_pre_render_pass` runs unconditionally every frame the accumulator
    exists (`crates/renderer/src/vulkan/context/build_and_upload_instances.rs`,
    inside `if let Some(ref wca) = self.water_caustic_accum`), so the
    prior-visit-was-a-clear case is the *normal* shape here, not an edge case.
- **Impact**: Maintenance, primarily: a fifth accumulator (or a fifth revision
  of the rule) has four places to land instead of one, and the guard test does
  not scale with the copies. **Correctness rider, explicitly unverified:**
  whether the missing `TRANSFER` in `clear_pre_render_pass`'s source scope is a
  live WAW hole depends on whether a frame can skip *both* `water.frag`'s
  atomics and `composite.frag`'s `texelFetch(waterCausticTex, …)` — that
  `texelFetch` sits behind a `params.caustic_flags.x > 0.5` select. I did not
  settle that; it is a synchronisation question and belongs to
  `/audit-renderer`, not to this dimension. The duplication and the
  three-of-four divergence are proven independently of how it resolves.
- **Related**: #3646, #3647 (both CLOSED by `1889585a`); #653 (the "mask must be
  right even when the fence serialises" rule the commit cites); #870.
- **Suggested Fix**: Add one helper to
  `crates/renderer/src/vulkan/descriptors.rs` — it already owns the
  `image_barrier_*` family — e.g.
  `clear_general_accumulator(device, cmd, image, range, extra_src_stages, dst_stages)`
  that emits the whole sandwich with `TRANSFER` structurally present on both
  sides, and route all four sites through it. Then collapse
  `skip_clear_mask_pin_tests` into a single assertion over the helper rather
  than three `include_str!` scans that must each be remembered.
- **Effort**: small (≤2 h)

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
