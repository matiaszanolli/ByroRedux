# #3600: OBL-D3-02: build_conversation_tree orders by PNAM chains, but Oblivion authors zero PNAM and zero ANAM on all 19,278 INFO records

**Source**: `docs/audits/AUDIT_OBLIVION_2026-08-30.md` — Dimension 3 (ESM Record Coverage)
**Severity**: HIGH
**Location**: `crates/plugin/src/esm/records/misc/dialogue.rs` — `parse_info` (`PNAM`/`ANAM` arms) and `build_conversation_tree`

## Description

`build_conversation_tree` orders INFO records by PNAM chains (head = `previous_info == 0`)
and reads the speaker from `actor_form_id` (ANAM). Oblivion authors **zero PNAM and zero
ANAM** on all 19,278 of its INFO records, so the ordering pass is a no-op and the speaker is
always 0.

## Evidence

Measured sub-record census over all 19,278 Oblivion INFO records (`occurrences/records`):

```
INFO CTDA 48531/18920   INFO NAM1 23877/19260   INFO QSTI 19278/19278
INFO DATA 19278/19278   INFO TRDT 23877/19260   INFO TCLT  9698/5611
INFO SCHR 19231/19231   INFO TCLF  4141/3792    INFO NAME  1342/1044
INFO SCRO  8405/5531    INFO SCTX  5718/5718    INFO SCDA  5552/5552
INFO CTDT    72/45      INFO SCHD    47/47
                        <- no PNAM, no ANAM anywhere
```

Code side, verified 2026-08-30: `parse_info` has `b"PNAM"` and `b"ANAM"` arms writing
`out.previous_info` / `out.actor_form_id`, and `build_conversation_tree` walks
`previous_info` forward and backward. There is no Oblivion-era fallback arm.

PNAM/ANAM were introduced **after** Oblivion. Oblivion orders INFOs by record order within
the DIAL group and identifies the speaker through CTDA conditions (48,531 CTDA across 18,920
records — the signal is present and unused for this purpose).

## Impact

Every Oblivion INFO has `previous_info == 0`, so the conversation tree degenerates to 19,278
single-element chains, and `actor_form_id` is 0 on every record. Dialogue for the whole title
is unordered and unattributed — silently, with no parse error, which is exactly the
"silently mis-read" failure class this dimension exists to catch.

## Suggested Fix

Add an Oblivion-era arm: order INFOs by their record order within the DIAL group (the
authored ordering for this generation), and derive the speaker from the INFO's CTDA
conditions rather than ANAM. Keep the PNAM/ANAM path for FO3+ untouched.

## Related

Same file, same generation gap: OBL-D3-03 (TCLF / NAME / CTDT dropped) and OBL-D3-04
(multi-response overwrite).

## Completeness Checks
- [ ] **SIBLING**: the DIAL parser must expose group order for the fallback to consume; check `parse_dial` and the DIAL GRUP walk
- [ ] **TESTS**: a regression test pins a real Oblivion DIAL group producing a multi-element ordered chain and a non-zero speaker
