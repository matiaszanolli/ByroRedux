# #4970: REN-D9-2026-09-27-02: `drop_skinned_blas`'s doc comment is attached to `commit_provisional_skinned_blas`

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4970
- **Labels**: low,renderer,documentation,doc-rot

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D9-2026-09-27-02**._

- **Severity**: LOW
- **Dimension**: Skinning (doc)
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`, the `/// Drop a per-skinned-entity BLAS. Routes through pending_destroy_blas …` block immediately above `/// #3991 / #917 — clear this frame's provisional-insert list …` on `pub fn commit_provisional_skinned_blas`. `pub fn drop_skinned_blas` has no doc.
- **Status**: NEW. Introduced by `0025d8221` (#3991, 2026-09-07), which inserted the commit and rollback pair between the doc and its function.
- **Description / Evidence**: Rustdoc merges both paragraphs into `commit_provisional_skinned_blas`'s docs. That tells readers a list-clear "routes through `pending_destroy_blas` with a `DEFAULT_COUNTDOWN` countdown", and leaves the one function that actually does the deferred destroy undocumented. `drop_skinned_blas` is the lifetime-critical path for the "skinned BLAS never destroyed while referenced" contract.
- **Impact**: Doc rot on a lifetime contract. No runtime effect.
- **Suggested Fix**: Move the "Drop a per-skinned-entity BLAS…" paragraph onto `drop_skinned_blas`.

Existing, still present:
- **#4876** (open, REN-D9-2026-09-24-06). `shader-pipeline.md` step 1a still says morph-weight visibility "comes from step 5b's bulk barrier". Not re-filed.

Resolved since baseline:
- **REN-D9-02 of 09-20** (PickedUp two-site lockstep, no guard) is fixed by `7ebf84817`. Both halves are now pinned: `picked_up_placements_stay_hidden_even_when_animation_says_visible` in `static_mesh_fx_skip_tests.rs` and `picked_up_body_gets_its_first_skin_upload_only_when_dropped` in `bone_palette_overflow_tests.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
