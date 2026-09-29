# TD2-2026-09-29-02: The source-scan `production_text` cut is copied into at least 9 places using 3 different needles

**Labels**: low,tech-debt,test-gap,bug

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 2 · **Status**: NEW · **Effort**: small
- **Location**:
  - Helpers:
    - `crates/renderer/src/source_scan.rs:21`
    - `crates/physics/src/source_scan.rs:21`, a documented copy: "cfg(test) items do not cross crate
      boundaries"
    - `byroredux/src/extensions/mod.rs:522`, its own brace-stripping `-> String` version
  - Inline copies:
    - `byroredux/src/app_events.rs:1746/1860/1901` (`"\n#[cfg(test)]\nmod "`)
    - `byroredux/src/anim_convert.rs:485` (`"\n#[cfg(test)]\n"` + `.unwrap_or(src)`)
    - `crates/core/src/ecs/resources/skin_slot_pool.rs:1112`, `crates/renderer/src/vulkan/water.rs:1902`
      and `frame_upscaler.rs:1320` (`"\n#[cfg(test)]"`, which also cuts at a `#[cfg(test)] use`)
- **Evidence**:
  - `_audit-common.md` makes `production_text` the house rule for source scans (#4604/#4842). Only
    renderer and physics can call it.
  - The byroredux bin has 201 `include_str!` scans and 5 uses. core, nif, plugin, scripting and ui have
    12–31 scans each and no helper.
  - `anim_convert.rs:485` falls back to the whole file when the cut misses. Its positive needle
    `resolve_flip_texture_for_role` also appears in its own assertion, so a lost cut makes the guard
    vacuous. That is the #4842 defect class.
- **Suggested Fix**:
  - Put one non-`cfg(test)` `#[doc(hidden)] pub mod source_scan` (`production_text`, `rust_files`) in
    `byroredux-core`, or in a tiny dev-dependency crate.
  - Delete the copies.
  - Turn `.unwrap_or(src)` into `.expect(...)`.

**Validated at HEAD 9fcfdc3fc**: `production_text` exists at `crates/renderer/src/source_scan.rs:20`, `crates/physics/src/source_scan.rs:21`, `byroredux/src/extensions/mod.rs:522`; inline `split_once("\n#[cfg(test)]...")` copies at `app_events.rs:1746/1860/1901`, `anim_convert.rs:485` (with `.unwrap_or(src)` at :487), `skin_slot_pool.rs:1112`, `water.rs:1902`, `frame_upscaler.rs:1320/1364/1628`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
