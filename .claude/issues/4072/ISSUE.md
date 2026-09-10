# #4072 — ESM-2026-09-09-D6-07

lstring id `0` means "no string", but it decodes to the literal text `<lstring 0x00000000>` — 10 889 fields on `Skyrim.esm`, 8 030 on `Fallout4.esm`

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4072 --json state`).

---

- **Severity**: MEDIUM
- **Dimension**: Localized Strings
- **Record / Sub-record**: `DESC` (CLAS / RACE / AVIF / MGEF / PERK / IMOD / MESG / SPEL), `NAM1` (INFO), `NNAM` (QUST)
- **Location**: `crates/plugin/src/esm/records/common.rs:193-202`
- **Status**: NEW
- **Description**: A localized plugin writes a 4-byte `00 00 00 00` payload for an
  lstring-bearing sub-record that has **no** authored string — the same
  "absent" convention a null FormID uses, and no companion table contains id 0
  (I checked: `skyrim_english` and `fallout4_en` both start at id `0x1`).
  `read_lstring_or_zstring` does not special-case it: `resolve_lstring(0)` misses,
  and the `unwrap_or_else` synthesises the human-readable string
  `"<lstring 0x00000000>"`, which is then stored in `description` /
  `response_text` / `text` as if it were content. Callers that test
  `.is_empty()` get `false`; callers that render get a placeholder where the
  authored data says "nothing".
- **Evidence**:
  - `records/common.rs:196-199` — no zero guard before the placeholder:
    ```rust
    // Resolve against the active StringTableSet first (#989).
    // Falls back to the placeholder when no table is installed or
    // the ID is absent from all three companion files.
    return resolve_lstring(id).unwrap_or_else(|| format!("<lstring 0x{:08X}>", id));
    ```
  - Census of 4-byte `0x00000000` payloads on text-bearing sub-record codes
    (`FULL`/`DESC`/`SHRT`/`NAM1`/`CNAM`/`NNAM`/`ITXT`/`BTXT`/`RNAM`/`BPTN`):

    | master | total | top contributors |
    |---|---:|---|
    | `Skyrim.esm` | **10 889** | `ARMO/DESC` 2 724, `WEAP/DESC` 2 451, `SPEL/DESC` 733, `CLAS/DESC` **138 (100 % of CLAS)**, `AVIF/DESC` 128, `MESG/DESC` 91, `RACE/DESC` 76, `PERK/DESC` 71, `QUST/NNAM` 11 |
    | `Fallout4.esm` | **8 030** | `COBJ/DESC` 1 812, `OMOD/DESC` 776, `ARMO/DESC` 628, `SPEL/DESC` 326, `MESG/DESC` 291, `AVIF/DESC` 282, `WEAP/DESC` 251, `PERK/DESC` 105, `INFO/NAM1` **43** |

    (Some codes in that census belong to records whose `DESC`/`CNAM` this crate
    does not route through the lstring reader — e.g. `PACK/CNAM`, `KYWD/CNAM` —
    so the *reachable* subset is smaller than the totals. The reachable sites are
    confirmed by reading the enclosing parsers: `parse_clas`
    (`actor/mod.rs:1788`), `parse_race` (`actor/mod.rs:1504`), `parse_avif`
    (`misc/effects.rs:62`), `parse_imod` (`misc/effects.rs:227`), `parse_perk`
    (`misc/magic.rs:296`), `parse_mgef` (`misc/magic.rs:669`), `parse_mesg`
    (`misc/dialogue.rs:538`), `parse_info` NAM1 (`misc/dialogue.rs:203-206`), and
    `parse_qust` NNAM (`misc/quest.rs:1110`). On `Skyrim.esm` alone that is
    138 + 76 + 128 + 91 + 71 + 11 = **515** confirmed-reachable fields, 100 % of
    them for `CLAS`.)
  - Table id ranges confirm 0 is never a valid id: the lowest id in
    `skyrim_english.strings` is `0x1`, in `fallout4_en.strings` `0x1`.
- **Impact**: Every affected record carries the string `<lstring 0x00000000>`
  where the plugin says "no description". Unlike a genuine unresolved id, this
  one can never be fixed by loading the right table — it will survive the D6-01
  fix and read as a localization failure to anyone debugging it. It also poisons
  emptiness checks, which is the shape that turns a cosmetic artifact into a
  logic bug (e.g. "show the description panel only if non-empty").
- **Related**: ESM-2026-09-09-D6-01, ESM-2026-09-09-D6-02 (both concern *genuine*
  misses; this one is a false miss).
- **Suggested Fix**: Return `String::new()` for `id == 0` in
  `read_lstring_or_zstring`, before the table lookup — mirroring `remap_fid`'s
  existing `if raw == 0 { return 0; }` null convention (`common.rs:214-216`) —
  and pin it with a unit test alongside the three existing
  `read_lstring_or_zstring_*` tests at `common.rs:553-597`.

---
