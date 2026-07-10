# DBG-D20-01: gpu_timers.rs module doc describes a non-existent "ran_this_frame" API and wrong flicker behavior

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1940

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/gpu_timers.rs:58-63` (module doc "When a bracket doesn't fire")
**Status**: NEW

## Description
The module doc claims two behaviors the implementation does not provide: (1) "the prior frame's result stays valid (no flicker)" — false, `read_and_reset` builds a fresh default snapshot each call and only fills brackets whose `active_bits` bit is set, so an inactive bracket reads back 0.0, overwriting its previous non-zero value. (2) "the caller flags the bracket as inactive via the `*_ran_this_frame` bits ... Consumers should pair the elapsed-ms field with the bit" — no such API exists anywhere in the struct/accessors.

## Evidence
grep confirms `ran_this_frame` appears only in this comment; `GpuTimerSnapshot` contains only f32 fields; the only accessor `last_snapshot()` returns the bare snapshot with no ran-this-frame bits.

## Impact
Documentation-only. No memory-safety or Vulkan-correctness consequence. The secondary real effect (consumers can't tell "0.0 = inactive" from "0.0 = fast") is a minor telemetry-fidelity limitation, not a bug.

## Related
#1194, #1484/#1499/#1505 (prior gpu_timers doc-rot passes that missed this)

## Suggested Fix
Rewrite the "When a bracket doesn't fire" paragraph to state the actual behavior: inactive brackets read 0.0 (do not retain the prior value), and there is currently no per-bracket ran-this-frame flag exposed to consumers. If the pairing contract is genuinely wanted, expose `active_bits` (e.g. add a `ran: u16` to `GpuTimerSnapshot`) or drop the sentence.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
