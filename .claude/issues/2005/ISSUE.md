# NIF-D2-A: nif-parser.md's GameVariant trait pattern example reintroduces the exact bug class #1838/#1839 fixed

**Labels**: documentation, nif-parser, medium

**Severity**: MEDIUM
**Dimension**: Version Gating
**Location**: `docs/engine/nif-parser.md:180-200` (code block at lines 184-191)
**Game Affected**: All — this is the parser's own canonical reference doc

## Description
The doc's example shows six `stream.variant().has_*()` calls (`has_properties_list`, `avobject_flags_u32`, `has_material_crc`, `uses_bs_lighting_shader`, `uses_bs_tri_shape`, `uses_fo76_shader_flags`), none of which exist on `NifVariant` any more — `#1840` deleted the last call-site-less predicates precisely because a `#1277` migration onto them broke real files (transitional `v20.2.0.7, uv=11, bsver≤26` content misdetected via `NifVariant` and slipped the stream by 2-4 bytes, #160/#1331), and `#160`/`#1331`/`#1838`/`#1839` reverted the migration. The current doctrine — stated in ~10 in-code comments — is to gate on raw `stream.bsver()` against named `crate::version::bsver` constants, not `NifVariant` helpers. The doc was never updated after the revert.

Confirmed against current code: `grep -rn "has_properties_list\|avobject_flags_u32\|has_material_crc\|uses_bs_lighting_shader\|uses_bs_tri_shape\|uses_fo76_shader_flags" crates/nif/src` returns zero non-comment hits, and `blocks/base.rs:70-84`'s live `NiAVObject::parse` explicitly uses `stream.bsver() > crate::version::bsver::FLAGS_U32_THRESHOLD` with a comment documenting exactly why the `NifVariant`-helper approach was reverted (#1331).

## Evidence
`grep -rn "has_properties_list\|avobject_flags_u32\|has_material_crc\|uses_bs_lighting_shader\|uses_bs_tri_shape\|uses_fo76_shader_flags" crates/nif/src` → zero non-comment hits. Revert rationale verbatim at `crates/nif/src/blocks/base.rs:78-81`. Meta-evidence: this very audit's own Dimension 2 checklist names several of the same six methods as "candidates to check," suggesting the checklist itself traces back to this stale doc section rather than live code.

## Impact
A contributor following the doc's literal example can't compile; more realistically, they may add a *new* `NifVariant`-keyed helper for a new feature and reintroduce the exact `Unknown`/hybrid-corner mis-parse class already paid down, this time uncaught by any compile error.

## Related
#1897 (the code-level instance of the same foot-gun — `ShaderFlags`/`has_shader_property_fo3_fields`/`variant()`), #1840, #1841, #1511, #1838, #1839, #160, #1331.

## Suggested Fix
Replace the doc's example with the actual current doctrine (`stream.bsver()` vs. named `bsver` constants) and add a short note on why `NifVariant`-method gating was tried and reverted.

## Completeness Checks
- [ ] **TESTS**: N/A (documentation-only fix; no code path to regress)

