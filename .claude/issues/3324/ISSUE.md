# FNV-2026-08-26-D4-01

**Issue**: #3324
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: MEDIUM
**Dimension**: 4 — ESM Record Parser
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/plugin/src/esm/records/items.rs:245-262` (FNV `DNAM` arm),
field declared at `:119`, defaulted at `:184`.

**Premise verified**: `grep -n 'b"VATS"' crates/plugin/src/esm/records/items.rs`
returns nothing — there is no arm for the sub-record anywhere in the crate.
`ap_cost` is initialised `0.0f32` at `:184` and only ever written on the
`GameKind::Fallout4` path (`:296`). The FNV arm's own comment states:

```
// `ap_cost`'s true DNAM offset is unconfirmed; it stays
// at the zero default until the full layout is mapped.
```

**Evidence** — census of all 261 FalloutNV.esm WEAP records:

```
WEAP sub-record census: EDID:261 OBND:261 ETYP:261 DATA:261 DNAM:261 CRDT:261
  VNAM:261 FULL:251 VATS:245 INAM:237 …
  VATS sizes: {20: 242, 16: 3}
```

A dedicated `VATS` sub-record exists on **245 of 261** weapons. Decoding it as
`u32 effect + f32 + f32 + f32 + u8` (17 B payload padded to 20) produces
self-consistent, clean data — e.g. `WeapKnifeCombatCass`:

```
raw = 00000000 000048 42 3333333f 00006041 01000000
      effect=00000000  f32@4=50.00  f32@8=0.70  f32@12=14.00  u8@16=1
```

and the f32 at offset 12 across all 245 records is an entirely integral
distribution in the AP-cost range:

```
{0.0: 200, 14.0: 4, 15.0: 3, 16.0: 5, 17.0: 3, 22.0: 1, 25.0: 1, 26.0: 1,
 27.0: 1, 29.0: 1, 30.0: 4, 35.0: 4, 36.0: 2, 38.0: 2, 43.0: 1, 44.0: 2,
 45.0: 4, 48.0: 6}
```

45 weapons carry a non-zero value; DNAM was never the right place to look.

**Impact**: `ItemRecord::ap_cost` is pinned to `0.0` for every FNV weapon, and
three further authored fields (VATS required skill, VATS damage multiplier, VATS
silence level) have no destination struct at all. The memory note
*"VATS System — AP formulas already in CHARAL, but the runtime … doesn't exist
yet"* names the runtime as the gap; this finding shows the **data layer is also
missing its per-weapon inputs**, so the VATS runtime has nothing to read when it
lands. Silence level additionally feeds stealth detection. Currently latent —
`grep -rn ap_cost` outside `items.rs` finds only one test literal
(`byroredux/src/npc_spawn/tests.rs:468`) — so no visible defect today.

**Fix sketch**: add `b"VATS" if matches!(game, GameKind::Fallout3NV)` to
`parse_weap`, confirm the field order and semantics against xEdit's
`wbStruct(VATS,…)` / UESP `Mod_File_Format/WEAP` before naming the fields (do
not infer them from the histogram alone), and correct the stale `:252-256`
comment so no future audit re-searches DNAM.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
