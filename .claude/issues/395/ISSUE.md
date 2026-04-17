# OBL-D5-H3: Stream-position drift masquerades as garbage NiTransformData / NiStringPalette reads

**Issue**: #395 — https://github.com/matiaszanolli/ByroRedux/issues/395
**Labels**: bug, nif-parser, high

---

## Finding

309 of the 678 truncation warnings (see OBL-D5-H1) cite bogus values:
- `NiTransformData: unknown KeyType: 1836409699` (= ASCII `"cers"`)
- `NiTransformData: unknown KeyType: 1665232495` (= ASCII `"oamo"`)
- `NiTransformData: unknown KeyType: 1094647971` (= ASCII `"cate"`)
- `NiStringPalette requested 4294967295-byte allocation` (= 0xFFFFFFFF)
- `NiStringPalette requested 2147483648-byte allocation` (= 0x80000000)

## Root cause

The named blocks are **victims**, not perpetrators. An earlier block's parser consumed the wrong number of bytes; when the next block starts reading, it finds ASCII chunks or absurd lengths where a u32 enum/count should be. On FO3+ we recover via `block_sizes[i]` → seek to `start_pos + size` (`lib.rs:240`). Oblivion has no such table.

## Why the error messages lie

`NiTransformData` and `NiStringPalette` are common late-in-block types, so they disproportionately surface as the "failing" block. The **actual defective parsers** (one block upstream, whichever they are) are silent.

## Fix

Two-step:
1. **Debug-mode drift detector**: in debug builds, every parser self-reports its consumed-byte count on return. Cross-check against `parsed_size_cache` at `lib.rs:260` on reappearance of the same type. Any mismatch is the earliest signal of drift — log `"NiFoo consumed 48 bytes, cached median 44 — suspect"`.
2. **Fix the defective upstream parser(s)** once identified. Candidates from Dim 5 H-1's warning list: `NiPSysEmitterCtlr`, `NiPSysModifierActiveCtlr`, legacy particle types (pre-NiPSys stack), and Havok `bhkRigidBodyT`.

Closing OBL-D5-H3 as "diagnostic tooling added + at least one upstream defect fixed" is a reasonable target; a blanket "no more drift" requires OBL-D5-H2 to land first so unknown types don't compound.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: The drift detector applies to all parsers; gate it behind `debug_assertions` to avoid release overhead.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Hand-craft a NIF where block N's parser is 4 bytes short of the real size; assert the diagnostic fires pointing at block N, not N+1.

## Source

Audit: `docs/audits/AUDIT_OBLIVION_2026-04-17.md`, Dim 5 H-3.
