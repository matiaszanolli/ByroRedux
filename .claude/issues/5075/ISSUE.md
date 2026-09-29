# #5075 — ESM-2026-09-29-D2-01: #4645 routes TRDA's emotion keyword through the load-order remap, but ~45% of vanilla rows author the 0xFFFFFFFF sentinel — one false warn! per row on every plugin with masters

**Labels**: medium, bug, esm-plugin, game:fo4, game:starfield

**Source**: `docs/audits/AUDIT_ESM_2026-09-29.md` — finding `ESM-2026-09-29-D2-01`

**Severity**: MEDIUM

**Dimension**: Sub-Record Byte Accounting (with FormID & Load Order)

**Record / Sub-record**: `INFO` / `TRDA`

**Location**: `crates/plugin/src/esm/records/misc/dialogue.rs:273-293`; `crates/plugin/src/esm/reader.rs:556-564` (the warn arm)

**Status in report**: NEW. It was introduced by the #4645 fix and is the same class as closed #4172, the MGEF `associated_item` sentinel.

## Description

xEdit types the TRDA emotion field `wbFormIDCk('Emotion', [KYWD, FFFF])` (`wbDefinitionsFO4.pas:9733`, `wbDefinitionsSF1.pas:12816`). `FFFF` is the Bethesda "none" sentinel. The decode passes the raw u32 through `remap_fid`, which only short-circuits `0`. For `0xFFFFFFFF` the mod index is `0xFF`. On a plugin with masters, `FormIdRemap::remap` falls into its "out-of-range index — genuinely suspicious" arm and logs at `warn` for every row. #4172 fixed exactly this for MGEF by bypassing the remap for the sentinel (`magic.rs:929-938`), but the new TRDA site repeats the pattern.

## Evidence

```rust
len if len >= 20 => {
    segment.emotion_keyword =
        remap_fid(SubReader::new(&sub.data[0..4]).u32_or_default(), remap);
```
Real-data census of TRDA rows with emotion = `0xFFFFFFFF`:

| plugin | sentinel rows | total rows | share |
|---|---|---|---|
| `Fallout4.esm` | 33,997 | 74,996 | 45% |
| `DLCRobot.esm` | 1,318 | 2,548 | 52% |
| `DLCCoast.esm` | 4,355 | 9,733 | 45% |
| `ShatteredSpace.esm` | 7,620 | 18,847 | 40% |

`Fallout4.esm` has no masters, so its rows take the `debug` pass-through arm. DLCRobot, DLCCoast and ShatteredSpace have masters, so every sentinel row warns.

## Impact

A FO4 load with Automatron and Far Harbor emits about 5,673 `FormID ffffffff has mod_index 255 but plugin has N masters` warnings, and Nuka-World adds more. Starfield plus Shattered Space emits about 7,620. This buries the genuinely suspicious out-of-range warnings the arm exists for. The stored value survives, because the pass-through returns it unchanged, so `emotion_keyword` is correct and nothing consumes it yet. Only the log is affected.

## Related

#4172 (closed, same class), #4645 (closed, the decode that introduced it), D2-02.

## Suggested Fix

Bypass the remap for `0xFFFF_FFFF` at the TRDA site, as #4172 did. The better fix, which prevents a third site, is a single `remap_fid_or_sentinel` helper in `records/common.rs`, used by every `[…, FFFF]`-typed field. Pin it with a test that feeds a sentinel TRDA through a remap that has masters.

Validated at HEAD 9fcfdc3fc: both TRDA arms (≥20 B and ≥12 B) in `crates/plugin/src/esm/records/misc/dialogue.rs` pass the emotion u32 to `remap_fid`, which only short-circuits `0` (`records/common.rs`); `FormIdRemap::remap`'s out-of-range arm in `reader.rs` logs at `warn`. Census figures are from the report.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every `[…, FFFF]`-typed field; MGEF `associated_item` #4172 precedent)
- [ ] **TESTS**: A regression test pins this specific fix (a sentinel TRDA through a remap that has masters)
