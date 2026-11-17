# Oblivion Actor-Value Identity and Portable Saves — Product Requirements

**Status**: ACTIVE (2026-11-17). Revision 5 corrects the §3.4 `ActorVitals` row; revision 4 files the tracking issue; revision 3 records the owner's approval; revision 2 records the owner's decisions on Q1–Q4; revision 1: the identity, load-order slot and save-portability contract that [oblivion-actor-values.md](oblivion-actor-values.md) decided in its Q1 but left unspecified, plus the execution sequence for that PRD's phases. Statements about how Oblivion behaves cite external sources (UESP, the Construction Set wiki dump under `/mnt/data/src/reference/`, xEdit's `wbDefinitionsTES4.pas`, the `Oblivion.exe` disassembly); a claim that could not be verified is marked **Phase 0: verify** instead of being stated as fact.

**Tracking**: #7007 (parent PRD), #7021 (Phase 1). Phases 0, 2, 3 and 4 run under #7008, #7009, #7010 and #7011, which carry this PRD's requirements beside the parent's.

**Scope**: how the 72 engine-supplied Oblivion `AVIF` records are named (a stable `FormIdPair` under an engine-owned `PluginId`), keyed at runtime (a load-order slot allocated after the real plugins), installed (once, before any consumer reads them) and persisted (saves store the stable pair, never the session's `u32`). It also sequences the parent PRD's resolution, population and consumer phases so the identity and its save round trip land first. Outside it: what each Oblivion stat value *is* (the parent PRD and CHARAL own the formulas), character creation, Morrowind, the 2024 *Remastered* formulas, and a save migrator chain (the save crate has none; a shape change bumps `FORMAT_MAJOR`).

**Related**: this document states the *promise*; the designs below own the *how*. Every mechanism promised here has an owner in this table, or Phase 0 writes the missing design.

| Mechanism | Owning design |
|---|---|
| What the 72 values are, how they are populated and consumed | [Oblivion Actor Values — PRD](oblivion-actor-values.md) |
| The per-game ruleset, derived stats and the population boundary | [CHARAL](../engine/charal.md) |
| Oblivion's formulas | [CHARAL — Oblivion ruleset](../engine/charal-oblivion-ruleset.md) |
| The save container, registry and format-version rule | [Save and load](../engine/save-load-roundtrip.md) |
| Detection and sneak-attack consumers | [Sneak and Detection](../engine/sneak-detection.md) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119 meanings, as in the related designs. Requirement IDs use the `OAI-` prefix.

---

## 1. Product thesis

The parent PRD gives Oblivion's actors their stats by supplying the `AVIF` records Oblivion's master never authored, so no downstream system knows it is running Oblivion. That works only if the supplied records are first-class: collision-free with every plugin a player can load, and still correct after a save is loaded under a different load order.

Every actor-value consumer in the engine keys on a global load-order `u32` (`ActorValues`, `EsmIndex`, conditions, ObScript, magic effects). A global `u32` is meaningless once the load order changes (`crates/scripting/src/load_order.rs`, module doc). Fallout 3, New Vegas and Skyrim get away with it because their authored `AVIF` records live in the master, which is always slot 0. Oblivion's supplied records have no master, so they need a slot of their own, and the save file must not bake that slot in.

**Corollary**: the *durable* identity (a `FormIdPair`) and the *runtime* key (a `u32` slot-composed id) are two different things, and the load-order identity bridge converts between them at exactly two places: when the records are installed and when a save is written or read. No consumer ever sees the pair, and no file ever holds the slot. The engine never reserves a fixed numeric range, never lets a supplied record share a slot with a plugin, and never attaches a saved value to a record it cannot name.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Engine plugin identity** | The one `PluginId` the engine owns for records no plugin authors. Derived in its own UUID v5 namespace so no plugin filename can produce it. |
| **Durable identity** | The `FormIdPair { plugin, local }` of a record; stable across sessions and load orders. |
| **Runtime key** | The global `u32` a record has in this session: its slot composed with its local id. What `ActorValues` and every consumer key on. |
| **Engine slot** | The unused `Regular` load-order slot the loader allocates for the engine plugin identity after the real plugins have theirs. |
| **Source table** | The one `const` table of `(index, EditorID name)` rows for Oblivion's 72 actor values. Every resolution path reads it; nothing restates it. |
| **Vanilla resolution** | Resolution by index or by name from a `CTDA` operand, a magic effect or an ObScript argument, as the original executable accepts them. |
| **Parent PRD** | [oblivion-actor-values.md](oblivion-actor-values.md); its `OAV-` requirements are not restated here. |

---

## 3. Core model

### 3.1 Identity

Each of the 72 records has `FormIdPair { plugin: ENGINE_PLUGIN_ID, local: index + 1 }`. The `+ 1` exists because `LoadOrderIdentity::global_form_id` rejects local id 0. The runtime key is `engine_slot.compose(local)`, so index 0 (Strength) is local 1 and index 71 is local 72, well inside a regular slot's 24-bit object space.

### 3.2 Install order

1. Assemble the complete load order and give every real plugin its slot.
2. Allocate the engine slot, add `(engine_slot, ENGINE_PLUGIN_ID)` to `LoadOrderIdentity`.
3. Install the 72 records into `EsmIndex` (idempotent; a duplicate identity or slot is an error).
4. Only then build rulesets, script name tables, spells and actors.

### 3.3 Persistence

A saved `ActorValues` key is the stable pair. On restore the pair goes through `LoadOrderIdentity::global_form_id` against the *current* load order. A pair whose plugin is not loaded produces a named diagnostic and the value is dropped, never attached to another record. The entity remap in `crates/save/src/driver.rs` (`build_form_id_remap`) maps entities only and does not cover these keys.

### 3.4 Persisted actor-value references at HEAD on 2026-11-15

| # | Where | Today |
|---|---|---|
| 1 | `ActorValues` map keys (`crates/core/src/ecs/components/actor_values.rs`) | `HashMap<u32, ActorValue>`; the global `u32` is serialized as is |
| 2 | `ActorVitals.health` (same file) | A resolved Health `u32`, stamped at spawn and registered for full restore. A live reload re-derives an NPC's, but the player body is stamped once and outlives it, so it is a live-overlay column too (SAVE-1, rev 5) |
| 3 | Recover-effect `actor_value` (`crates/core/src/ecs/components/restoration.rs`) | `u32` in a saved component |
| 4 | Active magic effect `actor_value` (`crates/scripting/src/magic.rs`, `magic_effects.rs`) | `u32` in a saved component |
| 5 | `SkillProgress` (`crates/scripting/src/skills.rs`) | Saved resource keyed by AVIF `u32` |
| 6 | `PersistentReferenceStates` (`byroredux/src/cell_loader/reference_state.rs`) | Saved resource nests parked `ActorValues` and `ActiveMagicEffects`; unresolved snapshot-column parking also decodes both |

The Phase 0 audit and exemptions are recorded in CHARAL §9. All six rows
require portable encoding on save, full restore and, where applicable, live
overlay and unresolved-entity parking (OAI-SAVE-002).
The subsequently added `ActorValues` view aliases also contain AVIF keys:
the alias, current source and base source each use a stable pair on disk.

---

## 4. Goals and non-goals

### 4.1 Goals

- **OAI-G-001 — One identity per record.** All 72 records carry the engine pair and resolve both ways through the identity bridge.
- **OAI-G-002 — No collision, ever.** No load order, however full, gives a supplied record a plugin's id; one that cannot fit fails loudly.
- **OAI-G-003 — Saves survive a load-order change.** A save round-trips with identical actor values after a plugin is added or removed.
- **OAI-G-004 — Every persisted AVIF reference is portable.** The count of save-registered AVIF `u32` fields not stored as pairs (and not exempted with a reason) is zero.

### 4.2 Non-goals

- **OAI-NG-001.** A fixed reserved `u32` range: the parent PRD's Q1 rejects it, because it collides with mod load orders.
- **OAI-NG-002.** A save migrator chain: a shape change bumps `FORMAT_MAJOR` (the rule in `crates/save/src/snapshot.rs`).
- **OAI-NG-003.** OBSE-only actor functions that accept Fame and Infamy; they get their own access path in their own PRD.
- **OAI-NG-004.** Changing the actor-value model of Fallout 3, New Vegas or Skyrim.

---

## 5. Requirements by area

### 5.1 Identity (`OAI-ID`)

- **OAI-ID-001.** The engine plugin identity MUST be a `PluginId` derived in a UUID v5 namespace distinct from the filename namespace in `crates/core/src/form_id.rs`, and a test MUST assert that no filename-derived id equals it for the vanilla Oblivion, Fallout 3, New Vegas and Skyrim master names.
- **OAI-ID-002.** Loading MUST fail with an error naming the engine identity when any loaded plugin's identity equals it.
- **OAI-ID-003.** Each of the 72 records MUST be `FormIdPair { ENGINE_PLUGIN_ID, index + 1 }`; a test MUST round-trip all 72 through `pair` and `global_form_id`.
- **OAI-ID-004.** The engine slot MUST be the lowest unused `Regular` slot after every real plugin is slotted, so it is deterministic for a given load order (Q1).
- **OAI-ID-005.** When no `Regular` slot is free, loading MUST fail with an error naming the engine records and the slot count, and MUST NOT reuse a plugin's slot; a test with a synthetic full table MUST assert it.
- **OAI-ID-006.** `LoadOrderIdentity` MUST contain the engine entry before any consumer runs, so `pair` and `global_form_id` convert it like any plugin's.

### 5.2 Records and resolution (`OAI-RES`)

- **OAI-RES-001.** Installation MUST run once after the complete load order is assembled and before rulesets, script names, spells and actors are built; a test MUST assert the order.
- **OAI-RES-002.** Installation MUST be idempotent and MUST reject a duplicate identity or a duplicate slot with an error.
- **OAI-RES-003.** The source table MUST be the only place the 72 `(index, name)` rows are written; `EsmIndex::resolve_actor_value`, `condition_actor_value_form` and `obscript_actor_values::actor_value_form` MUST read it. A text-scan guard MUST fail if a second copy appears.
- **OAI-RES-004.** The engine MUST install 72 records. Vanilla index resolution MUST return a key for 0–37 and 40–71 (70 values), `None` for 38 and 39, and `None` above 71. Vanilla name resolution MUST reject `Fame` and `Infamy`. A test MUST assert all of it. This reconciles the parent PRD's "72" with its "70" (index coverage ÷ 70): 72 records exist, 70 are vanilla-resolvable.
- **OAI-RES-005.** Fame and Infamy MUST be reachable by a named non-vanilla lookup so a future dedicated command does not need a second table; a test MUST assert it returns the installed records.

### 5.3 Persistence (`OAI-SAVE`)

- **OAI-SAVE-001.** A saved `ActorValues` key MUST be its stable `FormIdPair`; restore MUST resolve it through the current `LoadOrderIdentity`.
- **OAI-SAVE-002.** Every other save-registered field that holds an AVIF `u32` (§3.4 rows 2–5) MUST be stored as a pair too, or be listed in the design with the reason it is exempt. The audit's list MUST be in the PR that lands OAI-SAVE-001.
- **OAI-SAVE-003.** The change MUST bump `FORMAT_MAJOR` from 132 in the same commit, with the version-history entry the other bumps carry, and no saved field gets `#[serde(default)]`.
- **OAI-SAVE-004.** A test MUST save with the engine at slot A, load with a load order that moves it to slot B, and assert every actor value, vital and active effect is identical by identity.
- **OAI-SAVE-005.** A saved pair whose plugin is not loaded MUST produce a diagnostic naming the pair and the entity, MUST drop the value, and MUST NOT attach it to any record; a test MUST assert the other actor values are unaffected.
- **OAI-SAVE-006.** The `SaveRegistry` row-order hash and the path-keyed shape guard MUST be updated in the same commit (the registry hashes its row order).

### 5.4 Parent PRD requirements this PRD sequences

Population (`OAV-POP-001` to `-007`) and consumers (`OAV-USE-001` to `-005`) are as the parent PRD states them. This PRD adds one requirement over them:

- **OAI-SEQ-001.** No Phase 3 or Phase 4 issue below MAY be started before Phase 1's changed-load-order round trip (OAI-SAVE-004) is green.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Actor values are executable-hardcoded indices; no record exists | 72 records under an engine identity (parent PRD) | — | A plugin id collides with it → OAI-ID-001, -002 |
| 2 | No record occupies a load-order slot | The engine takes one `Regular` slot | — | A 253-plugin load order has no slot → OAI-ID-005 |
| 3 | Oblivion saves store nothing by `AVIF` FormID | Redux saves store the pair | The session `u32` in a save | A save read under another load order mis-keys values → OAI-SAVE-001, -004, -005 |
| 4 | Index 38 and 39 are not vanilla-resolvable | Installed, vanilla-resolution rejected | — | An accidental accept → OAI-RES-004 |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | The only game whose records the engine supplies |
| Fallout 3 | n/a | Authors `AVIF`; the pair codec is identity-preserving for master-slot ids (Q2) |
| Fallout: New Vegas | n/a | Authors `AVIF`; same as Fallout 3 |
| Skyrim (LE / SE / AE) | n/a | Authors `AVIF`; same as Fallout 3 |
| Fallout 4 | n/a | Authors `AVIF`; not an `ActorValues` consumer today |
| Fallout 76 | n/a | Authors `AVIF` |
| Starfield | n/a | Authors `AVIF` |

### 7.2 Acceptance

A scenario passes when:

1. **Identity.** `cargo test -p byroredux-scripting` round-trips all 72 pairs, asserts the engine slot is the lowest unused, and asserts slot exhaustion and a plugin-identity collision both fail with their named errors (OAI-ID-001 to -006).
2. **Resolution.** `cargo test -p byroredux-plugin` resolves 70 indices by number and name, rejects 38, 39 and 72, and asserts the install order (OAI-RES-001 to -005).
3. **Portable save.** `cargo test -p byroredux --bin byroredux save_io` (pinned toolchain) saves with the engine at slot A, loads with it at slot B, and asserts every value identical; a missing-plugin load drops with a diagnostic (OAI-SAVE-001 to -006).
4. **Audit closure.** The PR for scenario 3 lists every save-registered AVIF field and its disposition (OAI-G-004).
5. **Parent scenarios.** The parent PRD's §7.2 scenarios 1–4 pass; scenario 4 (live) additionally reloads a save after adding a plugin and reads the same `GetAV Sneak`.

---

## 8. Metrics

| Metric | Definition | Granularity |
|---|---|---|
| Persisted AVIF fields portable | save-registered AVIF `u32` fields stored as pairs or exempted ÷ all found | per audit |
| Identity coverage | records round-tripping through the bridge ÷ 72 | per phase |
| Save round trips under a changed load order | passing ÷ attempted | per phase |

The numbers are published in ROADMAP's per-game compatibility matrix, Oblivion row.

---

## 9. Delivery sequence

**Phase 0 — Evidence and design (no engine code).** Close the parent PRD's verify items that no local document settles, from the `Oblivion.exe` disassembly: which `ACBS` flag combination selects auto-calc at runtime (OAV-POP-003), whether `GetAV Aggression` returns the `AIDT` byte (OAV-POP-005), the operation order of the health formula and the level-1 values (OAV-POP-003). Already answered from local sources: the `CTDA` operand is a signed 32-bit actor-value index (xEdit `wbInteger('Actor Value', itS32, wbActorValueEnum)`), and `ACBS` bits 4 and 7 are "Auto-calc stats" and "PC Level Offset" (xEdit and UESP agree). Write the identity, install-order and population design into CHARAL and run the §3.4 save audit. Exit: every verify item has a source (a URL, a file and line, or a disassembly address with its trace), or is filed as an observation issue; the §3.4 table has no "not yet audited" row.

**Phase 1 — Identity bridge and portable saves.** OAI-ID-001 to -006 and OAI-SAVE-001 to -006, together, because the parent PRD's biggest risk is a session-specific key that leaks into a save. Exit: §7.2 scenarios 1, 3 and 4 pass.

**Phase 2 — Records and resolution.** OAI-RES-001 to -005, then the parent's OAV-RES-001 to -005. Exit: §7.2 scenario 2 passes; index coverage 70 / 70.

**Phase 3 — Population.** The parent's OAV-POP-001 to -007. Exit: the parent's scenarios 2 and 3 pass; population coverage published.

**Phase 4 — Consumers and disposition.** The parent's OAV-USE-001 to -005. Exit: the parent's scenario 4 passes; consumers on real values 10 / 10.

---

## 10. Decisions required

1. **Q1 — Which slot the engine takes.** Recommended: the lowest unused `Regular` slot after the real plugins, so a load order is deterministic and a mod's FormIDs never move. Alternative: the highest free `Regular` slot (`0xFC` down), which keeps the engine out of the way of a plugin added later but makes the slot shift whenever the top is taken. **Decided** (2026-11-15): the lowest unused `Regular` slot.
2. **Q2 — How far the pair codec reaches.** Recommended: it applies to every game's saved actor-value keys, since a per-game codec is the per-game branching the translation layers forbid; for Fallout 3, New Vegas and Skyrim it is identity-preserving, as their authored records are in the master at slot 0, and for mod-authored `AVIF` records it is a fix. Alternative: Oblivion only, leaving the other games' saves load-order-dependent for mod-authored records. **Decided** (2026-11-15): every game's saved actor-value keys.
3. **Q3 — Old saves.** Recommended: reject every pre-133 save, as the save crate's rule requires, since no migrator exists. Alternative: write the migrator chain first, which is its own PRD. **Decided** (2026-11-15): reject every pre-133 save.
4. **Q4 — Fame and Infamy.** Recommended: install all 72 and reject 38 and 39 in vanilla resolution only (OAI-RES-004, -005). Alternative: install 70 and add the two when a dedicated command lands, which costs a second install path. **Decided** (2026-11-15): install all 72; vanilla resolution rejects 38 and 39.

---

## 11. Document map

| Need | Go to |
|---|---|
| What the values are and how actors get them | [Oblivion Actor Values — PRD](oblivion-actor-values.md) |
| The ruleset, stat models and population boundary | [CHARAL](../engine/charal.md) |
| Oblivion's formulas | [CHARAL — Oblivion ruleset](../engine/charal-oblivion-ruleset.md) |
| The save container and format-version rule | [Save and load](../engine/save-load-roundtrip.md) |
| Detection and sneak-attack consumers | [Sneak and Detection](../engine/sneak-detection.md) |
| The tracking issues | #7007 (parent), #7008 to #7011 (phases) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-15 | First draft: the identity, slot and save contract behind the parent PRD's Q1, the 72-versus-70 reconciliation, and the execution sequence | — |
| 2 | 2026-11-15 | Owner's decisions recorded: Q1 lowest unused `Regular` slot, Q2 the pair codec for every game, Q3 pre-133 saves rejected, Q4 72 installed and 38 / 39 rejected in vanilla resolution | — |
| 3 | 2026-11-15 | PROPOSED → APPROVED | owner: approved |
| 4 | 2026-11-15 | APPROVED → ACTIVE: tracking #7021 (Phase 1) under parent #7007 | — |
| 5 | 2026-11-17 | §3.4 row 2 corrected: `ActorVitals` joins `MUTABLE_DELTA_COLUMNS` because the player's Health key is not re-derived on a live load (audit SAVE-1); OAI-SAVE-004's live-load leg now covers vitals | — |

---

## 13. Execution guide for the implementing agent

This section is the hand-off. It is not part of the PRD standard; it says which tool answers which question, so the work is done once and verified the same way each time.

### 13.1 Read first

1. `CLAUDE.md` (`AGENTS.md` is a symlink to it; never edit `AGENTS.md`).
2. This PRD, then [oblivion-actor-values.md](oblivion-actor-values.md), then `docs/engine/charal.md` §9 "Open research items".
3. `crates/scripting/src/load_order.rs`, `crates/save/src/snapshot.rs` (the `FORMAT_MAJOR` rule), `byroredux/src/save_io/registry.rs`, `crates/plugin/src/esm/records/index.rs::resolve_actor_value`.

Do not edit `docs/engine/charal.md` while another session has it modified: run `git status docs/engine/charal.md` first and ask the owner if it is dirty.

### 13.2 Working rules

- **One issue per milestone slice**, filed under #7007 once this PRD is APPROVED. Commit as `Fix #N: <what and why>`. A commit message that closes several issues must repeat `Fix #X` for each. **No `Co-Authored-By` trailer of any kind.** Each commit pushes; commit one issue at a time and never leave debug code in the tree.
- **No per-game branches in consumers.** The Oblivion difference lives in the plugin and loader boundary and in CHARAL's ruleset. A consumer that gains an `if game == Oblivion` is wrong.
- **Extend a text-scan guard in the same commit** as the refactor that moves the text it scans.
- **Grep `include_str!`** before splitting or moving a file.
- **Do not guess.** If a claim is not in a local source or a disassembly trace, write **Phase 0: verify** and file an issue.

### 13.3 Build and test recipes

```bash
TC=$(rustup which --toolchain 1.100.0 cargo)
PATH="$(dirname "$TC"):$PATH" "$TC" test -p byroredux --bin byroredux save_io    # binary save tests
cargo test -p byroredux-core --features inspect                                   # the feature is load-bearing
cargo test -p byroredux-scripting
cargo test -p byroredux-plugin
cargo test -p byroredux-save
python3 scripts/check-prd-conformance.py                                          # after any PRD or index edit
```

- Run `cargo clippy` without `-D` in a side target dir; `-D` hides the dependents of a red crate.
- Real-data tests are `--ignored` and need `BYROREDUX_REQUIRE_GAME_DATA=1`. The plugin `--ignored` tests can spike past 20 GB: run one test at a time. If a doctest dies with a bus error, `TMPDIR` is on a full tmpfs; point it at `/mnt/data`.
- Rustc 1.100 has no `target/debug/deps`; find test binaries through cargo's JSON output.

### 13.4 Evidence tools (Phase 0)

| Question | Tool |
|---|---|
| A record layout or flag | `rg -l <FOURCC> /mnt/data/src/reference/uesp-wiki` and `/mnt/data/tmp/wbDefinitionsTES4.pas` (xEdit) |
| An ObScript function or Oblivion rule | `/mnt/data/src/reference/cs-uesp-wiki` (CS wiki), with the pages under `(main)/` |
| A condition function's parameter type | `wbDefinitionsTES4.pas`, `ParamType1` of the function row |
| What the original executable does | `Oblivion.exe` disassembly: `python3 -I` with `capstone` (no `pefile` or `r2`); do not name the script `dis.py`; keep it outside the game-data directory; find a record handler by its `cmp eax, <FourCC>`, a console command by its name string then the command-table entry (execute pointer at +0x18), and callers by scanning `.text` for `E8 rel32` |
| Whether a value is in a real master | an `--ignored` test over `Oblivion.esm`; report counts, do not assume |

The `Oblivion.exe` `.text` is readable even though it has a `.bind` section. The `fstart.py` CC-padding heuristic fails on it: scan back for a prologue after `ret` or `int3`, and use capstone `skipdata`. Record every address you cite.

### 13.5 Live verification (owner-run, Phase 4 and the Phase 1 reload check)

The agent must not launch a second engine instance alongside one that is already running. The owner or the Claude session runs:

```bash
BYRO_DEBUG_SERVER=1 cargo run --release -- --esm Oblivion.esm --cell AnvilTheCountsArms \
  --bsa "Oblivion - Meshes.bsa" --textures-bsa "Oblivion - Textures - Compressed.bsa" \
  --bench-frames 300 --bench-hold
cargo run -p byro-dbg        # attaches on port 9876; BYRO_DEBUG_PORT overrides
```

Then, in `byro-dbg`: `obscript.cmd <ref> GetAV Sneak`, `stealth.status`, `hud.status`, and `tex.missing` first if any surface looks chrome. `byro-dbg` shows what the engine does; it is not an oracle for what the original game does, so a behavioural claim still needs a source from §13.4. `docs/smoke-tests/oblivion-actor-values.sh` (the parent's scenario 4) follows the existing smoke-script pattern: it exits `77` without `BYROREDUX_OBLIVION_DATA`.

### 13.6 Who runs what

| Step | Implementing agent | Owner / Claude session |
|---|---|---|
| Code, unit tests, fixtures, doc edits | yes | review |
| Disassembly evidence (Phase 0) | may | verifies each cited address |
| `--ignored` real-data census | yes, if data is present | runs it otherwise |
| Live `byro-dbg` scenario | no | yes |
| Audits (`/audit-save` after Phase 1, `/audit-esm` and `/audit-scripting` after Phase 2, `/audit-character` after Phase 3, `/audit-incremental` at each close) | no | yes |
| PRD state transitions, closing issues | no | yes |

### 13.7 Done, per phase

A phase is done when its §9 `Exit:` measurement is shown (the command and its output), the conformance gate passes if any PRD or index file changed, `cargo test` for every touched crate is green, no Phase 0 verify item remains unmarked, and the ROADMAP Oblivion row is updated by the owner at close.
