# #6948: TD1-2026-11-15-02: `TextureRegistry::flush_upload_batch` grew 157 to 369 code lines (+212) in the M39 residency work

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6948
- **Labels**: low,renderer,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Age**: all M39 commits, 494d077b2 (2026-11-03) through f754eee28 (2026-11-14). Other contributors: 6a4c661a8 (#6738), b8d89bdcb (#6737), b34b897df (#6624), dd2cd84e9 and 9179059a4 (11-04).
- **Effort**: small (verbatim extraction, no barrier or order change)
- **Labels**: `tech-debt`
- **Location**: `crates/renderer/src/texture_registry/upload.rs:1253-1710` (369 code lines, 457 span, nesting 5)
- **Status**: NEW. It is the largest organic growth of any function since 670f81917 and has no size pin.
- **Description**: One function does four phases in sequence:
  1. Declare `StagedUpload` and run the one-time-command closure that records every copy (~265 lines of the span; parse, staging, residency-change staging, floor-first slices, cube faces).
  2. Handle a failed submission. This is the `may_be_in_flight` leak-or-destroy branch of #4891, and it is the part you least want to re-read in a 457-line body.
  3. Install staged textures (the `for s in staged` loop, ~75 lines, #6738 refusal handling).
  4. Record the floor sources and re-parse their DDS headers (`floor_sources` loop).
- **Evidence**: No size pin covers it (the renderer pin list in `crates/renderer/src/vulkan/context/function_size_pin_tests.rs` is context-only). My scan gives 157 code lines at 670f81917 against 369 at HEAD, same file and name.
- **Impact**: The newest and still-moving residency code sits in the function with the most failure-path subtlety (leak versus destroy decided by `OneTimeCommandError::may_be_in_flight`). The next M39 change adds to it.
- **Suggested Fix**: Extract the three post-submit phases and the recording closure body as `&mut self` methods in the same file: `record_batch_uploads(..) -> (staged, failed_changes)`, `discard_failed_batch(staged, e)`, `install_staged(staged)` and `record_floor_sources(..)`. Move the code verbatim in the same order. Add a `flush_upload_batch` line to `crates/renderer/src/vulkan/context/function_size_pin_tests.rs` (or a texture-registry sibling) so it cannot regrow. Read *feedback_speculative_vulkan_fixes* first: this is an extraction only, no behaviour change.
- **Related**: #6143 (closed; same extraction shape: #6708, #6709, #6710).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
