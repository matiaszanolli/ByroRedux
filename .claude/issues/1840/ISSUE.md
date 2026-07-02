# NIF-D2-03: Four call-site-less NifVariant helpers survive the #938/#1511 prunings — the exact class those issues deleted

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1840
**Labels**: bug, nif-parser, low

**Severity**: LOW
**Dimension**: Version Gating (dead helper)
**Location**: `crates/nif/src/version.rs:555` (`has_properties_list`), `:564` (`avobject_flags_u32`), `:592` (`has_effects_list`), `:616` (`uses_bs_tri_shape`)
**Status**: NEW

## Description

**Game Affected**: none (dead code / contributor foot-gun)

#1511 deleted six helpers whose only references were their own test asserts; #938 deleted three more. Four helpers remain in the same state:
- `uses_bs_tri_shape` — exactly one reference in the entire repo: its own definition. Git pickaxe (`-S ".uses_bs_tri_shape()"`) returns nothing — never called in any commit (dispatch routes BSTriShape by block-type name). Verified: 0 non-test/non-version call sites.
- `avobject_flags_u32` — zero invocations anywhere (incl. tests); its remaining references are "do NOT use this" comments at `base.rs:78` / `shader.rs:168` (#1331). The archetypal trap-family member.
- `has_properties_list` / `has_effects_list` — `version.rs` test asserts only; production reads raw bsver per #160 with comments pointing away from the helpers (`base.rs:107`, `node.rs:111`).

Secondary (softer): the `ShaderFlags` typed view (#1277 Task 6, `shader_flags.rs`) has zero production adopters a month after landing — it needs either a consumer or a decision.

## Evidence

Repo-wide grep for `.<helper>()` production hits: none (the residual grep hits for `has_properties_list`/`avobject_flags_u32`/`has_effects_list` are the "do NOT use" comment references, not calls). Same result at baseline `2d4c350d` — the 2026-06-23 audit's "no dead helpers" claim did not hold at its own baseline. Confirmed live at HEAD `1b4e8e84`: `grep -rn "\.has_properties_list(" crates byroredux | grep -v version.rs` only hits a comment at `base.rs:101`; same pattern for the other three (comment-only or zero hits).

## Impact

Contributor foot-gun — adopting `avobject_flags_u32` or `has_properties_list` in a new parser would re-introduce the #1331/#160 transitional-export mis-parse the raw-bsver call sites were specifically fixed to avoid.

## Suggested Fix

Delete the four (plus their `version.rs` test fns) per the #1511 precedent; decide the fate of `has_shader_alpha_refs`/`has_material_crc`/`has_culling_mode` together with the NIF-D2-01 (#1838)/NIF-D2-02 (#1839) fix; either adopt or ticket the `ShaderFlags` view.

## Completeness Checks
- [ ] **SIBLING**: Coordinate with NIF-D2-01 (#1838)/NIF-D2-02 (#1839) — fixing those orphans three more helpers (`has_shader_alpha_refs`, `has_material_crc`, `has_culling_mode`) that should be deleted in the same sweep
- [ ] **TESTS**: Deleting dead code + its dead test fns; no new regression test needed beyond confirming `cargo test -p byroredux-nif` stays green post-deletion
