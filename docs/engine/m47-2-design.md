# M47.2 — Full Scripting Runtime Design

**Status**: scoping (2026-06-21). Tier 3 milestone. Dependencies all closed:
R5 (`go ECS-native`, 2026-05-16), M30.2 (full `.psc` parse, 2026-05-23),
M47.0 (event-hooks runtime, 2026-05-23), M47.1 (condition eval, 2026-05-23).

**Goal**: run Bethesda script behavior on **unmodified game data**, across
the lineage, by translating each game's compiled scripting format into the
ECS-native component+system shape R5 validated — *not* by interpreting a VM.
"Closes the loop" means a scripted REFR loaded from a vanilla BSA/BA2 does
the thing it does in the shipping game (a quest-door advances a stage, a
pressure plate shakes the camera, a trigger fires), driven entirely by ECS.

**Non-goals** (carried from M47.0, reaffirmed):
- No Papyrus stack-VM. No fibre / suspendable script frames / continuations.
- No general AST→ECS lowering. The transpiler is a **recognizer catalog**
  ("detect the shape, extract the constants, populate the component"), with
  per-script fallthrough — *not* arbitrary-statement code generation. R5's
  evaluation argued against general lowering; this doc commits to that.
- No custom-event broadcast machinery up front. Vanilla content uses it
  essentially nowhere (0 hits across the R5 corpus scan); it lands only if a
  recognized script demands it.

---

## Decisions locked (2026-06-21)

Three forks were resolved with the project owner before scoping:

1. **Frontend / source strategy → full `.pex` decompiler.** Vanilla BSAs
   ship compiled `.pex`, not `.psc`. M47.2 builds a real Papyrus-bytecode
   decompiler (tables **and** control-flow reconstruction) so it reads
   shipping content directly, rather than depending on the often-absent CK
   `Source/` archives.
2. **Transpiler philosophy → scale the recognizer catalog.** Generic
   recognizers for high-frequency script families; per-script recognizers
   for the long tail. This is the existing `translate/` scaffold's shape.
3. **Game scope → Papyrus *and* Obscript.** Skyrim / SSE / FO4 via Papyrus
   (`.psc` + `.pex`); Oblivion / FO3 / FNV via Obscript, parsed from the
   `SCPT.SCTX` source-text sub-record where present.

---

## What's already built (the M47.2 substrate)

M47.2 is **not** a from-scratch transpiler. The recognizer backend and the
whole dispatch runtime already exist and are tested against the four R5
demos. The inventory:

| Piece | Where | State |
|---|---|---|
| Recognizer boundary `translate_script(source, game, script_instance, owning_quest) -> Option<Recognized>` | `crates/scripting/src/translate/mod.rs` | **framework done** |
| `RecognizeCtx` / `Recognized` / `SpawnFn` / `Recognizer` types | `translate/archetype.rs` | done |
| `ScriptSource` frontend enum (`PapyrusSource`, `Obscript`) | `translate/source.rs` | `PapyrusSource` live; `Obscript` is a typed placeholder |
| `CanonicalEvent::from_papyrus` (event-name normalization) | `translate/tables.rs` | done |
| 2 recognizers: `rumble` (per-script), `quest_stage_gate` (generic) | `translate/recognizers/` | done + tested |
| Papyrus AST + parser (M30 / M30.2) | `crates/papyrus/` | done; round-trips all 4 R5 scripts |
| `ScriptRegistry` (editor_id → spawner) | `scripting/src/registry.rs` | done |
| Event markers (Activate/Hit/CellLoad/TriggerEnter/Equip/Update/Timer) | `scripting/src/events.rs` | defined; some emit sites missing |
| Condition eval (M47.1, OR-precedence, `ConditionList`) | `scripting/src/condition.rs` | 7-function catalog, additive |
| `QuestStageState` (current + stages-done history) | `scripting/src/quest_stages.rs` | done |
| `RecurringUpdate` / `OnUpdateEvent` (`RegisterForUpdate`) | `scripting/src/recurring_update.rs` | done |
| 4 hand-translated R5 demos (the target shape) | `scripting/src/papyrus_demo/` | done + tested |
| VMAD decode `ScriptInstanceData::parse` + `ScriptInstance::object_form_id` | `crates/plugin/.../script_instance.rs` | **record-level done**; not yet threaded to attach |

