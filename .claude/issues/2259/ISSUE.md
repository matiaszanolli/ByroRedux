# TD1-081: build_tlas is an ~835-LOC single function — long-standing debt, resurfaces the 05-13 TD9-012 finding at a higher LOC count

Severity: low
Source audit: docs/audits/AUDIT_TECH_DEBT_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2259

**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:46-880` (`build_tlas`, ~835 LOC of the file's 887 total)
**Status**: NEW (the underlying complaint was first raised 2026-05-13 as TD9-012 at 684 LOC, in the pre-split `acceleration.rs` monolith; no GitHub issue was ever filed and it lapsed once the containing file was split below the 2000-LOC discovery threshold — this is the first time it's tracked as its own issue)

**Description**: `tlas.rs` (the module `acceleration.rs` was split into under Session 34/35) is essentially one function: `build_tlas` runs from line 46 to ~880, with only a trivial 3-line `tlas_handle` accessor after it. Growth has been slow and steady, not a Session-62 spike: 834 LOC (2026-06-02) → 887 today, +151 LOC/+22% over roughly three months. It builds/rebuilds the whole per-frame TLAS: instance-buffer sizing and rebuild, scratch-buffer growth/shrink decisions, per-draw-command instance transform + shadow-mask + custom-index assembly, and the BUILD-vs-UPDATE dispatch, all inline in one `unsafe fn`.

**Evidence**: `grep -n "pub unsafe fn build_tlas\|pub fn tlas_handle"` → 46, 884. Size-history confirmed via `git show <rev>:...tlas.rs | wc -l` across commits spanning 06-02→08-02.

**Impact**: Maintainability only — the function has an extensive block of correctness-critical comments (SSBO-index/shadow-mask assembly, prior fix-preserving comments cited inline) that a reader currently has to hold in their head across 835 lines with no sub-function boundaries to rest at.

**Related**: TD9-012 (2026-05-13 tech-debt audit, pre-split numbering, 684 LOC) — same underlying function, never re-tracked after the containing file was split; the file-level fix (splitting `acceleration.rs` into `tlas.rs`/`blas_static.rs`/`blas_skinned.rs`/etc.) coincidentally removed it from the discovery command's radar without addressing the complaint.

**Suggested Fix**: Mirror the extraction style already used in `blas_static.rs`/`blas_skinned.rs` (both pull named helper functions like `scratch_should_shrink`, `decide_use_update` out to `predicates.rs`, already imported here) — extract the instance-buffer rebuild/resize block and the per-draw-command instance-assembly loop (the largest contiguous chunks) into private helpers in `tlas.rs`, keeping `build_tlas` as the top-level sequencing function.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable
