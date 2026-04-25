# SK-D5-04: Stream-alignment drift on 7 Skyrim parsers — 100% → ~99.7% parse-rate regression

## Finding: SK-D5-04

- **Severity**: MEDIUM (per `_audit-severity.md` "NIF parse mismatch (stream position off)")
- **Source**: `docs/audits/AUDIT_SKYRIM_2026-04-24.md`
- **Game Affected**: Skyrim SE (and likely Skyrim LE) — vanilla `Skyrim - Meshes0/1.bsa`
- **Logs**: `/tmp/audit/skyrim/meshes0.err`, `/tmp/audit/skyrim/meshes1.err`

## Description

Seven block parsers misalign vs declared `block_size`. Each survives because the block-size table snaps the stream forward, but the per-block fields are wrong (over-consumes leak the next block's prefix into the current parse; under-consumes drop tail fields):

| Parser | Drift | Note |
|---|---|---|
| `BSLODTriShape` | over-consumes 23 bytes | Expected 109, consumed 132 |
| `NiStringsExtraData` | under-consumes 26 bytes | Strings array body never read; entire payload lost |
| `BSLagBoneController` | under-consumes 12 bytes | Expected 38, consumed 26 |
| `BSWaterShaderProperty` | over-consumes 14 bytes | Expected 40, consumed 54 |
| `BSSkyShaderProperty` | over-consumes 10 bytes | Expected 44, consumed 54 |
| `bhkBreakableConstraint` | under-consumes 41 bytes | Expected 73-77, consumed 36 |
| `BSProceduralLightningController` | under-consumes 69 bytes | Expected 95, consumed 26 |

This is the second driver of the 100% → ~99.7% parse-rate regression on `Skyrim - Meshes0.bsa` (the first being SK-D5-03).

## Highest-Yield Fixes

1. **`NiStringsExtraData`** (>30 occurrences in MeshXX, all losing their string list — used for SpeedTree LOD bone names + anim-event trigger lists). #164 was closed with the parser added; this regression suggests an incomplete read of the array body. Re-verify against nif.xml `NiStringsExtraData`.
2. **`BSWaterShaderProperty` / `BSSkyShaderProperty`** — over-consumes leak ~10-14 bytes of the next block's prefix into the current parse, so flag/colour fields will be subtly wrong even though the file rounds out.
3. **`bhkBreakableConstraint`** — high drift (41 bytes); collision behaviour for breakables affected.
4. **`BSLagBoneController`** — under-consume drops the bone-list tail; lag-bone animation broken.
5. Lowest priority: **`BSLODTriShape`** (deprecated TES3-era hack, rarely shipped) and **`BSProceduralLightningController`** (1 vanilla file).

## Suggested Fix

Each parser needs a structural review against `/mnt/data/src/reference/nifxml/nif.xml` per the No-Guessing policy. File sub-issues per parser if the bundle becomes too large for one PR.

## Related

- #164 (closed): NiStringsExtraData parser added — but evidently incomplete vs the strings-array body read.
- #359 (closed): BSTriShape data_size sanity check — the same defensive pattern would surface this drift class earlier.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: For each parser, cross-check the nif.xml definition end-to-end including version-gated fields.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Per parser, add a roundtrip test that asserts `consumed == declared block_size`.

_Filed from audit `docs/audits/AUDIT_SKYRIM_2026-04-24.md`._
