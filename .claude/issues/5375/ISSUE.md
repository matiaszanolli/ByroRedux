# #5375: FNV-2026-10-08-D2-01: A later plugin's DIAL override replaces the master's whole `DialRecord`, INFO list included. Each FNV story DLC drops 8.5–9.9k FalloutNV.esm INFOs, and `GREETING` falls from 5,300 INFOs to 11–129

**Labels**: high,esm-plugin,gameplay,dialogue,bug,game:fnv,game:fo3,game:fo4,game:skyrim,legacy-compat
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5375

**Source**: `docs/audits/AUDIT_FNV_2026-10-08.md` — `FNV-2026-10-08-D2-01` (HEAD `00f580e09`)

- **Severity**: HIGH. Authored content is silently dropped under a realistic, documented launch shape, and the topic is the one the new #5367 greeting, force-greet and voice layers depend on. There is no warning and no fallback.
- **Dimension**: ESM Data Slice. The mechanism (load-order merge) belongs to `/audit-esm`. Consumers are `/audit-gameplay` and `/audit-scripting`.
- **Location**:
  - `crates/plugin/src/esm/records/grup_walker.rs:243` and `:393`: each plugin inserts a fresh `DialRecord`.
  - `crates/plugin/src/esm/records/grup_walker.rs:284` and `:403`: INFO children are pushed onto that plugin's copy.
  - `crates/plugin/src/esm/records/index.rs:41-53`: `map_category!` merges with `HashMap::extend`.
  - `crates/plugin/src/esm/records/index.rs:663`: `dialogues` uses that macro.
  - `crates/plugin/src/esm/records/index.rs:1086-1192`: `merge_from`.
- **Status**: NEW. No open or closed issue and no 2026-10-05/-08 report mentions DIAL/INFO merging across plugins. ESM-2026-10-08 covers INFO `DATA`/`QSTI` decode only.
- **Description**:
  - In the Gamebryo/Creation format, INFO records are independent records nested under their DIAL. A plugin that adds or changes INFOs on a master topic ships an override copy of the DIAL record plus a Topic Children group that holds **only its own new or changed INFOs**. The engine composes the topic's INFO set across the load order by INFO FormID.
  - ByroRedux parses each plugin into its own `EsmIndex`, so the DLC's `DialRecord` holds only the DLC's INFOs. `merge_from` then folds `dialogues` with `extend`, which is last-write-wins on the whole record. The master's INFOs vanish.
  - Overrides that carry no Topic Children group leave the topic with **zero** INFOs.
  - Nothing else merges `infos`. A repo-wide grep finds only the two per-plugin `push` sites.
- **Evidence**: a raw GRUP walk over the shipped masters (`/tmp/audit/fnv/py/`).
  - FalloutNV.esm alone: `GREETING` 0x000000C8 has 5,300 INFOs, out of 23,247 INFOs in the file.
  - Per story DLC loaded after FalloutNV.esm:

    | DLC | Master DIALs overridden | Master INFOs replaced | DLC INFOs kept | Topics left with 0 INFOs | `GREETING` after merge |
    |---|---|---|---|---|---|
    | DeadMoney | 93 | 9,914 | 424 | 8 | 73 |
    | HonestHearts | 79 | 8,573 | 631 | 8 | 69 |
    | OldWorldBlues | 24 | 8,715 | 453 | 0 | 129 |
    | LonesomeRoad | 56 | 9,325 | 109 | 5 | 11 |
    | GunRunnersArsenal | 10 | 13 | 10 | 0 | — |

    GunRunnersArsenal hits vendor topics, e.g. `188AlexanderSeeInventory` drops from 3 INFOs to 1.
  - Other topics hit include `RadioHello` (129 INFOs), `Attack` (249), `Hit` (156), `HELLO` (1,074), `Death` (93) and `StartCombatResponse` (53).
  - FO3, same shape: Anchorage keeps 130 `GREETING` INFOs, ThePitt 327, BrokenSteel 301, PointLookout 145 and Zeta 148.
  - Skyrim: `Update.esm` overrides 59 Skyrim.esm DIALs, replacing 790 INFOs with 108 and emptying 1 topic, e.g. `DialogueRiftenHellos` and `DBAstridSecondGreetForcegreet`.
  - Code:
    ```rust
    // index.rs:46-48 (map_category!), used for `dialogues` at :663
    |target: &mut EsmIndex, source: &mut EsmIndex| {
        target.$field.extend(std::mem::take(&mut source.$field));
    },
    ```
- **Impact**:
  - Every FNV DLC launch is affected, including the documented `--master FalloutNV.esm --esm HonestHearts.esm` shape that `m48-5-fnv-hud.sh` boots into `GSDocMitchellHouse`, the same cell as `dt1`.
  - In that session, #5367 Phase G's generic greeting for Doc Mitchell can only select among HonestHearts' 69 Zion-NPC greeting INFOs, all gated to DLC speakers.
  - Force-greet `PKDD` topics that a DLC overrides, Say-Once/Random pools, and Phase V voice (which resolves from the chosen INFO) all see only the last plugin's INFOs.
  - With all four story DLCs loaded, only the last-loaded one's INFOs survive on each shared topic.
  - The same loss affects FO3 DLC launches and every Skyrim load order that includes `Update.esm`. The default `--game skyrim_se` profile loads Skyrim.esm alone. The DLC repro in CLAUDE.md lists `Update.esm`.
  - The single-master floor test `dialogue_greeting_and_forcegreet_fnv_floor` cannot see this.
- **Related**: #5367 (Phases G/F/V), #5271 (QSTI per INFO), #3543 (tombstone merge). Cell children already have a dedicated merge (`merge_cell_references`, `crates/plugin/src/esm/cell/mod.rs:1433`). DIAL children have none.
- **Suggested Fix**:
  - Give `dialogues` a dedicated merge entry in `EsmIndex::categories()`. The override's header fields replace the master's. Its `infos` are folded into the master's by INFO FormID: a later INFO with the same id replaces, a new one appends, and a Deleted INFO is removed.
  - Order the result by INFO `PNAM` (previous INFO) rather than plugin append order.
  - Pin it with a two-plugin synthetic test and a real-data floor: FalloutNV.esm + HonestHearts.esm `GREETING` ≥ 5,300 INFOs.

## Completeness Checks
- [ ] **SIBLING**: Same last-write-wins whole-record merge checked for every other `map_category!` record that owns nested child records (QUST stages/objectives, PACK, any parent+children category) in `EsmIndex::categories()`
- [ ] **TESTS**: A regression test pins this specific fix
