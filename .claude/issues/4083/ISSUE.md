# #4083 — ESM-2026-09-09-D7-08

`memory-budget.md`'s new ESM-Index section transcribes a map count that was already wrong when it was written

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4083 --json state`).

---

- **Severity**: LOW
- **Dimension**: ESM→ECS Handoff (doc rot)
- **Record / Sub-record**: —
- **Location**: `docs/engine/memory-budget.md:24-25`, `:48-50`
- **Status**: NEW
- **Description**: the section added by `f2a91b2b` (2026-08-31) to close
  ESM-2026-08-30-D7-02 opens *"`EsmIndex` … is 93 session-lifetime `HashMap`s, one
  per record type"*. At `f2a91b2b` itself the struct declared **92** `HashMap`
  fields; at HEAD it declares **94** (`leveled_spells` and `grasses` landed since).
  So the number was off by one on the day it was written — unless it silently
  counted `cells: EsmCellIndex`, which is not a `HashMap` and is not "one per record
  type" — and is off by two now. The same figure is repeated at `:48`
  (*"Most of the 93 maps are lean"*).
- **Evidence**:
  ```
  docs/engine/memory-budget.md:24   `EsmIndex` (`crates/plugin/src/esm/records/index.rs`) is 93 session-lifetime
  docs/engine/memory-budget.md:48   Most of the 93 maps are lean, but a meaningful fraction — `camera_shots`,
  ```
  Counted by extracting the `pub struct EsmIndex { … }` body and matching
  `pub <ident>: HashMap<`: **94** at HEAD, **92** at `f2a91b2b`. The
  `categories()` table has 100 rows (94 map rows + `magic_effects_by_code` keyed by
  value + 6 `cell_category!` rows that count through `cells.*`), and
  `record_types` is the single recorded exclusion.
- **Impact**: cosmetic on its own, but it is the derived-count-in-prose class the
  project has filed before (#3681, #3866), in a doc whose entire purpose is to be
  the number of record. Anyone re-deriving it will get a third answer.
- **Related**: #3718 (the finding this section closed); CLAUDE.md's standing
  instruction to re-derive counts rather than trust a transcribed one.
- **Suggested Fix**: drop the literal — *"one `HashMap` per record category (see
  `EsmIndex::categories()`; re-derive with
  `grep -c 'map_category!' crates/plugin/src/esm/records/index.rs`)"* — the same
  treatment CLAUDE.md already prescribes for the debug-server component count.

---