The gaps M47.2 fills: **the two compiled frontends, engine attach-path
integration, recognizer breadth, and the unwired event emit sites.**

---

## Architectural spine

```
                          ┌─ .psc source ───── M30 parser ───────┐
   unmodified game data ──┼─ .pex bytecode ─── decompiler ───────┤─▶ Papyrus `Script` AST ─┐
                          │                                       │                          │
                          └─ SCPT.SCTX text ── Obscript parser ───┴─▶ Obscript AST ──────────┤
                                                                                              ▼
                                                          translate_script() — recognizer chain
                                                          (match shape, extract constants)
                                                                              │
                                                          + VMAD ScriptInstanceData (per-instance params)
                                                          + owning_quest (alias ownership)
                                                                              ▼
                                                          Recognized.spawn(world, entity)
                                                          → ECS component(s) + existing dispatch systems
```

Two languages, three on-disk forms, **one recognizer backend**. Per-game
variance is resolved once, at the boundary, behind `translate/tables`.

### Frontend → AST normalization (key refinement)

Because the decompiler is *full* (not table-only), **`.pex` lowers to the
same `byroredux_papyrus::ast::Script` the `.psc` parser produces.** A
compiled script is therefore just another `ScriptSource::PapyrusSource` —
**no new enum variant, and every Papyrus recognizer works on source and
compiled content identically.** This is the payoff of choosing the full
decompiler over a parallel "compiled" code path. (The `source.rs` comment
anticipating a separate `PapyrusCompiled` variant is superseded by this
decision; the variant is not added.)

Obscript is a genuinely different language and cannot masquerade as a
Papyrus `Script`. It keeps `ScriptSource::Obscript`, which evolves from
"raw `ScriptRecord`" to "parsed Obscript AST" once the SCTX parser lands.
Obscript recognizers form a parallel family; recognizers already
pattern-match `ctx.source`, so the signature is unchanged.

### Why recognizers consume the AST, not a new IR

Recognizers match the existing `Script` AST directly. A neutral cross-
language IR was considered and **deferred**: Papyrus is the overwhelming
majority of scripted content, the decompiler unifies both Papyrus forms
onto `Script` for free, and Obscript's recognizer set is small. We promote
to a shared IR only if Papyrus/Obscript recognizer duplication becomes real
— a measured decision in Phase 5, not an upfront abstraction.

---

## The recognizer contract (unchanged from the R5 shape)

A recognizer is `fn(&RecognizeCtx) -> Option<Recognized>`. It inspects the
source plus per-instance binding context and either declines or returns a
boxed spawn closure that inserts canonical components.

```rust
pub struct RecognizeCtx<'a> {
    pub source: &'a ScriptSource<'a>,           // Papyrus Script (source or decompiled) | Obscript
    pub game: GameKind,                         // for table-driven, never ad-hoc, per-game shape
    pub script_instance: Option<&'a ScriptInstanceData>, // VMAD per-instance properties
    pub owning_quest: Option<u32>,              // alias→quest ownership (Self.GetOwningQuest())
}

pub struct Recognized {
    pub archetype: String,                      // "quest_stage_gate@DA10MainDoorScript"
    pub spawn: SpawnFn,                         // Box<dyn Fn(&mut World, EntityId) + Send + Sync>
}
```

