# PHYS-01: extract_ragdoll applies BhkRigidBody CInfo translation/rotation unconditionally, bypassing the is_t gate its sibling extractor requires

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2447
**Finding ID**: PHYS-01 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 4 — PHYSAL (source boundary/extract)
**Location**: `crates/nif/src/import/collision/ragdoll.rs:90-104`, contrast `crates/nif/src/import/collision/mod.rs:316-334` (`extract_from_classic`)
**Status**: NEW

## Description
`extract_from_classic` gates applying a `BhkRigidBody`'s CInfo translation/rotation on `body.is_t` — only `bhkRigidBodyT` activates the offset; plain `bhkRigidBody` carries the same wire fields but Gamebryo treats them as identity even when stale/non-zero bytes survive in vanilla content (fixed under #2316 specifically because applying non-T bytes displaced FO3 architecture colliders). `extract_ragdoll`, building ragdoll bodies from the same block type, reads `body.translation`/`body.rotation` unconditionally with no `is_t` check.

## Evidence
Confirmed directly: `extract_from_classic` computes `has_offset = body.is_t && (...)` before applying the CInfo offset; `extract_ragdoll` computes `translation`/`rotation` from `body.translation`/`body.rotation` unconditionally with no equivalent gate.

## Impact
If a ragdoll bone is authored as plain `bhkRigidBody` carrying stale non-identity translation/rotation bytes — the exact pattern #2316 fixed for architecture — the extractor applies that garbage offset to the body's rest-space pose, propagating through `template_from_imported`'s rest-pose delta into every activation's world-space seed, displacing/misrotating the ragdoll body relative to its bone. Unconfirmed on vanilla content either way (no test pins ragdoll bones as always-T); no comment explains the asymmetry with the sibling extractor.

## Related
#2316 (CLOSED, FO3-D5-01 — the sibling fix this extractor doesn't mirror).

## Suggested Fix
Mirror `extract_from_classic`'s `is_t` gate in `extract_ragdoll`, or add a comment citing real-corpus evidence that ragdoll bones are always `bhkRigidBodyT` if that's actually true.

## Completeness Checks
- [ ] **TESTS**: A regression test constructs a plain (non-T) `bhkRigidBody` ragdoll bone with stale offset bytes and confirms it's ignored, mirroring #2316's architecture test
- [ ] **SIBLING**: Confirm no third `BhkRigidBody` CInfo-reading site has the same gap
