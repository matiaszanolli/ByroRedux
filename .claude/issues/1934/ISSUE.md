# CAUSTIC-D14-01: #1234 named-macro fix in caustic_splat.comp has no regression-test coverage

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1934

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/shader_constants.rs` (`triangle_shaders_use_named_instance_flag_constants`); target `crates/renderer/shaders/caustic_splat.comp:200`
**Status**: NEW

## Description
The #1234 fix — replacing the bare literal `4u` with `INSTANCE_FLAG_CAUSTIC_SOURCE` in `caustic_splat.comp` — is not protected by any regression test. The anti-literal scan iterates only over `[("triangle.frag", ...), ("triangle.vert", ...)]` and searches for the token `inst.flags`. `caustic_splat.comp` is absent from that list, and even if added its accessor is `instances[instIdx].flags` (a different pattern), so the scan would not match it. A future edit reverting caustic line 200 to `flags & 4u` would compile clean and pass the entire suite.

## Evidence
Test source enumerates exactly the two triangle shaders. The parallel `4u` literal is still live in the generated header (`include/shader_constants.glsl:64`), so a silent revert is plausible.

## Impact
Latent. If bit 2 is ever reassigned and caustic silently kept `4u`, caustic-source selection would drift with no test failure. Cosmetic/correctness risk only under a future flag renumbering; no current-behavior defect.

## Related
#1234 (original fix); #427 (layout drift assert — a different net)

## Suggested Fix
Extend `triangle_shaders_use_named_instance_flag_constants` (or add a sibling) to also scan `caustic_splat.comp`, generalizing the accessor pattern to also catch `instances[…].flags &`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
