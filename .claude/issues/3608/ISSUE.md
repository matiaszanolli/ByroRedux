# #3608 — REN-2026-08-30-D16-02: `renderer.md`'s bloom section still credits composite with the bloom add, and omits `bloom_apply.comp`

**Labels**: `low,renderer,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3608 --json state`.

---

- **Severity**: Low
- **Dimension**: Bloom
- **Location**: `docs/engine/renderer.md` (§"Bloom (M58)", ~line 642; pipeline bullet ~line 64)
- **Status**: OPEN — new
- **Description**: The section names only `bloom_downsample.comp` and
  `bloom_upsample.comp` and states *"The final `up_mips[0]` is what composite
  adds to scene HDR before tone-mapping"*. Since #2796 composite does not add
  bloom at all — a third shader, `bloom_apply.comp`, reads composite's output
  back as a storage image and does the add in place, and the bloom chain now
  runs **after** composite rather than before it. `bloom_apply.comp` is not
  mentioned anywhere in `renderer.md`, including its file-tree listing
  (`~line 195`, "Bloom pyramid (M58) — separable down/up compute passes").
- **Evidence**:
  - `crates/renderer/shaders/bloom_apply.comp:52` — `imageStore(sceneImage, coord, vec4(scene.rgb + bloom * BLOOM_INTENSITY, scene.a));`
  - `crates/renderer/src/vulkan/bloom.rs:760` — `pub unsafe fn apply_to_scene(...)`
  - `crates/renderer/shaders/composite.frag:820`–`829` — "bloom now dispatches AFTER this pass … `bloomTex` (binding 7) is therefore unused by this shader now"
  - `crates/renderer/src/vulkan/context/post_passes.rs:271` — `self.record_bloom_pass(cmd, frame);` ordered after `record_composite_pass`
- **Impact**: The doc points a reader at the wrong pass for the add site and
  at the wrong pass ordering. Given #3247 is open on exactly the barriers
  around that relocation, an out-of-date map of where bloom reads and writes
  actively works against whoever picks up #3247.
- **Suggested Fix**: Add `bloom_apply.comp` to the section, the pipeline
  bullet, and the file-tree listing; restate the add site as
  "`bloom_apply.comp`, in place on `composite.scene_images[frame]`, upstream
  of the FSR/native upscale and of `presentation.frag`'s ACES" — the
  tone-map claim itself is still correct and should stay.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D16-02

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
