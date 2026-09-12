# SF-2026-09-11-D6-02: BSShaderCRC32 hash derivation is fully reproducible (32/32) — retract the stale "do not repeat this search" instruction

**Issue**: #4280 — https://github.com/matiaszanolli/ByroRedux/issues/4280
**Labels**: low,nif-parser,nif,doc-rot,game:starfield,legacy-compat,documentation

**Severity**: LOW
**Dimension**: Dimension 6 — NIF Shader Blocks, BSVER 155+
**Location**: `crates/nif/src/shader_flags.rs:515-524 (comment); docs/audits/AUDIT_STARFIELD_2026-08-30.md:288-293`
**Status**: NEW

## Description
Two standing comments — `crates/nif/src/shader_flags.rs:515-524` and `docs/audits/AUDIT_STARFIELD_2026-08-30.md:288-293` — record the `BSShaderCRC32` hash derivation as opaque/unrepeatable ("recorded so the search is not repeated"). This audit established the derivation empirically (32/32 match): reflected CRC-32, polynomial `0xEDB88320`, init `0`, no final XOR, over the flag name in ASCII **uppercase** exactly as nif.xml spells it — equivalent to `crates/bsa/src/csg.rs::bscrc32`'s parameterization except for the case fold (CSG lowercases, shader flags uppercase). The prior negative result reused `bscrc32`, which hard-codes a lowercase fold internally, so varying input case against that function collapsed every variant onto the same lowercase hash and the correct uppercase form was never actually tested.

## Evidence
`crates/nif/src/shader_flags.rs::bs_shader_crc32` now names all 32 of 32 nif.xml `BSShaderCRC32` entries — complete spec coverage, 10/10 of the values observed in vanilla Starfield — per the CRC32 Flag Table in `docs/audits/AUDIT_STARFIELD_2026-09-11.md`.

## Impact
No functional impact (the 32 values were already hard-coded correctly, just described as un-re-derivable). Documentation-accuracy risk: the stale "do not repeat this search" instruction, left uncorrected, would misdirect a future contributor away from what is now a solved, reproducible derivation — the exact failure mode `_audit-common.md` warns against.

## Related
None filed.

## Suggested Fix
Update both comment sites (`shader_flags.rs:515-524` and, if still referenced, the 2026-08-30 audit doc) to record the now-solved derivation instead of the stale "unrepeatable" claim.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
