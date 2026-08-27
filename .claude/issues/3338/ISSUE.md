# FNV-2026-08-26-D4-03

**Issue**: #3338
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 4 — ESM Record Parser
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/plugin/src/esm/records/actor/mod.rs:1549` (`b"MNAM" =>
record.ranks.push(...)`), doc claim at `:592-593`.

**Premise verified**: the only rank-related arm is `MNAM`; there is no `RNAM`
(rank number) or `FNAM` (female rank title) arm, and `ranks` is a flat
`Vec<String>` indexed positionally.

**Evidence** — per-record sub-record ordering across all 682 FNV factions:

```
factions where RNAM count != MNAM count: 17
  OmertaFaction        0x10c6f8  RNAM=3 MNAM=2  order=[RNAM, RNAM, MNAM, FNAM, RNAM, MNAM, FNAM]
  MegatonLucasSimmsFaction 0x428cc RNAM=2 MNAM=1 order=[RNAM, RNAM, MNAM]
  NCRCFPowderGangerFaction 0x8d395 RNAM=1 MNAM=0 order=[RNAM]
Totals: RNAM 111 · MNAM 94 · FNAM 53
RNAM value histogram: {0:57, 1:16, 2:9, 3:8, 4:8, 5:4, 6:4, 7:4, 8:1}
```

OmertaFaction authors rank **0** with no title, then ranks 1 and 2 with titles.
The parser yields `ranks = [title_of_rank_1, title_of_rank_2]`, so index 0 returns
rank 1's label — an off-by-one on every faction in that set.

**Impact**: latent. `grep -rn "\.ranks\b"` finds no consumer outside
`actor/tests.rs:306`. It becomes live the moment `XRNK` (43 REFR + FACT ownership
ranks) is used to render an ownership/crime label.

**Fix sketch**: pair `RNAM` with the following `MNAM`/`FNAM` and store
`Vec<FactionRank { index: u32, male: String, female: String }>`, or key a
`HashMap<u32, String>` off the last-seen RNAM.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
