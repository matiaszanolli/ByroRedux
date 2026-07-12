# #1833: RT-3 — FNV runtime baseline skin_pool_max is stale (1365 vs live 1364)

**Severity**: low
**Location**: `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv:11`

## Description
The FNV baseline recorded `skin_pool_max 1365`; the current run reports
1364, and all four other baselines (fo3/oblivion/skyrim_se/fo4) already
record 1364. Not a code regression — the pool cap is uniform 1364 across
every game in this run; the baseline unit was just never regenerated
alongside its siblings.

## Suggested Fix
Hand-edit the TSV line from 1365 to 1364 to align with the other four
baselines. No code change required.

---

# #1842: NIF-D2-04 — FLAGS_U32_THRESHOLD doc cites a nonexistent nif.xml token #BS_GTE_26#

**Severity**: low
**Location**: `crates/nif/src/version.rs:322-325`

## Description
The constant's doc cited nif.xml's "`#BS_GTE_26#`" predicate; nif.xml
defines no such token (verified via grep — zero hits). The actual gate is
the inline `vercond="#BSVER# #GT# 26"` on `NiAVObject.Flags` (nif.xml:3442).
Both call sites (`base.rs:82` `> 26`, `properties.rs:44` `>= 26` as the
negation of a different `#BSVER# #LT# 26` gate) already use correct
operators; only the doc's "GTE" contradicted the actual "GT" semantics on
the exact boundary value this constant exists to pin.

## Suggested Fix
Reword to "nif.xml gates `NiAVObject.Flags` on the inline `#BSVER# #GT# 26`
(no named token); u16 at `bsver <= 26`."
