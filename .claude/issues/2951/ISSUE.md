# CHAR-D4-04: nine of thirteen BY_FORM_ID reputation FormIDs have no capture-document value

- **Issue**: [#2951](https://github.com/matiaszanolli/ByroRedux/issues/2951)
- **Finding ID**: `CHAR-D4-04`
- **Labels**: `low,legacy-compat,documentation`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2951 --json state`.

---

- **Severity**: LOW
- **Dimension**: Pools, Afflictions & Reputation
- **Game**: fnv
- **Location**: `crates/core/src/character/reputation.rs:191-218` (`fnv_faction_thresholds::BY_FORM_ID`, `thresholds_for`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md:480-484` — "the canonical
  FalloutNV.esm faction FormIDs are now captured (*Gamebryo console commands*) …
  **e.g.** Boomers `000FFAE8`, NCR `000F43DE`, Legion `000F43DD`, BoS `0011E662`".
  Four of thirteen values were transcribed; the remaining nine (Followers, Great
  Khans, Powder Gangers, White Glove Society, Freeside, Goodsprings, Novac, Primm,
  The Strip) exist only in code.
- **Description**: The keys of the fallback lookup — the values that decide whether
  `GetReputationThreshold` finds a faction at all — are 69 % unsourced by the
  capture layer that is supposed to be the authority for every constant.
- **Evidence**: verified this audit against real game data rather than left open:
  scanning `FalloutNV.esm` for each FormID's owning record header shows **all
  thirteen are `REPU` records**, which is the record type `GetReputation`'s
  `param_1` carries — so the shipped values are *correct*. The gap is documentary,
  not numeric:
  ```
  BOOMERS [('REPU','0xffae8')]  BOS [('REPU','0x11e662')]  LEGION [('REPU','0xf43dd')]
  FOLLOWERS [('REPU','0x124ad1')]  GREAT_KHANS [('REPU','0x11989b')]  … 13/13 REPU
  ```
- **Impact**: None to runtime today. The risk is that a future correction or
  extension of the table has nothing to check itself against, and the next audit
  must re-derive from game data (as this one did) instead of diffing a document.
- **Related**: CHAR-D4-05 (same table, provenance described wrongly).
- **Suggested Fix**: Add the full 13-row `(REPU FormID, r1/r2/r3)` table to
  `charal-fnv-fo3-ruleset.md`, replacing the "e.g." list, and note that the values
  were confirmed against `FalloutNV.esm` record headers.

## Completeness Checks
- [ ] **SIBLING**: The same drift class is swept across the other capture documents / docstrings, not just the row cited
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
