# #5469: TD2-2026-10-08-01: #5100 closed with 25 inline source-scan cuts and 2 full brace-stripper copies still outside `core::source_scan`; three keep the whole-file fallback #5100 removed elsewhere

**Labels**: low,renderer,tech-debt,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5469

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-08.md` — `TD2-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Incomplete-fix remainder of CLOSED #5100 (not a duplicate). Re-verified at HEAD: 27 `find("#[cfg(test)]")` / `split_once` hits outside `crates/core/src/source_scan.rs`; `strip_test_modules` copy at `crates/plugin/src/esm/records/tests.rs:2678`, `production_lines` at `crates/renderer/src/vulkan/context/draw.rs:1388`.

- **Severity**: LOW. The duplicates share no live divergent bug today: every scanned file's first `#[cfg(test)]` is a
  trailing `mod`.
- **Dimension**: 2 — Logic Duplication
- **Location**:
  - **renderer (21)**:
    - `vulkan/allocator.rs:746,787`
    - `vulkan/context/mod.rs:1890,2093`
    - `vulkan/context/skinned_blas_refit.rs:1121,1429`
    - `vulkan/buffer.rs:2007,2173`
    - `vulkan/image.rs:491`
    - `vulkan/context/dispatch_skin_and_cluster.rs:828,890`
    - `vulkan/skin_compute.rs:1867`
    - `vulkan/device.rs:1332,1435`
    - `vulkan/context/assemble_camera_and_lights.rs:725`
    - `vulkan/context/post_passes.rs:1812,1862,2024,2106,2164`
    - `vulkan/scene_buffer/shader_contract_tests.rs:2922`
  - **bin**:
    - `cell_loader/object_lod.rs:1379`
    - `render/groundcover_hasher_tests.rs:19-25`
    - `cell_loader/unload.rs:1115-1121`
    - `workspace_hygiene_tests.rs:439`
  - **pex**: `decompile/boolean.rs:845`
  - **full stripper copies**:
    - `crates/plugin/src/esm/records/tests.rs:2678` (`strip_test_modules`)
    - `crates/renderer/src/vulkan/context/draw.rs:1388` (`production_lines`)
- **Status**: NEW. It is the residue of CLOSED #5100 (`655b317c9`, 10-06, "one source-scan cut in core; the copies
  delegate or die"). That commit edited `post_passes.rs` yet left five inline cuts in it.
- **Age**: the surviving cuts date from 2026-08-30 (pex) to 2026-10-01 (`draw.rs`, `c57e5cc4a`).
- **Effort**: small
- **Description**:
  - `crates/core/src/source_scan.rs` provides `production_text` (cut at the first `#[cfg(test)]\nmod `) and
    `strip_test_modules`. The latter is brace-matched and skips braces in strings, raw strings, chars and comments.
  - The renderer's `source_scan` re-exports both.
  - Most of the inline cuts use the weaker `find("#[cfg(test)]")`. That needle also truncates at a `#[cfg(test)] fn` or
    `use`, which is exactly the hazard the core doc warns about.
  - `object_lod.rs:1379` (`map_or(src, …)`), `skin_compute.rs:1867` and `device.rs:1435` (`unwrap_or(len)`) keep the
    whole-file fallback. #5100 removed that fallback from `anim_convert` as the #4842 vacuous-guard class.
  - The two stripper copies count `{`/`}` naively, including braces inside string literals. Core's merged matcher was
    written to stop exactly that.
- **Evidence**: `grep -rn 'find("#\[cfg(test)\]")\|split_once("\\n#\[cfg(test)\]' --include='*.rs' crates byroredux tools`
  returns 27 lines outside core. Two of them are `boot/mod.rs:655` (a module-name lister, not a cut) and the hygiene scan
  of files without tests, which is legitimate.
- **Impact**: the skill and `_audit-common.md` tell auditors that the helper has "ONE home". A reader trusting #5100's
  closure will miss these. The next guard that copies a neighbour inherits the weak needle.
- **Related**: #5100 (CLOSED), #4842, #5164.
- **Suggested Fix**:
  - Replace each cut with `crate::source_scan::production_text(src)` (renderer, via its re-export) or
    `byroredux_core::source_scan::production_text`.
  - Replace the two strippers with `strip_test_modules`. `draw.rs` can then count `.lines()` on the result.
  - Add a `workspace_hygiene_tests` case that fails on `find("#[cfg(test)]")` outside `crates/core/src/source_scan.rs`.

## Completeness Checks
- [ ] **SIBLING**: Every inline cut in the renderer, bin and pex crates routed through `source_scan` (none left with the whole-file `map_or(src, …)` / `unwrap_or(len)` fallback)
- [ ] **TESTS**: A regression test pins this specific fix (a `workspace_hygiene_tests` case banning `find("#[cfg(test)]")` outside `source_scan.rs`)
