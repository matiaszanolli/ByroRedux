# #4068 — ESM-2026-09-09-D4-01

#3616's multi-segment dialogue fix is `TRDT`-only — FO4 and Starfield author `TRDA`, so 16,552 INFO records still collapse to a single response segment

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4068 --json state`).

---

- **Severity**: MEDIUM
- **Dimension**: Record Schema Dispatch (with Dim 2 byte-accounting consequences)
- **Record / Sub-record**: `INFO` / `TRDA` (vs `TRDT`), `NAM1`, `NAM2`
- **Location**: `crates/plugin/src/esm/records/misc/dialogue.rs:212-232` (`TRDT` arm), `:196-211` (`NAM1`/`NAM2` arms), `:340-360` (the derive-flat-fields tail)
- **Status**: NEW — the un-swept half of **#3616** (CLOSED, labelled `medium` / `game:oblivion`)
- **Description**: #3616 (`1da0ebad`, 2026-09-06) converted `InfoRecord`'s response
  handling from assign-to-a-flat-field to push-a-`ResponseSegment`, keyed on a fresh
  `TRDT` opening each segment. Its own SIBLING check looked sideways at `NAM2` (the same
  Oblivion sub-record shape) but not **up the lineage**: FO4 and Starfield renamed the
  per-segment struct opener from `TRDT` to **`TRDA`**, which has no arm anywhere in the
  workspace. With no opener, `NAM1`'s
  `current_response.get_or_insert_with(Default::default).text = …` **assigns into the
  same lazily-created segment on every repeat** — precisely the pre-#3616 behaviour,
  still live for the two newest titles.
- **Evidence**:
  - `grep -rn 'TRDA' crates byroredux docs` → **no matches**. The sub-record is
    unrecognised everywhere.
  - Current `misc/dialogue.rs:196-211` — the two arms that keep the segment open:
    ```rust
    b"NAM1" => {
        current_response.get_or_insert_with(Default::default).text =
            read_lstring_or_zstring(&sub.data);
    }
    b"NAM2" => {
        current_response.get_or_insert_with(Default::default).designer_notes =
            read_zstring(&sub.data);
    }
    ```
    Only the `b"TRDT"` arm at `:222-224` ever calls `current_response.take()` to push a
    finished segment.
  - Census over shipped masters (bounded read-only walker, `/tmp/audit/esm/trda.py` +
    `info2.py`), counting `NAM1` occurrences per `INFO` record:

    | master | INFO records | `TRDT` | `TRDA` | records with >1 `NAM1` | response segments lost |
    |---|---|---|---|---|---|
    | `Oblivion.esm` | 19 278 | 23 877 | 0 | — (fixed by #3616) | 0 |
    | `Fallout3.esm` | 22 327 | 29 463 | 0 | — | 0 |
    | `FalloutNV.esm` | 23 247 | 29 674 | 0 | — | 0 |
    | `Skyrim.esm` | 31 465 | 34 427 | 0 | — | 0 |
    | **`Fallout4.esm`** | 78 087 | **0** | **74 996** | **4 988** | **7 288** |
    | **`Starfield.esm`** | 126 347 | **0** | **130 284** | **11 564** | **16 911** |

    FO4 `NAM1`-per-record distribution: `{0: 10379, 1: 62720, 2: 3486, 3: 1037, 4: 285,
    5: 98, 6: 46, 7: 17, 8: 11, 9: 4, 10: 2, 11: 1, 13: 1}`. Starfield's tops out at 15.
  - `TRDA` payload width is **20 bytes on FO4** (74 996/74 996) and **12 bytes on
    Starfield** (130 284/130 284) — two different layouts. I am deliberately **not**
    proposing offsets for either: I have no xEdit/UESP citation for `TRDA` in hand, and
    guessing one is exactly what `feedback_no_guessing` forbids. The segment-splitting
    half of the fix needs no layout knowledge at all.
  - These INFO records genuinely reach `parse_info`: FO4/Starfield nest the dialogue tree
    under `QUST`, and `grup_walker.rs:346-355` walks it and pushes each `parse_info`
    result onto its parent `DIAL`.
- **Impact**: live consumer. `crates/scripting/src/dialogue.rs:372-382` reads
  `info.response_text` (drives `estimate_dialogue_duration`), `info.designer_notes` and
  `info.emotion_type`. On FO4 and Starfield: (a) 4 988 + 11 564 multi-segment NPC lines
  play only their **last** segment — 24 199 authored response segments discarded, ~3× the
  4 617 that #3616 was filed for; (b) `emotion_type` and `response_number` are `0` on
  **every** FO4/Starfield INFO, because only the `TRDT` arm ever sets them; (c) the
  `responses` vector — the whole point of #3616 — always has length ≤ 1.
- **Related**: #3616, #3614; `/audit-scripting` Dim 7 owns the consumer.
- **Suggested Fix**: two independent steps, in this order. (1) Zero-risk and layout-free:
  add `TRDA` to the segment-*opening* arm so a fresh `TRDA` does the same
  `current_response.take() → push` and starts a new empty segment. That alone recovers
  all 24 199 segments. (2) Separately, and only with a cited layout: decode `TRDA`'s
  emotion/response-number fields per game — FO4's 20-byte and Starfield's 12-byte forms
  need their own census and citation before any offset is read.

---
