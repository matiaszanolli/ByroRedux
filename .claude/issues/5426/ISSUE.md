# #5426: FNV-2026-10-08-D5-02: Every conversation open scans all 18,215 FNV DIALs and clones the 5,300-INFO `GREETING` record (twice on force-greet), and GAME-D5-01's unconsumed failure path repeats this every frame

**Labels**: low,performance,gameplay,dialogue,bug,game:fnv,legacy-compat
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5426

**Source**: `docs/audits/AUDIT_FNV_2026-10-08.md` — `FNV-2026-10-08-D5-02` (HEAD `00f580e09`)

- **Severity**: LOW. A CPU cost that only becomes per-frame through GAME-D5-01, whose fix (consume or cool down a failed open) removes the loop.
- **Dimension**: Ambient AI / dialogue on FNV data. The mechanism belongs to `/audit-performance` and `/audit-gameplay`.
- **Location**:
  - `byroredux/src/systems/npc_dialogue.rs:275-280` (`generic_greeting_record`: a linear `.find` over `index.dialogues.values()`).
  - `byroredux/src/systems/npc_dialogue.rs:491-494` (`forcegreet_open`: `.cloned()` of the whole DIAL).
  - `byroredux/src/systems/npc_dialogue.rs:244-265` (`select_on_topic` returns `record.clone()` again).
  - `byroredux/src/systems/forcegreet.rs:112-114` (retried each frame while `forcegreet_open` returns `false`).
- **Status**: NEW, as a per-title magnitude and a correction.
  - AUDIT_PERFORMANCE_2026-10-08 (line 222) records `generic_greeting_record` as "per activation or force-greet, not per frame". That premise fails under GAME-D5-2026-10-08-01's documented retry loop.
  - GAME-D5-01 names the retry but not its cost.
- **Description**:
  - `InfoRecord` owns `String`s (`response_text`, `designer_notes`, `editor_id`), `Vec<ResponseSegment>`, conditions and `topic_links`, so a `GREETING` clone is about 5,300 records' worth of heap allocation.
  - A topic-less force-greet (69 FNV proc-15 packs author no `PKDD`) that fails selection is never consumed. The NPC parked within 128 BU then rescans the 18,215-entry map, clones the record and re-evaluates up to 5,300 INFOs' conditions every frame.
  - On success the open still clones twice, and stores one copy in `DialogueRegistry`.
- **Evidence**: `parse_rate_fnv_esm` reports `dialogues=18215`. The FalloutNV.esm GRUP walk gives `GREETING` 5,300 INFOs. Code:
  ```rust
  None => generic_greeting_record(&index).cloned(),   // npc_dialogue.rs:493
  ```
  and `forcegreet.rs:112`: `if forcegreet_open(world, npc, directive.topic) { opened.push(npc); }`. A `false` keeps the directive.
- **Impact**: a per-frame main-thread cost for every stuck force-greet NPC on FNV (60 candidates per GAME-D5-01). The user's hardware rule treats a CPU bottleneck as a bug.
- **Related**: GAME-D5-2026-10-08-01, AUDIT_PERFORMANCE_2026-10-08 (line 222).
- **Suggested Fix**:
  - Fix GAME-D5-01's consumption.
  - Resolve the generic greeting FormID once per load order, as a resource or a field on `EsmIndex`.
  - Select on `&DialRecord` and clone only the chosen record. Better, store `Arc<DialRecord>` so `DialogueRegistry::insert_topic` shares rather than copies.

## Completeness Checks
- [ ] **SIBLING**: Other whole-`DialRecord` clone sites (`select_on_topic`, `npc_dialogue.rs` topic open at `index.dialogues.get(..).cloned()`) moved to borrow/`Arc` too
- [ ] **TESTS**: A regression test pins this specific fix
