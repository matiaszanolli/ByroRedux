# #5494: ESM-2026-10-09-D4-01: The #5375 DIAL fold inserts every `PNAM == 0` INFO at index 0, so an override plugin's own chain heads compose in reverse file order. The same data stays in file order when the topic is new to the plugin

**Labels**: bug, dialogue, esm-plugin, game:fnv, game:fo3, medium

**Source**: `docs/audits/AUDIT_ESM_2026-10-09.md` — finding `ESM-2026-10-09-D4-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

> **Severity note**: Raised LOW → MEDIUM at publish per the FO3 audit's addendum (appended below): on The Pitt the catch-all GREETING now shadows 92 conditioned greetings.

- **Severity**: LOW. The order only matters at the dialogue selector's equal-priority tie-break and in the positional Random stack. The engine's cross-plugin head order is unsourced, so only the internal inconsistency is reported.
- **Dimension**: Record Schema Dispatch & Coverage (`EsmIndex::merge_from`)
- **Record / Sub-record**: `DIAL` / `INFO` `PNAM`
- **Location**: `crates/plugin/src/esm/records/index.rs:98-118` (`fold_info_into_topic`) and the `dialogues` merge row (`:709-753`). Consumer: `crates/scripting/src/dialogue.rs:291-301` (stable priority sort, list order inside a tie).
- **Status**: NEW
- **Description**: The fold iterates the override's INFOs in file order:
  - A known FormID replaces its master copy in place.
  - A new INFO goes after its `PNAM` predecessor.
  - A new INFO with `PNAM == 0` goes to position 0.

  Suppose the override has heads H1 (with follower a1) and H2 (with a2). The composed list becomes `[H2, a2, H1, a1, master…]`. If the same plugin authors a topic of its own, `extract_dial_with_info` keeps walk order `[H1, a1, H2, a2]`. Only one test covers the fold (`dialogue_override_folds_infos_into_the_master_topic`), and it inserts a single non-zero-`PNAM` INFO.
- **Evidence** (`scripts/info_pnam*.py`, `scripts/info_heads_quest.py`). FNV story DLCs never point a new INFO's `PNAM` at a master INFO. New INFOs in master-owned topics are either `PNAM = 0` heads or follow the DLC's own INFOs.

  | plugin | `PNAM = 0` heads in master topics | master topics with >1 such head | of those, two heads share a `QSTI` |
  |---|---|---|---|
  | Dead Money | 80 | 3 | 0 |
  | Honest Hearts | 203 | 31 (max 13) | 4 |
  | Old World Blues | 161 | 22 | 0 |
  | Lonesome Road | 69 | 20 | 0 |
  | `Knights.esp` (Oblivion) | 34 | 5 (`GREETING` 13) | 0 |
- **Impact**: Where two heads tie in quest priority (or share a quest, as in 4 Honest Hearts topics), the later-authored head wins the file-order tie-break, and a #5397 Random stack can open or close at a different line.
- **Related**: #5375, #5397, #3600 (all closed). PERF's "checked and not filed" note on the same function's O(n·m) cost is a separate concern.
- **Suggested Fix**: Insert consecutive `PNAM == 0` heads from one override after the previously inserted head from that plugin, not at 0. That preserves the plugin's own file order. Cite a source, or record as unsourced, for where an override's heads sit relative to the master's. Add a two-head fixture.

---

## Addendum to ESM-2026-10-09-D4-01 — the FO3 census, and a concrete selection flip (recommend MEDIUM + `game:fo3`)

*(From `AUDIT_FO3_2026-10-09.md`.)*

- **Status**: Related: ESM-2026-10-09-D4-01 (LOW, `/audit-esm`). Same root cause, so it is not re-filed. That finding's census had no FO3 rows. It called the effect "limited to equal-priority ties + same-quest heads" and the engine semantics "unsourced".
- **Mechanism**: `fold_info_into_topic` inserts every INFO whose `PNAM` is 0 or absent at index 0 (`crates/plugin/src/esm/records/index.rs:98-118`, called at `:733-736`). An override plugin's PNAM-less INFOs therefore compose in **reverse file order**, ahead of the master's.
- **FO3 census** (`py/info_pnam.py`, `info_heads_quest.py`, `dlc_head_reverse.py`):

  | DLC | New INFOs in master topics | PNAM 0/absent | Multi-head master topics (max heads) | Topics whose heads all share one quest |
  |---|---|---|---|---|
  | Anchorage | 461 | 461 (**866 / 866 Anchorage INFOs author no PNAM at all**) | 35 (129) | 35 |
  | ThePitt | 907 | 867 | 21 (301) | 21 |
  | BrokenSteel | 553 | 115 | 8 (43) | 5 |
  | PointLookout | 614 | 102 | 26 (15) | 1 |
  | Zeta | 417 | 46 | 11 (11) | 0 |

- **Why file order is the authored order**: Anchorage authors zero PNAMs, so file order is the only order it has. A plugin's catch-all fallback is authored last in its group. Reversing the order puts it first.
- **Concrete flip** (`py/pitt_greeting.py`):
  - ThePitt `GREETING`, quest `DLC01DialogDowntown`, has 101 INFOs, 93 of them PNAM-less heads.
  - The last-authored head is `0x0100B68E` "Go away.": it has no CTDA, and `DATA` flags1 is `0x01` (Goodbye).
  - Its conditioned siblings come before it, for example "Less talky. More worky." (CTDA fn 59 / 71) and "Hey there! Welcome to the family!" (fn 72).
  - After the fold, "Go away." heads its same-priority group. `select_info`'s #5397 positional rule takes the first passing INFO (`crates/scripting/src/dialogue.rs:302-362`), and `actor_matches` passes because `actor_form_id == 0`. So it wins for every speaker.
  - Result: 92 conditioned Pitt Downtown greetings never play, and every greeting reached through that group closes the conversation.
  - Before #5375, the override replaced the topic, so the DLC's own order was file order. The flip therefore arrived with #5375, inside this window.
- **Suggested Fix**: as ESM-D4-01. Fold an override's PNAM-less run in file order, as one block. Add an FO3 DLC fixture that pins the Pitt group's first passing INFO.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
