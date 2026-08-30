# RT-6: the P2 playable-slice gate 5 fails on a working engine — `^`-anchored regex vs escaped single-line `byro-dbg` output

**Issue**: #3552
**Labels**: bug, medium, tech-debt, test-gap
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-30.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-30.md` — RT-6.

## Description

`docs/smoke-tests/p2-melee-core.sh` reports `FAIL -- slot 9 carries no Inventory column — gate 5 is unassertable`. **The save does carry it.** The gate is broken, not the engine.

## Evidence

The captured `save.info` reads:
```
... EquippedWeapon: 17 rows\n  Inventory: 91 rows\n  LightFlicker: 56 rows ...
```
and `inventory.status` reads `stack_rows=22 item_count=205 occupied_slots=6 equipped_weapon=0x00013790`.

The gate uses an anchored regex (`docs/smoke-tests/p2-melee-core.sh:334`):
```sh
grep -Eq '^  Inventory: [0-9]+ rows' "$save_info_log" \
```

But console output returns as `DebugResponse::Value { data }` and is printed by `serde_json::to_string_pretty` (`tools/byro-dbg/src/display.rs:8`), which renders a JSON **string** on **one** line with `\n` escaped. A `^`-anchored match can never succeed. Every other assertion in the script uses `grep -F` and passes.

## Impact

P2 reports FAIL on a healthy build, and the **save -> exit -> reload round trip that gate 5 exists to prove is never actually exercised**. The playable-slice contract's third gate has been permanently red for a reason unrelated to the engine.

## Suggested Fix

Replace the anchored regex with `grep -Fq 'Inventory: '` (or `grep -Eq 'Inventory: [0-9]+ rows'` without `^`), matching the `-F` style the rest of the script already uses.

## Completeness Checks
- [ ] **SIBLING**: Every other `^`-anchored `grep -E` in `docs/smoke-tests/*.sh` audited for the same escaped-single-line JSON hazard
- [ ] **TESTS**: After the fix, gate 5 must actually *fail* on a deliberately broken save round trip — confirm it can go red, or it is still unassertable
