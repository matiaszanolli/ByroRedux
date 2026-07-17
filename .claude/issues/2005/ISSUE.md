# 2005: NIF-D2-A: nif-parser.md's GameVariant trait pattern example reintroduces the exact bug class #1838/#1839 fixed

https://github.com/matiaszanolli/ByroRedux/issues/2005

Labels: medium, nif-parser, documentation

**Severity**: MEDIUM · **Dimension**: Version Gating
**Location**: `docs/engine/nif-parser.md:180-200` (code block at lines 184-191)
**Status**: NEW
**Audit**: docs/audits/AUDIT_NIF_2026-07-16.md (NIF-D2-A)

## Description
The doc's example shows six `stream.variant().has_*()` calls, none of which exist on `NifVariant` any more — `#1840` deleted the last call-site-less predicates precisely because a `#1277` migration onto them broke real files (transitional `v20.2.0.7, uv=11, bsver<=26` content misdetected via `NifVariant` and slipped the stream by 2-4 bytes, #160/#1331), and `#160`/`#1331`/`#1838`/`#1839` reverted the migration. The current doctrine is to gate on raw `stream.bsver()` against named `crate::version::bsver` constants, not `NifVariant` helpers. The doc was never updated after the revert.

## Evidence
`grep -rn "has_properties_list\|avobject_flags_u32\|has_material_crc\|uses_bs_lighting_shader\|uses_bs_tri_shape\|uses_fo76_shader_flags" crates/nif/src` → zero non-comment hits. Revert rationale verbatim at `crates/nif/src/blocks/base.rs:78-81`.

## Impact
A contributor following the doc's literal example can't compile; more realistically, they may add a *new* `NifVariant`-keyed helper for a new feature and reintroduce the exact mis-parse class already paid down, this time uncaught by any compile error.

## Related
#1897, #1840, #1841, #1511, #1838, #1839, #160, #1331.

## Suggested Fix
Replace the doc's example with the actual current doctrine (`stream.bsver()` vs. named `bsver` constants) and add a short note on why `NifVariant`-method gating was tried and reverted.

## Completeness Checks
- [ ] TESTS: N/A (documentation-only fix; no code path to regress)
