# #1482 — REN-D16-NEW-01: generated_header_contains_all_defines pins only 4 of 13 DBG_* bit values

_Snapshot as filed 2026-06-09 from docs/audits/AUDIT_RENDERER_2026-06-09.md. GitHub is authoritative for live state._

**Severity**: LOW (test-coverage gap; diagnostic-only blast radius)
**Dimension**: Tangent-Space & Normal Maps (M-NORMALS) / shader-constant pinning
**Source**: `docs/audits/AUDIT_RENDERER_2026-06-09.md`
**Status**: NEW

## Description
The DBG bit catalog is *supposed* to be value-pinned by `generated_header_contains_all_defines` in `crates/renderer/src/shader_constants.rs`. In reality the **redeclaration** test `triangle_frag_dbg_bits_not_redeclared` (`:208-233`) lists all the `DBG_*` names, but the **value-pinning** test `generated_header_contains_all_defines` (`:46-97`) only asserts the emitted `#define` value for 4 bits: `DBG_BYPASS_POM`, `DBG_VIZ_NORMALS`, `DBG_BYPASS_NORMAL_MAP`, `DBG_DISABLE_HALF_LAMBERT_FILL` (`:70-73`).

The remaining 9 — including the M-NORMALS-relevant `DBG_VIZ_TANGENT` (0x8), plus `DBG_VIZ_GLASS_PASSTHRU` (0x80), `DBG_BYPASS_VERTEX_COLOR` (0x400), `DBG_DISABLE_AO` (0x800), `DBG_LEGACY_LIGHT_ATTEN` (0x1000) — have **no value assertion** anywhere in `crates/renderer/src/`.

## Evidence
- `shader_constants.rs:70-73` — the entire DBG block of the value-pin test (4 entries).
- `build.rs:274-308` emits all bits via hand-written `writeln!` in a fixed order; nothing tests that the emit value/order for the unpinned 9 stays correct.
- A `build.rs` reorder or copy-paste typo (e.g. `{DBG_VIZ_NORMALS}u` under the `DBG_VIZ_TANGENT` line) would compile cleanly, pass `triangle_frag_dbg_bits_not_redeclared`, and ship a wrong-valued diagnostic bit with no test failure.

## Impact
Diagnostic-only — a mis-valued DBG bit would corrupt a debug visualization, not production rendering. No GPU/correctness risk. But it defeats the lockstep guarantee the catalog claims.

## Note
The live catalog has grown past what the audit checklist documented (`0x400`/`0x800`/`0x1000` exist beyond the listed `0x200` ceiling) — the **checklist** is the stale party, not the code.

## Suggested Fix
Extend `generated_header_contains_all_defines` to assert the emitted `#define` value of all 13 `DBG_*` bits (drive it from the same const list the redeclaration test uses, so the two stay in sync automatically).

## Completeness Checks
- [ ] **SIBLING**: apply the same all-bits value-pin to the `MAT_FLAG_*` and `INSTANCE_FLAG_*` generated defines (same generated-header contract).
- [ ] **TESTS**: this finding *is* the test fix.
- [ ] **UNSAFE / DROP / LOCK_ORDER / FFI / CANONICAL-BOUNDARY**: N/A.