The chain in `translate/mod.rs` runs **per-script recognizers first**
(so a bespoke script isn't swallowed by a family match), generic families
second. A script no recognizer claims returns `None` — a silent miss the
caller treats exactly like an M47.0 registry miss ("no consumer yet").

**The split rule (R5's load-bearing finding), restated for the transpiler:**
a Papyrus event handler with a latent `Utility.Wait()` becomes *two*
systems — the code before the wait runs on the event-driven system; the
code after runs on a dt-driven tick system that decrements a
`wait_remaining_secs: f32` field carried inside a state-enum variant. A
recognizer that matches a wait-bearing family must emit that split. The
`rumble` demo is the reference implementation.

---

## Frontends in detail

### A. Papyrus `.psc` source — SHIPPED

`ScriptSource::PapyrusSource(&Script)` via `byroredux_papyrus::parse_script`
(M30.2). Serves mod source and the CK `Source/` folder at runtime; the
recognizer-validation fixtures (R5 corpus) are `.psc`. No new work — this is
the frontend Phases 0/3 develop the catalog against.

### B. Papyrus `.pex` compiled — NEW (heavy lift #1)

The vanilla-runtime format. A `.pex` decode is two layers:

**B1 — Container / table reader.** Parse the `.pex` structure: magic +
version header, string table, debug info, user-flags, and the object table
(parent, auto-state, variable table, **property table**, **state table** →
function entries with signatures). Stop before instruction decoding. This
alone yields a `Script` with populated *properties, states, and function
signatures* but empty bodies — enough for any recognizer keyed on **name +
property set + state names** (params then sourced from VMAD).

**B2 — Bytecode control-flow reconstruction.** Decode the per-function
Papyrus Assembly instruction stream (the stack/register opcodes:
`ASSIGN`, `CALLMETHOD`/`CALLSTATIC`, `JUMP`/`JUMPT`/`JUMPF`, `CMP_*`,
arithmetic, `RETURN`, …) and lift it back to structured `Stmt`/`Expr`:
recover `If`/`ElseIf`/`While` from the jump graph, method calls from
`CALLMETHOD`, etc. Output is body statements on the Phase-B1 `Script`, so
recognizers that must read statement bodies (e.g. the exact `SetStage(Z)`
target, or predicates not held in VMAD) work on compiled content.

Decompilation targets the existing AST faithfully enough that a recognizer
cannot tell source from compiled. Fidelity is validated by running the same
recognizer over the `.psc` fixture and the decompiled `.pex` of the same
script and asserting identical `Recognized` output (byte-for-byte component
equality — the pattern `quest_advance` tests already use).

Format references are well-documented (see References); **no opcode
semantics are guessed** — they come from the UESP PEX spec + the Champollion
reference decompiler.

### C. Obscript via `SCTX` — NEW (heavy lift #2, pre-Papyrus games)

Oblivion / FO3 / FNV `SCPT` records carry compiled `SCDA` bytecode **and**,
in the vast majority of vanilla records, the original `SCTX` source text
(already parsed and retained verbatim on `ScriptRecord`). M47.2 parses
`SCTX` — a small Obscript lexer + parser (distinct grammar: `scn` header,
`Begin <blocktype> … End` blocks, `set X to Y`, `if/elseif/endif`, a
different function namespace). The result is an Obscript AST carried by
`ScriptSource::Obscript`, matched by a parallel Obscript recognizer family.

`SCDA`-bytecode disassembly is the fallback for the rare `SCTX`-absent
record and is **deferred** within M47.2 (filed, not built) — SCTX coverage
is high enough that the text path unblocks the dormant pre-Skyrim scripts
(e.g. the ~1257 FO3 SCPTs) without it.

---

## Engine integration (the attach path)

Today scripts attach via M47.0's static chain (`base_record.script_form_id
→ index.scripts → ScriptRegistry → ScriptSpawnFn`) populated by a hardcoded
`papyrus_demo::register_spawners`. M47.2 makes attachment **dynamic and
recognizer-driven**, in the cell loader's per-REFR walk
(`cell_loader/references.rs`, after entity spawn):

```
for each spawned REFR:
  base = index.base_record(placed_ref.base_form_id)
  // Papyrus (Skyrim+): VMAD on the base record (or the REFR's own VMAD)
  // Obscript (≤FNV):    base.script_form_id → index.scripts[..] (SCPT)
  source           = frontend_for(game, base)        // Script (psc|pex) | Obscript
  script_instance  = base.script_instance.as_ref()   // VMAD ScriptInstanceData (already decoded)
  owning_quest     = alias_owner_of(placed_ref)      // for ReferenceAlias-attached scripts
  if let Some(rec) = translate_script(&source, game, script_instance, owning_quest):
      (rec.spawn)(world, refr_entity)                // inserts canonical components
      log "attached {rec.archetype} to {form_id:08X}"
```

VMAD decode exists at the record level (`ScriptInstanceData::parse`); the
integration work is **threading `script_instance` and `owning_quest` into
`RecognizeCtx` at attach time** and resolving which frontend a given
game+record uses. The hardcoded demo registration is retired in favor of
this path (the demos remain as recognizer-test fixtures).

---

## Save/load synergy (M45, landed 2026-06-21)

Script-state components (`RumbleOnActivate`, `QuestAdvanceOnActivate`,
state enums with `wait_remaining_secs`, `RecurringUpdate`, …) **are** the
"post-spawn mutable game state" M45's delta-apply path replays by stable
`FormIdPair`. Every recognizer-emitted component type must register in the
`SaveRegistry` (and appear in the binary's `MUTABLE_DELTA_COLUMNS`). The
`QuestStageState` resource likewise needs a save column. This is designed in
from Phase 0 — a generated component that doesn't round-trip a save is a
silent state-loss bug, not a follow-up.

---

## Phased plan

Sequenced to de-risk: integration first (cheap, proves the spine on data we
can already parse), then the two compiled frontends in increasing-cost
order, with catalog scaling and event sites interleaved.

### Phase 0 — Dynamic attach integration (frontend-independent)
Wire `translate_script` into the cell-loader REFR-attach path; thread VMAD
`script_instance` + `owning_quest` into `RecognizeCtx`; retire the hardcoded
demo registration; register the emitted component types + `QuestStageState`
with the M45 `SaveRegistry`.
**Deliverable:** the 2 existing recognizers fire on real REFRs, driven by
`.psc` fixtures; a synthetic E2E (`.psc` ACTI with VMAD → component lands →
event fires → side-effect marker appears) is green; script state survives
save/load. *Cost: small.*

### Phase 1 — `.pex` table reader (B1)
`.pex` container + table decode → partial `Script` (properties/states/
signatures, empty bodies). Add the `.pex`→`Script` entry point; classify
real compiled vanilla scripts by name + property set; params from VMAD.
**Deliverable:** metadata-classifiable compiled scripts attach from a real
Skyrim/SSE/FO4 BSA. *Cost: small–medium.*

### Phase 2 — `.pex` bytecode decompiler (B2)
Papyrus-Assembly instruction decode + control-flow reconstruction → full
`Script` bodies. Fidelity gate: recognizer output identical for `.psc` vs
decompiled `.pex` of the same script.
**Deliverable:** full vanilla Papyrus content decompiles; body-reading
recognizers operate on compiled scripts. *Cost: large (risk center #1).*

### Phase 3 — Recognizer catalog scaling
Measure script-family frequency across the extracted `.pex` corpus
(`crates/bsa/examples/r5_extract_pex_ba2.rs` already pulls them); add generic
recognizers for the top families by occurrence; per-script fallthrough for
the tail; expand M47.1 condition functions (GetIsID, GetFactionRank,
HasPerk, …) as recognizers demand them.
**Deliverable:** a measured % of vanilla scripted REFRs get working ECS
behavior. *Cost: incremental / ongoing.*

### Phase 4 — Event emit sites
Wire the defined-but-unemitted markers as recognized scripts require them:
`OnEquipEvent` (M41 equip pipeline), `OnTriggerEnterEvent` (Rapier sensor
volumes), `HitEvent` (combat). `sys.accesses` stays 0-conflict.
**Deliverable:** scripts gated on those events actually fire. *Cost: medium
(touches 3 subsystems).*

### Phase 5 — Obscript via `SCTX` (Oblivion / FO3 / FNV)
Obscript lexer + parser over `SCTX` text → Obscript AST + a parallel
recognizer family. `SCDA` disassembly deferred (filed).
**Deliverable:** pre-Papyrus scripted content gets behaviors. *Cost: large
(risk center #2).*

---

## Risks & mitigations

- **Decompiler correctness (Phase 2).** Mitigation: the `.psc`-vs-`.pex`
  identical-output fidelity gate turns "is the decompiler right" into a
  concrete, automatable assertion against the R5 fixtures; Phase 1's
  metadata path delivers value even if B2 stalls.
- **Obscript grammar surface (Phase 5).** Mitigation: parse `SCTX` text
  (a documented, human-readable grammar) before touching `SCDA` bytecode;
  scope the recognizer family to the high-frequency pre-Skyrim patterns.
- **Recognizer coverage plateau.** Mitigation: drive the catalog by measured
  corpus frequency, not guesswork; `None` is always a safe silent miss, so
  partial coverage degrades gracefully (unrecognized scripts are inert, not
  broken).
- **Save bloat / state loss.** Mitigation: the Phase-0 save-registry
  requirement; CI-style assertion that every recognizer-emitted component
  has a save column.

---

## Verification checklist for "M47.2 done" (per phase)

**Phase 0**
- [ ] `translate_script` is called from the cell-loader REFR-attach path
- [ ] `RecognizeCtx.script_instance` carries decoded VMAD at attach time
- [ ] `RecognizeCtx.owning_quest` resolves for alias-attached scripts
- [ ] hardcoded `papyrus_demo::register_spawners` retired from the attach path
- [ ] emitted components + `QuestStageState` registered with the M45 `SaveRegistry`
- [ ] synthetic E2E: `.psc` ACTI w/ VMAD → component lands → event → side-effect marker on player
- [ ] script state survives a `save`→`load` round-trip

**Phase 1**
- [ ] `.pex` container + tables decode to a partial `Script`
- [ ] a real compiled vanilla script attaches via name+property classification
- [ ] decode rejects malformed `.pex` (bad magic / version / truncation) cleanly

**Phase 2**
- [ ] Papyrus-Assembly opcodes decode; jump graph lifts to If/ElseIf/While
- [ ] fidelity gate: recognizer output identical for `.psc` vs decompiled `.pex`
- [ ] `quest_stage_gate` + `rumble` fire on decompiled vanilla content

**Phase 3**
- [ ] top-N script families by corpus frequency have generic recognizers
- [ ] condition-function catalog expanded to cover recognized scripts' CTDAs
- [ ] coverage metric (recognized / total scripted REFRs) reported on a real cell

**Phase 4**
- [ ] `OnEquipEvent` / `OnTriggerEnterEvent` / `HitEvent` have real emit sites
- [ ] `sys.accesses` reports 0 unknown / 0 conflicts after additions

**Phase 5**
- [ ] Obscript `SCTX` parses to an Obscript AST
- [ ] an Obscript recognizer family fires on FO3/FNV scripted REFRs

Always confirm milestone state against [ROADMAP.md](../../ROADMAP.md).

---

## References

External — Papyrus language + compiled format (**no opcode/format guessing**):
- [PEX File Format](https://en.uesp.net/wiki/Skyrim_Mod:Compiled_Script_File_Format) (UESP — `.pex` container + table layout)
- [Papyrus Assembly](https://en.uesp.net/wiki/Skyrim_Mod:Papyrus_Assembly) (UESP — `.pas` opcode reference)
- [Champollion](https://github.com/Orvid/Champollion) (reference `.pex` → `.psc` decompiler impl)
- [Papyrus Category](https://falloutck.uesp.net/wiki/Category:Papyrus) · [Events Reference](https://falloutck.uesp.net/wiki/Events_Reference) · [Function Reference](https://falloutck.uesp.net/wiki/Function_Reference)
- [Script File Structure](https://falloutck.uesp.net/wiki/Script_File_Structure) (`.psc` grammar)

External — Obscript (pre-Papyrus):
- [Oblivion Mod:Script File Format](https://en.uesp.net/wiki/Oblivion_Mod:Script_File_Format) (`SCPT` / `SCDA` / `SCTX`)
- [Oblivion Mod:Script Functions](https://en.uesp.net/wiki/Oblivion_Mod:Script_Functions) · [Differences from Previous Scripting](https://falloutck.uesp.net/wiki/Differences_from_Previous_Scripting) (ObScript → Papyrus)

Internal:
- [`docs/r5-evaluation.md`](../r5-evaluation.md) + [`docs/r5/source/`](../r5/source/) — R5 verdict + reference `.psc` fixtures
- [`docs/engine/m47-0-design.md`](m47-0-design.md) — event-hooks runtime (the attach chain M47.2 extends)
- [`docs/engine/scripting.md`](scripting.md) — full scripting architecture + the 136-event ECS mapping
- [`docs/engine/papyrus-parser.md`](papyrus-parser.md) — M30 parser + AST → ECS transpilation target
- [`docs/legacy/papyrus-api-reference.md`](../legacy/papyrus-api-reference.md) — full Papyrus API surface
- Code: [`crates/scripting/src/translate/`](../../crates/scripting/src/translate/) (recognizer backend), [`crates/papyrus/`](../../crates/papyrus/) (AST + parser), [`crates/plugin/src/esm/records/script_instance.rs`](../../crates/plugin/src/esm/records/script_instance.rs) (VMAD), [`crates/bsa/examples/r5_extract_pex_ba2.rs`](../../crates/bsa/examples/r5_extract_pex_ba2.rs) (`.pex` corpus extractor)
