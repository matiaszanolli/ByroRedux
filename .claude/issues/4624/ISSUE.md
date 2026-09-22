# NIF-D6-2026-09-21-02: parse_havok_packfile pre-sizes Vec::with_capacity(num_sections) straight from an on-disk u32

**Issue**: #4624
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIF_2026-09-21.md)

**Severity**: MEDIUM. No production caller exists today. The special-rules row for untrusted packfile readers makes this **HIGH** as soon as a loader consumes `BhkSystemBinary` blobs.
**Dimension**: Allocation Hygiene
**Game Affected**: FO4 / FO76 / Starfield (`bhkPhysicsSystem` / `bhkRagdollSystem` → `BhkSystemBinary`)
**Location**: `crates/nif/src/blocks/collision/havok_packfile.rs:389-396`

## Description
`num_sections` is read from the blob header and passed to `Vec::with_capacity` before the per-iteration bounds check. `PackfileSection` is about 128 bytes (a String, 7 u32 fields and 3 Vecs). So `num_sections = 0xFFFF_FFFF` requests roughly 550 GB, and the allocation failure aborts the process. A 64-byte crafted blob is enough.

Today only `examples/havok_blob_recon.rs` and the in-module tests reach this code — confirmed via `grep -rn "parse_havok_packfile"` across the workspace, which finds no caller outside `havok_packfile.rs` itself, its own tests, and the example binary (re-exported through `collision/mod.rs` but not consumed by any production import path). That is the same reachability the baseline used to rate the sibling #4155 (same function) MEDIUM.

Confirmed at HEAD `ee6d3fb39`: `havok_packfile.rs:394` still reads `let mut sections = Vec::with_capacity(num_sections as usize);` immediately after `num_sections = read_u32_le(data, 20)?` with no bound applied first.

## Suggested Fix
Bound `num_sections` by `(data.len() - SECTION_TABLE_START) / SECTION_HEADER_SIZE` before reserving (real packfiles carry 3 sections).

## Related
#4155 (closed — sibling finding in the same function, offset-arithmetic widening).

Source: docs/audits/AUDIT_NIF_2026-09-21.md (NIF-D6-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other on-disk-count-driven `Vec::with_capacity` sites in the Havok packfile reader)
- [ ] **TESTS**: A regression test pins this specific fix (forged `num_sections` rejection)
