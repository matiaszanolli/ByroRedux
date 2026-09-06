---
description: "Deep audit of the M30/M47 scripting domain — .pex decompiler (Champollion port), .psc Papyrus parser, AST→ECS recognizer chain, ECS scripting runtime, and the cell-loader attach path"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Scripting Subsystem Audit (M30 / M47.0 / M47.1 / M47.2)

Audit the three scripting crates plus their engine-side wiring for
correctness across the full compiled-Papyrus pipeline: untrusted `.pex`
bytecode decode (`crates/pex/`), the 5-phase decompiler that lifts that
bytecode back to the shared Papyrus AST, the `.psc` source parser
(`crates/papyrus/`), the AST→ECS recognizer chain whose load-bearing
invariant is *decline-on-any-unmodeled-term* (`crates/scripting/src/translate/`),
the ECS scripting runtime (events / timers / conditions / triggers / quest
stages), and the cell-loader REFR-attach path that resolves a scripted REFR's
VMAD-named `.pex` and runs the recognizer chain.

This domain (`pex`+`papyrus`+`scripting` alone are ~52k LOC combined as of
this sync — up from ~16k pre-2026-08-31; see the SDK/extender coverage-gap
note below for where almost all of that growth went) has a growing set of
prior audit passes in `docs/audits/AUDIT_SCRIPTING_*.md` (16 as of this sync
— don't hardcode a count here, it rots every cycle like the filename) — read
the most recent one first (Phase 1 below). The
decompiler is the **highest bug-density area**: it parses untrusted bytecode
and runs five tree-rewriting passes, so dimensions are weighted toward it
(three of seven, though **this weighting predates the 2026-08-31→09-02
SDK/extender-compatibility growth and no longer reflects where most of the
domain's LOC or newest untested surface actually is** — see the coverage-gap
note below). Its correctness story rests on a corpus-decompile
smoke harness and the `.psc`-vs-`.pex` fidelity gate — point findings there,
not at speculation.

**Coverage gap — SDK / extender-compatibility subsystem is NOT covered by any
dimension below (flag, don't silently skip)**: between 2026-08-31 and
2026-09-02 (commits `316fb202`..`fed3e550`, ~100 commits, all after this
skill's last full sync at `64f64480`/2026-08-30) a large SKSE/JContainers/
StorageUtil/ObScript compatibility layer landed —
`crates/scripting/src/papyrus_provider/` (6.2k LOC, new — typed provider
call lowering/dispatch; split out of a single file by #3852), `crates/scripting/src/obscript.rs` +
`crates/scripting/src/obscript_runtime.rs` (686 + 1.6k LOC, new — legacy
Oblivion/FO3/FNV `SCTX`/bytecode-derived branch execution; note this
predates and is **distinct** from the not-yet-built `.psc`-side Obscript/SCTX
*frontend* the "Future-phase gaps" section below still correctly calls out),
`crates/scripting/src/compatibility.rs` (984 LOC, new — extender-call
preflight/aggregation), plus a new `crates/sdk` crate (~14k LOC) exposing
canonical engine state (actor values, equipment, plugin/load-order metadata,
faction relationships, form lookups, UI/menu state, input mappings) to
provider calls. None of this is in the **Crates** or **Engine-side wiring**
lists below, no dimension's entry-points/checklist mentions it, and the most
recent report (`docs/audits/AUDIT_SCRIPTING_2026-08-30.md`) predates all of
it. This is a real, large, currently-unaudited surface — untrusted-input
handling (does the ObScript bytecode-derived path apply the same
bounds-checked-read discipline Dim 1 requires of `.pex`?), the
decline-on-unmodeled invariant (does a provider call that can't be typed
decline cleanly?), and lock ordering (does `crates/sdk`'s state exposure
introduce a new resource-lock nesting order against `QuestStageState`/
`Inventory`/etc.?) are all open questions. Treat it as a scoping gap to
report, not as something to audit ad hoc under an existing dimension's
checklist — it needs its own pass designed with the same rigor as Dimension 8
was for `hkx`, not squeezed into Dims 5/6/7 in passing.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology,
deduplication, context rules, and finding format. See
`.claude/commands/_audit-severity.md` for the severity scale. Do NOT duplicate
those here.

## Scope

**Crates** (crate-roster sanity check in `_audit-common.md`; `pex` is the newest owned by this audit):
- `crates/pex/src/` — `.pex` reader + 5-phase decompiler. Files: `crates/pex/src/opcode.rs`,
  `crates/pex/src/reader.rs`, `crates/pex/src/model.rs`, `crates/pex/src/lib.rs`,
  and `crates/pex/src/decompile/` (`mod`, `cfg`, `lift`, `control_flow`, `boolean`,
  `lower`, `node`, `event_names`).
- `crates/papyrus/src/` — `.psc` lexer (logos) + Pratt parser → AST. Files:
  `crates/papyrus/src/token.rs`, `crates/papyrus/src/lexer.rs`, `crates/papyrus/src/ast.rs`,
  `crates/papyrus/src/span.rs`, `crates/papyrus/src/error.rs`, `crates/papyrus/src/lib.rs`,
  and `crates/papyrus/src/parser/` (`mod`, `expr`, `stmt`, `script`).
- `crates/scripting/src/` — ECS-native runtime + recognizer chain. Runtime:
  `crates/scripting/src/events.rs`, `crates/scripting/src/timer.rs`,
  `crates/scripting/src/cleanup.rs`, `crates/scripting/src/condition.rs`,
  `crates/scripting/src/trigger.rs`, `crates/scripting/src/quest_stages.rs`,
  `crates/scripting/src/fragment.rs`, `crates/scripting/src/recurring_update.rs`,
  `crates/scripting/src/registry.rs`, `crates/scripting/src/scene.rs` (added
  to this inventory 2026-08-07 — SCEN-record playback runtime that has grown
  into the M47.3 quest-alias substrate: `SceneRegistry`/`ScenePlayer` own
  scene action sequencing, `SceneActorBindings` is the live
  `(QuestFormId, AliasId) -> EntityId` alias-fill table `fragment.rs`'s
  `resolve_object` and `condition.rs`'s `RunOn::QuestAlias` both resolve
  through, and `apply_alias_injections`/`QuestAliasInjectionState` apply
  alias-injected factions/inventory onto `FactionRanks`/`Inventory` — see
  Dim 5/6; `scene.rs` is now a thin re-export over
  `crates/scripting/src/scene/playback.rs` + `crates/scripting/src/scene/quest_alias.rs`),
  `crates/scripting/src/lib.rs`.
  **Runtime modules added to this inventory 2026-08-13** — none of them existed
  when Dims 1–7 were written, so route findings deliberately rather than
  assuming a dimension already covers them:
  `crates/scripting/src/package.rs` (the largest of the new set — AI-package
  script surface, cross-reference `/audit-fnv` Dim 9 for the procedure runtimes),
  `crates/scripting/src/cinematic.rs` (the M47.2 MQ101 scripted-camera slice —
  see Dimension 8), `crates/scripting/src/dialogue.rs`,
  `crates/scripting/src/vm_state.rs` (the Papyrus-visible state machine
  `condition.rs` and `fragment.rs` both read),
  `crates/scripting/src/player_control.rs` (disable/enable player controls —
  the gate `byroredux/src/systems/character.rs` honours, cross-reference
  `/audit-physics` Dim 5), `crates/scripting/src/globals.rs`,
  `crates/scripting/src/equipment.rs`. Recognizer
  chain: `crates/scripting/src/translate/` (`mod`, `source`, `archetype`, `compose`,
  `effects`, `tables`, `recognizers/{mod, quest_stage_gate, rumble,
  two_state_activator}` — `two_state_activator` landed after this file's last
  refresh; it recognizes `default2StateActivator` the same per-script way
  `rumble` does). Reference scripts: `crates/scripting/src/papyrus_demo/`.

**Engine-side wiring** (Dimension 7 — outside the crates):
- `byroredux/src/cell_loader/references/attach.rs` (split out of `mod.rs`,
  #1877; `mod.rs` re-exports them and keeps their call sites) —
  `attach_vmad_scripts` / `attach_script_for_refr` call
  `byroredux_scripting::translate_pex`; the `trigger_volume_from_primitive`
  builder spawns invisible `TriggerVolume` REFRs from `XPRM` primitives.
- `crates/plugin/src/esm/records/index.rs` — `base_record_script_instance`
  accessor (VMAD retained on ACTI/CONT/NPC/CREA base records, plus the item
  family per #2189 and the statics/terminal families per #2663 — see Dim 7).
- `crates/plugin/src/esm/records/script_instance.rs` — `ScriptInstanceData` /
  `ScriptInstance` (decoded VMAD).
- `byroredux/src/asset_provider/script.rs` — `build_script_provider` parses the
  repeatable `--scripts-bsa` flag; `extract_pex` resolves a VMAD script name
  to `.pex` bytes.

**Ground truth — read these before auditing**:
- `docs/engine/scripting.md` — the 50KB authoritative model (ECS-native VM
  replacement, recognizer-chain design, 136-event ECS mapping).
- `docs/engine/papyrus-parser.md` — M30 `.psc` parser + AST.
- `docs/engine/m47-0-design.md` — event-hooks runtime (the attach chain M47.2 extends).
- `docs/engine/m47-2-design.md` — the `.pex` decompiler + recognizer-chain spec,
  the `.psc`-vs-`.pex` fidelity gate, "no opcode semantics guessed" rule.
- `docs/engine/m47-2-recognizer-scaling.md` — corpus characterization
  (26,641 `.pex`; handler vs fragment populations; decline-the-tail thesis);
  its "Shipped (2026-07-21)" section documents the `AddItem`/`MoveTo` object-
  targeting effects and the real-corpus ~0%-yield finding — read before
  flagging anything about those two effects.
- `docs/engine/m47-3-quest-alias-design.md` — QUST alias (`ALST`/`ALLS`)
  decode + the alias-fill/injection runtime, **Phases 0–3 shipped
  2026-08-07** (commit `a844c26b`). The decode itself
  (`crates/plugin/src/esm/records/misc/quest.rs`) stays out of this skill's
  crate scope (`/audit-esm` Dim 4 owns it, added 2026-08-13), but the *consumer*
  runtime — `SceneActorBindings`, alias resolution, alias-injected
  factions/inventory — now lives in `crates/scripting/src/scene.rs`, which
  IS in scope (see the crate-scope entry above and Dim 5/6). Read this doc's
  "Remaining subsystem boundary" section before auditing: it names exactly
  which fill types / injected-data families are still bounded follow-ups
  (Created Object spawn, Story Manager event fills, true `LCTN`, reference
  collections, unloaded-world search, and the packages/spells/keywords
  overlay families) — flag those as real gaps if a finding assumes they
  already work, but don't re-file their absence as a new discovery.
- The crate module docstrings: `crates/pex/src/lib.rs`,
  `crates/pex/src/decompile/mod.rs`, `crates/scripting/src/translate/mod.rs`,
  `crates/scripting/src/fragment.rs`.

**Doc-rot check (re-verify the line numbers each cycle — this doc has grown and
they drift)**: `docs/feature-matrix.md`'s Scripting-section status row (line
174 as of this sync, was 139) correctly reflects the shipped `.pex` recognizer
slice, AND its phase-order parenthetical is now also correct —
`` `.pex` recognizer slice (CFG→lift→short-circuit→control-flow→lower) `` —
the boolean short-circuit pass is listed BEFORE control-flow reconstruction,
matching the actual pipeline (Dim 3, confirmed in `decompile_body`/`lower.rs`).
**Both the "shipped slice" framing and the phase-order parenthetical are fixed;
do not re-flag either.** (The parenthetical fix landed 2026-08-19, `f6555b7b`
— stale mentions of the wrong order only survive in historical
`docs/audits/AUDIT_SCRIPTING_2026-08-{07,12}.md` snapshots, which is expected
and not itself a finding.) The "What Doesn't Work Yet" gaps table is now much
further down (line 308 header, the `Full Papyrus transpiler (M47.2)` row
itself at line 322 as of this sync — don't hardcode either number, re-find
them) and still lists the full transpiler as a Tier 3 gap with the recognizer
slice noted inline — this remains accurate (the recognizer chain is a
targeted slice, not a general transpiler); do not re-flag that framing as
stale either.

**Corpus / fidelity instruments (point findings here, do not re-derive)**:
- `crates/pex/examples/pex_corpus_smoke.rs` — runs `byroredux_pex::parse` +
  `decompile::decompile_script` over every `.pex` in real game archives; the
  **source of the 99.996% (26640/26641) zero-panic decompile claim**. Verify the
  claim by re-reading the harness's success/failure tally logic — confirm it
  actually counts a decompile *panic* / `Err` as a failure (a harness that
  swallows panics would inflate the rate).
- `crates/pex/examples/pex_corpus_shapes.rs` + `docs/r5/corpus-shape-survey.txt` —
  the structural-fingerprint coverage instrument behind the recognizer-scaling doc.
- `crates/bsa/examples/r5_extract_pex_ba2.rs` — the `.pex` corpus extractor.
- `docs/smoke-tests/m47-triggers.sh` — engine-side spawn+attach gate on real
  Skyrim data (`--scripts-bsa`, the `M47.2 scripts:` cell-load summary line).

**Future-phase gaps (do NOT flag as missing unless scope says so)**:
- Obscript / `SCTX` frontend (Oblivion/FO3/FNV) — `ScriptSource::Obscript` is a
  typed placeholder; the SCTX parser is M47.2 Phase 5, not built.
- M47.1 condition resolvers (`GetActorValue`/`GetDistance`/`GetFactionRank`/`GetIsID`/
  `HasPerk`, the Global comparand, and the 6 stub branches from #1316) are
  **no longer stubs** — all 13 catalog functions are fully implemented with
  correct Bethesda safe-default sentinels (#1663–#1668, #1316, all closed
  2026-06-29→07-04; re-verified `AUDIT_SCRIPTING_2026-07-16.md` Dimension 6,
  27 passing unit tests). Re-verification against a live headless cell with
  real CTDA data (not just unit tests) remains outstanding — a gap, not a stub.
- The fragment lowerer (b2) is **no longer a "may be partial" gap — it is
  fully wired and live-data-verified** (2026-07-21). `populate_quest_fragments`
  (`byroredux/src/asset_provider/script.rs`) resolves each scripted quest's
  `QF_` `.pex` from `--scripts-bsa`, decompiles, and registers into
  `QuestStageFragments` at cell load; `quest_fragment_dispatch_system`
  consumes `QuestStageAdvanced` and applies. Verified against real
  `Skyrim.esm`: 845 scripted quests → 5,108 stage bindings → 742 fragments
  fully lowered and registered. Do not re-flag this as unwired.
- **QUST `VMAD` property-table wiring (2026-07-21, same-session fix)**:
  `QuestRef::Property`/the new `ObjectRef::Property` (see Dim 5) used to
  always decline at dispatch because `quest_fragment_dispatch_system` passed
  `vmad: None` unconditionally — `parse_quest_fragments` decoded the QUST's
  own VMAD scripts-section internally (to find the fragment-section offset)
  and then discarded it. Fixed: `QustRecord.script_instance` now retains it,
  `QuestStageFragments::insert_vmad`/`vmad()` store it per-quest, and
  `resolve_quest`/`resolve_object` (Dim 6) receive the real VMAD. Verified
  live: 969 Skyrim quests now have a resolvable property table. A
  `Property`-targeted effect that still declines with a *populated* VMAD on
  hand is a real regression; declining with no VMAD registered (no
  `--scripts-bsa`, or the quest's VMAD carried no scripts section) is correct.
- **Object-targeting fragment effects (`AddItem`/`MoveTo`, 2026-07-21)** — see
  Dim 5/6 for the mechanism. Real, tested, dispatch-wired. The original
  *live-corpus measured at ~0% real yield* finding predates the M47.3
  alias-fill runtime landing (`a844c26b`, 2026-08-07). What changed: an
  object receiver bound to a bare, alias-bound `ObjectReference Property`
  now resolves live through `SceneActorBindings` (Dim 5's `ObjectRef`
  hole-binding bullet) instead of declining, and `object_expr_ref`
  (`effects.rs`) resolves `alias.GetRef()`/`alias.GetActorRef()` method-call
  locals through `Binding::Object` into `scope.object_locals` too (the same
  map §Dim 5's `receiver_object` bullet above describes), so
  `ObjectReference k = SomeAlias.GetActorRef(); k.AddItem(...)` now resolves
  live rather than declining. Verify against the current three-map
  (`quest_locals`/`object_locals`/`decl_locals`) behavior, not any
  "method-call-derived locals still decline" framing.
  **The re-measurement itself is DONE — a settled, asymmetric result, not an
  open question — re-run `fragment_coverage` only if verifying it, don't
  re-litigate it from first principles**: the 2026-08-27 pass re-ran the
  harness over real Skyrim SE + FO4 + Starfield `Misc` archives.
  `AddItem` is now genuinely non-zero (12 emissions Skyrim + 42 FO4/Starfield
  = 54 total). `MoveTo` is **still exactly 0** across all three games — but
  this pass established the zero is *structural*, not incidental: 3,253 of
  3,334 real `.pex` `MoveTo` calls (97.6%) carry the compiler-materialized
  4-arg form `(0.0, 0.0, 0.0, matchRotation)`, which the "conservative-shape
  decline" bullet below correctly rejects (it accepts *only* the literal
  2-arg receiver+destination shape) — every real author's `MoveTo` call hits
  that decline, 100% of the time, because the `.pex` frontend (the only
  production input path) always materializes the omitted default arguments
  that hand-authored `.psc` never writes out. This is tracked as **open**
  issue #3487 ("three effect primitives guard on hand-authored `.psc` arity,
  but the `.pex` frontend materializes every default argument — `MoveTo`
  declines 100% of 3,334 real calls"), with a suggested fix already scoped
  (accept the 4/5/6-arg form when the offset args are literal `0` and the
  rotation flags are literals; the *actual* non-zero offset case, ~2.4% of
  real calls, should still decline). Cite #3487 rather than re-deriving this
  from scratch; `docs/engine/m47-3-quest-alias-design.md`'s own Phase 2
  checklist item for this re-measurement can be treated as answered (with
  this qualification) even though the checkbox itself may still render
  unticked in that doc.
- **QUST alias decode + fill-and-apply runtime (M47.3, Phases 0–3 shipped
  2026-08-07, `a844c26b`)** — `QustRecord.aliases: Vec<QuestAlias>`
  (`ALST`/`ALLS`/fill-types/`FNAM`/injected data/`ALFI` "Force Into Alias")
  decodes in `crates/plugin/src/esm/records/misc/quest.rs`, live-verified
  against `Skyrim.esm`/`Fallout4.esm` (`crates/plugin/examples/
  qust_alias_survey.rs` + `qust_alias_rawdump.rs`). That parser stays out of
  this skill's crate scope (`crates/plugin`, not `pex`/`papyrus`/
  `scripting`) — don't expand this skill's dimensions to cover `quest.rs`
  itself, it belongs to `/audit-esm` Dim 4. **The
  fill-and-apply runtime this bullet used to describe as unbuilt is now
  live and IS in scope**, in `crates/scripting/src/scene.rs`:
  `SceneActorBindings` fills Forced Reference / Unique Actor / loaded Find
  Matching / Location Alias Reference / External Alias / Force-Into-Alias
  aliases from loaded candidates, and `apply_alias_injections` applies
  alias-injected factions onto `FactionRanks` and inventory onto
  `Inventory` (permanent-grant ledger in `QuestAliasInjectionState`,
  persisted across save/load — see Dim 6). `QuestRef::Property` still
  declines on an alias-bound entry — quests themselves aren't alias-fillable,
  so that's still correct, not a regression
  (`dispatch_skips_property_targeted_effect_when_the_property_is_alias_bound`).
  `ObjectRef::Property` (`fragment.rs::resolve_object`, Dim 5) and
  `RunOn::QuestAlias` (`condition.rs`, Dim 6) now resolve an alias-bound
  entry (`alias >= 0`) through `SceneActorBindings::resolve` instead of
  declining — verified live by
  `dispatch_activate_then_set_open_updates_mq101_style_gate`
  (`crates/scripting/src/fragment/tests.rs`). **Do not re-flag the
  alias-bound `ObjectRef::Property`/`RunOn::QuestAlias` decline as still
  correct-because-unbuilt** — that framing is stale. Real, bounded gaps that
  DO remain (per `docs/engine/m47-3-quest-alias-design.md`'s "Remaining
  subsystem boundary" and the commit message): Created Object alias fill
  (needs a base-record spawn pipeline), From-Event alias fill (needs Story
  Manager event payloads), Forced Location / true `LCTN` alias traversal,
  reference-collection aliases, unloaded-world Find-Matching search (Story
  Manager), and the injected packages/spells/keywords/names/voice-types/
  combat-override overlay families, which stay parsed-and-exposed as
  `QuestAliasInjectedOverlays`/`QuestAliasRuntimeOverlays` rather than
  applied to any consumer component. Flag a finding that assumes one of
  those already works; don't flag their absence as a new discovery — it's
  documented, not silent.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,2,3`). Default: all 8.
- `--depth shallow|deep`: `shallow` = check API contracts + the decline/bounds
  invariants; `deep` = trace each decompiler pass's tree rewrite + the per-frame
  ECS lifecycle. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: PEX Reader & Opcode Decode | Decompiler CFG & Lift | Decompiler Control-Flow / Boolean / Lower | Papyrus Lexer & Pratt Parser | Recognizer-Chain Soundness | Scripting Runtime Systems | Engine Attach & Trigger Wiring
- **Untrusted-Input**: Yes | No (set Yes for any finding on a path that consumes raw `.pex` / `.psc` bytes — these escalate by the special rules below)

## Severity Notes for This Domain

Apply `_audit-severity.md` as written. Domain-specific escalations:

| Condition | Minimum Severity |
|-----------|-----------------|
| Panic / OOB index / unbounded alloc reachable from untrusted `.pex` or `.psc` bytes | HIGH (CRITICAL if it's memory-unsafe — see the `transmute` in `crates/pex/src/opcode.rs`) |
| Decompiler emits a **wrong** AST that a recognizer then matches (false-positive lowering → wrong ECS behavior on vanilla content) | HIGH (silent, all-game blast radius; same class as a wrong NIFAL `Material`) |
| Recognizer emits a component on an **unmodeled** condition/term instead of declining | HIGH (the load-bearing invariant; a quest advancing on the wrong predicate is silent game-logic corruption) |
| Copy-propagation / boolean-collapse soundness bug (folds a temp into the wrong consumer, mis-attributes an `&&`/`||` operand) | HIGH (corrupts the AST the recognizer reads) |
| Stack overflow via unbounded recursion in the parser or a decompiler tree walk | HIGH |
| ECS lock held across a second resource/component mutation (deadlock vector) | HIGH |
| Transient marker not drained / drained out of stage order (re-fires every frame, or fires a frame late) | HIGH |
| `feature-matrix.md` doc-rot, stale comments | LOW |

The decline-on-unmodeled invariant is the scripting analogue of NIFAL's
single-boundary rule: **a partial / approximate lowering is worse than no
lowering**, because an inert unrecognized script is safe but a wrongly-lowered
one corrupts game state with no fallback to mask it.

## Phase 1: Setup

1. Parse `$ARGUMENTS` for `--focus`, `--depth`.
2. `mkdir -p /tmp/audit/scripting`
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 300 --json number,title,state,labels > /tmp/audit/scripting/issues.json`
4. **Read the most recent `docs/audits/AUDIT_SCRIPTING_*.md` report** (sort by
   date — do not hardcode a filename here, it rots every cycle). Diff direction
   against it rather than re-litigating settled findings. In particular, the
   M47.1 condition-resolver stubs (#1663–#1668, #1316) that earlier reports
   tracked as open are now CLOSED and fully implemented — verify against the
   live `crates/scripting/` code before flagging any condition-resolver gap,
   don't assume the stub-era finding still applies.
5. Read the three crate module docstrings + `docs/engine/m47-2-design.md` §"Frontends
   in detail" and §"Risks & mitigations" to confirm what is *designed* to decline /
   defer vs. what is a real defect, before reporting any "missing handling" finding.

## Phase 2: Launch Dimension Agents

Ordered by bug risk: untrusted decode + the five decompiler passes first
(Dims 1–3), then the source parser, the recognizer invariant, the runtime
lifecycle, and the engine wiring.

### Dimension 1: `.pex` Reader & Opcode Decode (untrusted input)
**Entry points**: `crates/pex/src/reader.rs` (`Reader`, `read_binary`, `read_header`,
`read_string_table`, `read_debug_info`, `skip_property_groups`, `skip_struct_orders`,
`read_objects`, `read_struct_infos`, `read_variables`, `read_guards`, `read_properties`,
`read_states`, `read_named_functions`, `read_function`, `read_typed_names`,
`read_instructions`, `value`, `string`, `string_index`, `take`); `crates/pex/src/opcode.rs`
(`OpCode`, `from_u8`, `MAX_OPCODE`, the `OPCODES` table); `crates/pex/src/model.rs`
(`Pex`, `Object`, `Function`, `Instruction`, `Value`, `ScriptType`); `crates/pex/src/lib.rs`
(`parse`, `PexError`).
**Checklist**:
- **`take(n)` is the single bounds gate.** Every primitive read funnels through
  `take` (`checked_add` + `<= data.len()` filter → `UnexpectedEof`). Verify NO
  read path bypasses it (a direct `self.data[...]` slice, a `try_into().unwrap()`
  on a short slice). Untrusted-Input: every finding here is Yes.
- **The `OpCode::from_u8` `transmute`.** `crates/pex/src/opcode.rs` does
  `unsafe { transmute::<u8, OpCode>(byte) }` guarded by `byte >= MAX_OPCODE → None`.
  This is memory-safety-critical: confirm (a) `MAX_OPCODE == 51` exactly matches
  the last discriminant (`TryLockGuards = 50`), (b) the enum is `#[repr(u8)]` with
  *contiguous* discriminants 0..=50 (a gap would make a valid-range byte transmute
  to an invalid variant = UB), (c) the guard is `>=` not `>`. The
  `discriminants_match_on_disk_order` + `from_u8_round_trips_and_rejects_oob` tests
  pin this — verify they actually cover every discriminant, not just spot values.
- **`arg_count` drives operand consumption.** `read_instructions` reads exactly
  `op.arg_count()` fixed operands + (if `has_varargs`) a `Value::Integer(n >= 0)`
  count then `n` operands. The `OPCODES` table is the contract. Cross-check every
  row against the UESP Papyrus Assembly spec / Champollion `OPCODES` (the file
  claims a verbatim port) — a wrong arg count desyncs the entire instruction
  stream silently (subsequent opcodes read garbage operands). Spot-check the
  var-arg opcodes (`callmethod`/`callparent`/`callstatic`/`lock_guards`/
  `unlock_guards`/`try_lock_guards`) and the high-arity ones
  (`array_findstruct` = 5, `array_getallmatchingstructs` = 6).
- **`BadVarArgCount`**: a negative or non-integer var-arg count is rejected.
  The var-arg element count `n` (`Value::Integer`, attacker-controlled up to
  `i32::MAX`) does **not** feed a `Vec::with_capacity(n)` — `read_instructions`
  grows the `var_args` vec geometrically via a plain `Vec::new()` + per-element
  `push` loop instead (#1710/SCR-D1-01), specifically because a
  `with_capacity(n)` there would pre-allocate tens of GB before the first
  out-of-range element read could hit `take`'s EOF guard. Verify this is still
  true — a "cleanup" that reintroduces `Vec::with_capacity(n as usize)` here
  reopens #1710. Regression guard: `hostile_vararg_count_errors_instead_of_ooming`
  (`crates/pex/src/reader.rs`). Every OTHER `with_capacity(count)` in the reader
  (`read_string_table`, `read_objects`, `read_instructions`'s own
  instruction-count vec, `read_typed_names`, `read_struct_infos`, …) is fed by a
  `u16` count capped at 65535 — benign. The `u32` `user_flags` / object-size
  fields are plain data (bitflags / a size hint), never used as a `Vec`
  capacity — not a hazard.
- **`string_index` range check**: a `u16` index is `.get(idx).cloned()` →
  `BadStringIndex` on miss. Verify NO field reads a raw `u16` and indexes
  `self.strings[idx]` directly (panic on OOB).
- **`value()` type tag**: only 0..=5 accepted (`BadValueType` otherwise). Confirm
  the six arms match `ValueType` and that `Value::Integer(self.u32()? as i32)`
  sign-reinterprets (not truncates) — Papyrus ints are signed.
- **Endianness / dialect detection**: magic LE (`0xFA57C0DE`) vs BE
  (`0xDEC057FA`) sets `endian`; `script_type` derives from endian + `game_id`
  (4→Starfield, 3→FO76, else FO4; BE→Skyrim). `u32_opt(true)` reads the magic LE
  before endian is known — verify every *other* multi-byte read honors `self.endian`
  and that the provisional `Endian::Little` in `new()` can't leak into a read
  before `read_header` sets it.
- **Skyrim-vs-FO4+ field gating**: `is_skyrim()` skips `const_flag`, `struct_infos`,
  property-group / struct-order debug tables; Starfield-only `guards`. A misgated
  field shifts the whole stream. Verify `read_objects` reads fields in the exact
  FileReader order and that `skip_property_groups`/`skip_struct_orders` consume the
  same bytes the (FO4+) writer emits (the doc says "consume-and-discard to stay
  aligned" — a wrong skip count corrupts every following object).
- **No partial `Pex` escapes**: `lib.rs` claims the reader "never returns a
  half-built `Pex`". Confirm `read_binary` is all-or-`Err` (no `Ok` with a
  truncated `objects` Vec on a mid-object EOF).
- Regression guards: `parses_a_handbuilt_fo4_pex`, `parses_a_handbuilt_skyrim_be_pex`,
  `parses_a_handbuilt_starfield_pex_with_guards`, `rejects_bad_magic`,
  `rejects_truncation` (`crates/pex/src/lib.rs`); `metadata_matches_champollion`
  (`crates/pex/src/opcode.rs`). FO4/LE, Skyrim/BE, and Starfield-guards dialects
  all round-trip via `PexWriter::new_be()` + the two new tests (#1728) — the
  prior MEDIUM coverage gap (hand-built writer only exercising FO4/LE) is closed;
  a future writer regression that drops the BE or guards path re-opens it.
**Output**: `/tmp/audit/scripting/dim_1.md`

### Dimension 2: Decompiler — CFG Construction & Opcode→Node Lift (highest bug density)
**Entry points**: `crates/pex/src/decompile/cfg.rs` (`build_cfg`, `CodeBlock`,
`Cfg`, `split`, `split_block`, `find_block_for_instruction`, `checked_target`,
`condition_name`, `END`); `crates/pex/src/decompile/lift.rs` (`lift_function`,
`create_node`, `check_assign`, `rebuild_expression`, `count_constant_id`,
`replace_constant_id`, `build_var_types`); `crates/pex/src/decompile/node.rs`
(`Node`, `NodeKind`, `is_final`, `is_temp_var`, `child_nodes`, `child_nodes_mut`,
`SYNTH_IP`).
**Checklist**:
- **Jump-target bounds**: `checked_target` validates `0 <= ip+offset <= count`
  (inclusive — the exit anchor is one past last). `build_cfg` errors on a
  non-integer offset (`BadJumpOffset`) or OOB (`JumpOutOfRange`). Verify the
  inclusive bound is correct (a jump to `count` lands on the synthetic exit block,
  not OOB) and that `condition_name` rejects non-{ident,bool,int} conditions
  (`BadJumpCondition`). Untrusted-Input: Yes.
- **Block-split arithmetic**: `CodeBlock::split(at)` truncates to `[begin, at-1]`
  and emits tail `[at, end]`. `at` is always `ip+1` or a jump target ≥ 1, so
  `at-1` can't underflow — confirm `split` is never called with `at == 0` (the
  initial full block starts at 0 and the exit anchor pre-exists, so `ip+1` for the
  final instruction maps to the anchor without a split). A `split(0)` is an
  underflow panic.
- **`jmpf` vs `jmpt` edge polarity**: `jmpf` jumps when FALSE, so true-edge =
  fall-through (`ip+1`), false-edge = target; `jmpt` is mirrored. This is the
  load-bearing CFG semantic — a flipped polarity inverts every `If`. The
  `forward_jmpf_builds_an_if_diamond` + `backward_jmpt_builds_a_loop_edge` tests
  pin it; verify the `(on_true, on_false)` tuple in `build_cfg` matches.
- **Copy-propagation soundness (`rebuild_expression`)**: a non-final
  (temp-producing) node is folded into the *single* following live statement
  that consumes its result via `count_constant_id`; **0 → advance, 1 → inline
  and resume at the live predecessor of the fold target, >1 →
  `ExpressionRebuildFailed`**. This is the AST-correctness core. Post-#2024
  the pass runs over an explicit doubly-linked live-index chain, NOT a
  restart-at-`i=0` rescan of the whole `Vec` — the old restart-at-0 behavior
  (Champollion's `it = scope->begin()`) was itself the O(n²) bug #2024 fixed
  (confirmed by a 20k/40k/80k-fold-pair benchmark); a "cleanup" that
  reintroduces it, or reintroduces `Vec::remove`'s O(n) shift, reopens #2024.
  Regression guard: `rebuild_expression_is_linear_up_to_the_wire_format_ceiling`
  (`crates/pex/src/decompile/lift.rs`). Verify: (a) the count is over the
  *immediately next live* statement only (the `next[i]` link — Champollion's
  single-consumer model); folding into a non-adjacent consumer would reorder
  side effects; (b) `is_final` / `is_temp_var`
  asymmetry is intact (`is_final` treats any `::temp` prefix as non-final incl.
  `_var`-suffixed; `is_temp_var` excludes `_var`) — the file documents this as a
  deliberate Champollion port, so a "cleanup" that unifies them is a regression;
  (c) the `replace_constant_id` `slot.take()` substitutes exactly once (the
  `debug_assert!(slot.is_none())` only fires in debug — a release build with a
  >1-match that slipped past `count_constant_id` would silently drop the producer).
- **`create_node` opcode→node map**: each opcode maps to a `NodeKind`. Spot-check
  the precedence values passed (they're cosmetic for AST lowering but the file
  carries them); the **`Cast` heuristic** (lift.rs): a cast is downgraded to a
  `Copy` when source is `None`, or when both sides are same-typed identifiers
  (or src is `::nonevar`). Verify the same-type test uses `type_of` on *both* and
  the `::nonevar` case-insensitive exception — a wrong downgrade turns a real
  type-narrowing cast into an identity copy (recognizer reads the wrong type).
- **`CallStatic`/`CallMethod`/`CallParent` operand order**: result, object, method
  name are pulled from specific arg indices (`id(2)`/`val(1)`/`id(0)` for
  CallMethod; `id(2)`/`val(0)`/`id(1)` for CallStatic — result/object/method in
  that order, matching the CallMethod convention; note the actual `Node::call_method`
  call site writes them `val(0), id(1)` positionally, i.e. object-then-method, so
  don't misread the source's left-to-right argument order as the result/object/
  method order this bullet uses). A swapped index mis-names the called function —
  fatal for a recognizer that keys on the method name (`SetStage`, `GetStageDone`).
  Cross-check against the UESP opcode operand order.
- **`id_of` on a literal**: operands that must be identifiers (`id(n)`) error with
  `ExpectedIdentifier` on a literal. Verify the lift never `unwrap()`s
  `as_identifier()` outside a checked branch (the Cast arm does
  `src.as_identifier().unwrap()` — confirm it's guarded by the preceding
  `matches!(src, Value::Identifier(_))` short-circuit).
- **Bodyless / native functions**: `build_cfg` returns `entry == END` for zero
  instructions; `lift_function` yields empty scopes. Verify a native/abstract
  function (no body) decompiles to an empty body, not a panic.
- **`Vec::with_capacity(op.arg_count())`** in lift — bounded (≤ 6), benign.
- Regression guards: `temp_folds_into_its_single_consumer`,
  `chained_temps_fold_into_one_expression`, `call_with_inlined_argument`,
  `property_set_lowers_to_assign_of_property_access`,
  `cast_between_different_types_is_a_cast_not_a_copy`,
  `double_use_of_a_temp_is_an_error` (`crates/pex/src/decompile/lift.rs`);
  `bodyless_function_yields_empty_cfg`, `straight_line_is_one_block_plus_exit`,
  `forward_jmpf_builds_an_if_diamond`, `backward_jmpt_builds_a_loop_edge`,
  `jump_out_of_range_is_an_error`, `non_integer_jump_offset_is_an_error`
  (`crates/pex/src/decompile/cfg.rs`).
**Output**: `/tmp/audit/scripting/dim_2.md`

### Dimension 3: Decompiler — Control-Flow, Short-Circuit Booleans & AST Lowering
**Entry points**: `crates/pex/src/decompile/control_flow.rs` (`reconstruct`,
`Reconstructor`, `rebuild`, `before_exit`, `take_scope`);
`crates/pex/src/decompile/boolean.rs` (`rebuild_boolean_operators`, `BoolPass`,
`collapse`, `last_result`, `take_operand`, `combine`, `BoolOp`);
`crates/pex/src/decompile/lower.rs` (`decompile_script`, `decompile_body`,
`lower_expr`, `lower_stmt`, `lower_body`, `build_handler`, `lower_property`,
`lower_type`, `lower_binary_op`); `crates/pex/src/decompile/event_names.rs`
(`is_event_name`, `EVENT_NAMES`); pipeline order in
`crates/pex/src/decompile/mod.rs`.
**Checklist**:
- **Pass order is load-bearing**: `decompile_body` runs cfg → lift →
  **`rebuild_boolean_operators` (before)** → `reconstruct` → `lower_body`. The
  boolean pass MUST precede control-flow reconstruction (it collapses `&&`/`||`
  short-circuit chains into one conditional so the CF pass sees a clean diamond).
  Verify the order; a swap leaves `||` chains as the "unmerged conditional `last`"
  case in `control_flow.rs` (which the file documents it *skips* — see below).
- **Control-flow shape classification (`rebuild`)**: reads structure off block
  edges — While (body tail jumps back to the condition: `last.next == current`),
  simple If (`last.next == exit`), If/Else (else). The **jmpt inversion** negates
  the condition and swaps edges when `before == current`. Verify the
  while/if/if-else discriminants against the edge invariants and that
  `before_exit` returns the block containing `exit-1` (the degenerate `exit == 0`
  returns `END` → `fail()`).
- **The deliberate skip in `control_flow.rs`**: when `last` is *itself*
  conditional, the block hits the `||` short-circuit case the boolean pre-pass
  is supposed to have already collapsed. This branch **fails closed**
  (`ControlFlowFailed`, SCR-D3-01/#1732) rather than silently advancing past
  the block and dropping its lifted statements — the pre-#1732 "advance by
  one, drop the guard" behavior this bullet used to describe was itself the
  wrong-AST hazard #1732 fixed. `decompile_script`'s `Err` then makes
  `translate_pex` degrade to a clean `None` decline (§Dim 5), so a script that
  reaches this branch declines rather than mis-decompiling. Verify the
  fail-closed `Err` is still there (a "fix" that resumes silently dropping the
  block reopens #1732) and that well-formed input never reaches this branch
  because the boolean pass ran first. Regression guard:
  `conditional_predecessor_fails_closed` (`crates/pex/src/decompile/control_flow.rs`).
- **Boolean collapse soundness (`boolean.rs`)**: `&&` = true edge falls through
  (`block.on_true() == block.end + 1`), `||` = false edge falls through. The
  operand block must *recompute the same condition variable* (`take_operand`
  checks `result == cond`). The file documents **two deliberate departures from
  Champollion**: (1) NO debug-line guard (it relies on the structural signal
  alone — Champollion uses per-instruction source lines to reject cross-line
  merges); (2) a termination guard (only re-process on a real merge). Audit both:
  for (1), reason about whether a non-`&&`/`||` block that *happens* to recompute
  a same-named temp on its fall-through edge could be falsely collapsed (a
  false-positive merge fabricates a boolean operator that wasn't in the source —
  wrong AST). The file says this is "validated against the corpus decompile rate +
  the R5 fidelity gate" — point the finding at those instruments, not speculation.
  For (2), confirm the re-process loop strictly shrinks the graph (merges a
  non-exit rejoin) so it terminates — an infinite loop here hangs the decompiler.
- **`combine` precedence + assign preservation**: `&&` = prec 7, `||` = prec 8;
  an enclosing `Assign` is rebuilt around the combined op. Verify the operand
  unwrap in `take_operand` (`std::mem::replace(value, Constant(None))`) leaves no
  dangling `None` in the tree.
- **AST lowering totality (`lower.rs`)**: `lower_expr` / `lower_stmt` must be
  total (no panic on any `NodeKind`). Note the *intentional* lossy lowerings —
  flag them only if a recognizer keys on the lost info:
  (a) statement-shaped nodes appearing as sub-expressions → `Expr::NoneLit`
  (should be unreachable; if reachable it's a lift bug);
  (b) `is` type-test → `Cast` (no AST `is`);
  (c) `StructCreate` → `New` with size 0;
  (d) `lower_binary_op` default arm → `BinaryOp::Eq` (a comment says "shouldn't
  reach here" — a real unknown op silently becomes `==`, which would corrupt a
  condition; verify only the modeled op strings reach it).
- **Event-vs-function classification (`build_handler`)**: a name is an `Event`
  iff (`on`-prefixed AND `is_event_name`) OR `::remote_`-prefixed. `EVENT_NAMES`
  is a sorted lowercase union (Skyrim+FO4+Starfield) binary-searched by
  `is_event_name`. Verify the list stays sorted (the `list_is_sorted_for_binary_search`
  test guards it) — an unsorted entry makes `binary_search` miss it, demoting a
  real event handler to a plain function (recognizers that look for `OnActivate`
  as an `Event` would miss it). A *missing* engine event in the union is the same
  bug; spot-check that high-frequency events from the recognizer-scaling doc
  (`onactivate`, `onload`, `ontriggerenter`, `onhit`, `ontimer`, `oninit`,
  `onupdate`) are all present.
- **`decompile_script` assembly**: synthetic `::`-prefixed variables dropped;
  auto-state functions → script-scope items, named states → `State` items;
  property getter/setter bodies decompiled via `build_named_function`. Verify the
  auto-state match uses `state.name == object.auto_state_name` (a Skyrim
  empty-string auto-state vs FO4 named auto-state both handled).
- **The 99.996% claim**: this dimension owns verifying the corpus-smoke harness
  (`crates/pex/examples/pex_corpus_smoke.rs`) actually decompiles (not just
  parses) every `.pex` and counts panics/`Err` as failures. The README/docs claim
  26640/26641 — confirm the harness's `decompile_script` call is inside the
  success/failure tally and that a panic isn't caught-and-counted-as-success.
- **Recursion-depth caps**: both `control_flow.rs::Reconstructor::rebuild` and
  `boolean.rs::BoolPass::rebuild` thread a `depth` param capped at
  `MAX_REBUILD_DEPTH = 1024`, erroring `DecompileError::RecursionLimit` rather
  than overflowing the stack (control-flow: pre-existing #1729; boolean: #1815/
  SCR-D2-01, fixed by `7fdb694b`). Verify both still cap — a "cleanup" that drops
  the boolean-pass thread regresses #1815. Regression guards: the
  `rebuild_rejects_excessive_recursion_depth` test exists in **both**
  `control_flow.rs` and `boolean.rs` (same name, distinct files/tests).
- Regression guards: `simple_if_reconstructs`, `if_else_reconstructs_both_branches`,
  `while_loop_reconstructs`, `nested_and_becomes_nested_ifs`,
  `straight_line_has_no_control_flow_nodes` (`crates/pex/src/decompile/control_flow.rs`);
  `and_collapses_to_a_single_if_with_an_and_condition`,
  `or_collapses_to_a_single_if_with_an_or_condition`,
  `plain_if_is_untouched_by_the_boolean_pass`,
  `straight_line_with_a_call_is_unchanged` (`crates/pex/src/decompile/boolean.rs`);
  `an_on_activate_function_lowers_to_an_event`, `a_plain_function_stays_a_function`,
  `an_if_with_a_call_lowers_to_an_if_statement`, `auto_property_lowers_with_auto_flag`
  (`crates/pex/src/decompile/lower.rs`); `list_is_sorted_for_binary_search`,
  `known_events_match_case_insensitively` (`crates/pex/src/decompile/event_names.rs`).
**Output**: `/tmp/audit/scripting/dim_3.md`

### Dimension 4: Papyrus `.psc` Lexer & Pratt Parser (untrusted input)
**Entry points**: `crates/papyrus/src/lib.rs` (`parse_script`, `parse_expr`);
`crates/papyrus/src/token.rs` (logos `Token`, `ignore(ascii_case)` keyword
attrs, the `Ident` regex); `crates/papyrus/src/lexer.rs` (`preprocess`,
`OffsetMap`); `crates/papyrus/src/parser/expr.rs` (`parse_expr_bp`,
`parse_expr_bp_inner`, `MAX_EXPR_DEPTH`, `PREC_*`); `crates/papyrus/src/parser/mod.rs`
(`expr_depth`); `crates/papyrus/src/parser/stmt.rs`; `crates/papyrus/src/parser/script.rs`
(`skip_to_next_line`, item recovery); `crates/papyrus/src/ast.rs`
(`BinaryOp::precedence`); `crates/papyrus/src/error.rs` (`ExpressionTooDeep`).
**Checklist**:
- **Recursion-depth cap (`MAX_EXPR_DEPTH = 256`, #1270 / SAFE-DIM3-NEW-02)**:
  `parse_expr_bp` increments `expr_depth` at entry, returns
  `ExpressionTooDeep` at the cap, decrements at exit. This is the stack-overflow
  guard against pathological `((((…))))`. Verify (a) the increment/decrement is
  balanced on every return path including the error path (a missed decrement
  would falsely cap legitimate sibling expressions); (b) ALL recursive expression
  entry funnels through `parse_expr_bp` (no direct `parse_expr_bp_inner` recursion
  that bypasses the gate); (c) the *statement* parser (`stmt.rs`) has its own
  guard: `stmt_depth`/`MAX_STMT_DEPTH = 256` (#1712) mirrors `expr_depth` and
  caps nested `If`/`While` block recursion — verify it still resets between
  top-level calls and rejects pathological nesting (guards:
  `stmt_depth_cap_rejects_pathological_nested_if`,
  `stmt_depth_cap_rejects_pathological_nested_while`,
  `stmt_depth_cap_accepts_legitimate_nesting`,
  `stmt_depth_resets_between_top_level_calls`). Untrusted-Input: Yes.
- **Operator precedence + associativity**: `ast.rs`'s `BinaryOp::precedence`
  gives Or=1, And=2, comparisons=3, Add/Sub/StrCat=4, Mul/Div/Mod=5; `ast.rs`'s
  separate `UnaryOp::precedence` gives unary=6; cast=7/postfix=8 are the
  `PREC_CAST`/`PREC_POSTFIX` consts in `parser/expr.rs`, not `ast.rs`. Left-
  associativity hinges on the Pratt loop's `op_prec <= min_bp →
  break` (the `<=`, not `<`). Verify `a - b - c` → `(a-b)-c`
  (`test_left_associativity`) and `a + b * c` → `a + (b*c)`. Note: Papyrus's
  *runtime CTDA* OR-precedence quirk (Bethesda's inverted AND/OR) is a *condition
  evaluation* concern (Dim 6) — the `.psc` source operators here are standard.
- **Line-continuation preprocessing (`lexer.rs::preprocess`)**: a `\` immediately
  before `\n` / `\r\n` / lone `\r` is elided (2 / 3 / 2 bytes) and recorded in
  `OffsetMap` for span remap; any other `\` passes through. Verify the `\r`-only
  ("Mac classic") branch and that the `OffsetMap` byte counts (2/3/2) exactly
  match the elided bytes — a wrong count drifts every subsequent error span. Edge:
  a trailing `\` at EOF (no following newline) — confirm it's emitted, not
  swallowed (an OOB peek).
- **Case-insensitive keywords**: every keyword `#[token(..., ignore(ascii_case))]`;
  identifiers preserve case via the `Ident` regex (`priority = 1`). Verify a
  keyword-shaped identifier (e.g. a variable literally named `state`) is handled
  per Papyrus rules — logos keyword tokens win over the lower-priority `Ident`
  regex, so `state` always lexes as the keyword. Flag if that breaks any legal
  vanilla identifier (Papyrus reserves these, so it's likely correct — confirm
  against the grammar in `docs/engine/papyrus-parser.md` rather than assuming).
- **Error recovery**: `parse_script` returns `Ok((Script, Vec<ParseError>))` for
  partial success (collects per-item errors, `skip_to_next_line`, continues) and
  `Err` only for fatal failures (missing `ScriptName`, lex error). `parse_expr`
  bails on first error. Verify `skip_to_next_line` always makes progress (consumes
  ≥1 token) — a recovery point that doesn't advance is an infinite loop on a
  malformed item. Confirm callers that need strict-fail check `result.1.is_empty()`.
- **Integer/literal parsing**: hex (`test_hex_literal`), negative ints, floats —
  verify no `unwrap()` on `str::parse` that a malformed-but-lexable literal could
  panic on (lexer should reject before parse, but confirm the seam).
- Regression guards (sample — there are ~56): `depth_cap_rejects_pathological_parens`,
  `depth_cap_accepts_legitimate_nesting`, `depth_resets_between_top_level_calls`
  (`crates/papyrus/src/parser/expr.rs`); `test_left_associativity`,
  `test_precedence_mul_over_add`, `test_precedence_and_over_or`,
  `test_cast_precedence` (`crates/papyrus/src/parser/expr.rs`);
  `test_preprocess_line_continuation`, `test_preprocess_crlf_continuation`,
  `test_lex_case_insensitive_keywords` (`crates/papyrus/src/lexer.rs`);
  `parse_full_rumble_on_activate_translation` (`crates/papyrus/src/parser/script.rs`).
**Output**: `/tmp/audit/scripting/dim_4.md`

### Dimension 5: Recognizer-Chain Soundness (decline-on-unmodeled — the load-bearing invariant)
**Entry points**: `crates/scripting/src/translate/mod.rs` (`translate_script`,
`translate_pex`, `RECOGNIZERS`); `crates/scripting/src/translate/archetype.rs`
(`RecognizeCtx`, `Recognized`, `SpawnFn`, `Recognizer`);
`crates/scripting/src/translate/source.rs` (`ScriptSource`);
`crates/scripting/src/translate/compose.rs` (`split_and`, `classify_guard_atom`,
`GuardPrimitive`, `GUARD_PRIMITIVES`, `GuardMatch`, `quest_via`, `QuestRef`,
`ObjectRef`, `prim_player_gate`, `prim_stage_done`);
`crates/scripting/src/translate/effects.rs`
(`lower_fragment`, `classify_effect`, `EffectPrimitive`, `EFFECT_PRIMITIVES`,
`Effect` incl. the `AddItem`/`MoveTo` object-targeting variants (2026-07-21)
and the `StartQuest`/`StopQuest`/`CompleteQuest`/`ResetQuest`/
`SetQuestActive`/`FailAllObjectives` quest-lifecycle variants (2026-08-07,
`a844c26b`) — the latter also widened `SetObjectiveDisplayed`/
`SetObjectiveCompleted`/`SetObjectiveFailed`'s `objective` field from `u16`
to `i32` (`prim_set_objective_*`; confirm the widen is a genuine bug fix —
Bethesda objective indices are a signed 32-bit field on the wire — and not a
silent range-check loosening), `receiver_object`, `prim_add_item`,
`prim_move_to`, `prim_start_quest`, `prim_stop_quest`, `prim_complete_quest`,
`prim_reset_quest`, `prim_set_quest_active`, `prim_fail_all_objectives`.
**Grown further 2026-08-24** (three same-day commits, `5f38402e`/`cee35507`/
`25a0aabd`): `Effect::Disable` (`prim_disable`, `<object>.Disable([fadeOut])`),
`Effect::SetGlobalValue` (`prim_set_global_value`, `<GlobalVariable>.SetValue(v)`),
and `Effect::Conditional` (`StageDoneGuard`, built from a narrowed `Stmt::If`
lowering in `lower_statements` — see the decline-invariant bullet below, this
is a real widening of what a fragment body may contain and the existing
"`Stmt::While` is the one narrowed exception to ANY-control-flow-declines"
framing is now incomplete);
`crates/scripting/src/translate/tables.rs` (`CanonicalEvent::from_papyrus`);
`crates/scripting/src/translate/recognizers/quest_stage_gate.rs` (`recognize`,
`extract_stage_gate`, `classify_if_condition`);
`crates/scripting/src/translate/recognizers/rumble.rs` (`recognize`);
`crates/scripting/src/translate/recognizers/two_state_activator.rs` (`recognize`
— per-script, matches `default2StateActivator`, added after this file's last
refresh; not otherwise covered by this dimension's checklist below).
**Checklist**:
- **The invariant**: a recognizer MUST return `None` (decline) on ANY unmodeled
  condition atom, effect statement, or unbindable hole — never emit a component
  built from a partial / approximated match. A false-positive lowering silently
  corrupts game logic (quest advances on the wrong predicate) with no fallback.
  This is the scripting analogue of NIFAL's no-fabrication rule.
- **Chain ordering (`mod.rs` `RECOGNIZERS`)**: per-script recognizers FIRST
  (`two_state_activator`, then `rumble`), generic families SECOND
  (`quest_stage_gate`), so a bespoke script isn't swallowed by a family match.
  `translate_script` is `RECOGNIZERS.iter().find_map(...)` — first match wins,
  all-`None` → silent miss. Verify the order matches the design (per-script
  before generic) and that adding a future generic recognizer can't shadow
  either per-script recognizer.
- **Guard decline enforcement (`compose.rs` + `quest_stage_gate.rs`)**: the
  load-bearing decline is `classify_guard_atom(atom, player_param)?` inside the
  per-atom loop in `classify_if_condition` — the `?` propagates `None` the instant
  an atom isn't claimed by `GUARD_PRIMITIVES`. Verify (a) the loop does NOT skip /
  ignore an unmatched atom (no `if let Some(..) = ... { }` that silently drops a
  `None`); (b) **`split_and` deliberately does NOT split `||`** — a disjunction is
  left as one atom no primitive matches, forcing a decline. This is intentional
  conservatism (the file documents it). Confirm an `If a || b` condition declines
  rather than lowering only the `a` half.
- **Effect decline enforcement (`effects.rs::lower_fragment`/`lower_statements`)**:
  mostly a flat-sequence model — `Stmt::ExprStmt(e) → classify_effect(&e.node, &scope)?`;
  `Stmt::VarDecl`/`Stmt::Assign` bind a local via `bind_local` (quest / object /
  player / side-effect-free-plain, or decline — the "ANY var-decl declines"
  framing this bullet used to state is no longer accurate, since a local that
  `bind_local` can classify is recorded, not declined); `Stmt::Return(None)` is
  the explicit no-op terminator; and there are now **two** narrowed exceptions
  to "ANY control flow declines" (both post-2026-07-21 — the "`Stmt::While` is
  the *one* exception" framing this bullet used to carry is stale):
  1. `Stmt::While` is accepted **only** through `lower_3d_loaded_wait` (MQ101
     cart-cinematic work), which requires the condition to be an OR-tree of
     `!<actor>.Is3DLoaded()` leaves and the loop body to be exactly one
     positive `Utility.Wait(..)` call.
  2. `Stmt::If` is accepted (2026-08-24, `cee35507`) **only** through the
     `Effect::Conditional` shape in `lower_statements`: `elseif_clauses` must
     be empty (any elseif declines the whole `If`); `split_and` +
     `classify_guard_atom(atom, None)` must classify every condition atom as
     an exact `GuardMatch::StageDone { expected: 0.0 | 1.0, .. }` (a
     non-stage-done atom, a non-conjunction, or a non-`0.0`/`1.0` comparand
     declines); both the `then` and `else` branches lower independently via a
     **cloned** `Scope` (`then_scope`/`else_scope`, so a local bound in one
     branch cannot leak into the other) through a recursive `lower_statements`
     call; and neither branch may contain a `Wait`/`WaitForActors3DLoaded`
     effect (`has_latent` rejects both — a conditional wrapping a latent
     effect declines the whole `If`, it does not partially lower). Verify all
     of these bounds are still exactly this narrow — a widened `elseif`
     allowance, a disjunction let through `classify_guard_atom`, or a latent
     effect surviving inside a branch would each be a decline-invariant
     regression. Any other `If`/valued-`Return` still hits `_ => return None`.
     **`lower_statements`'s own recursion is capped (Fix #3279,
     `ae9d4194`, 2026-09-03)**: `MAX_CONDITIONAL_DEPTH = 256` — the smaller
     of the two upstream caps that transitively bound it (`.psc`'s
     `MAX_STMT_DEPTH` / the `.pex` decompiler's `MAX_REBUILD_DEPTH`) — is
     threaded through as an explicit `depth: u32` parameter and declines
     (`None`) at or past it rather than recursing further; this was a
     defense-in-depth gap closure (both reachable input paths were already
     bounded upstream), not a live-exploitable unbounded recursion. Verify
     the cap is still there and still the smaller of the two upstream
     values. Guards: `lowers_get_stage_done_conditional`,
     `declines_unmodeled_conditional_guard`,
     `conditional_depth_cap_declines_pathological_nested_if`,
     `conditional_depth_cap_accepts_legitimate_nesting`
     (`crates/scripting/src/translate/effects.rs`).
  `effects.rs` has grown substantially since this dimension was last refreshed
  (many more `Effect`/`EFFECT_PRIMITIVES` variants beyond `AddItem`/`MoveTo` —
  scene/player-control/vehicle/cinematic effects for the MQ101 cart sequence,
  plus `Disable`/`SetGlobalValue`/`Conditional`, 2026-08-24); this checklist
  does not enumerate them individually, so treat the decline invariant above
  as the load-bearing thing to re-verify rather than assuming the older,
  smaller primitive table.
- **`Effect::Conditional` dispatch (`fragment.rs::apply_effects`, 2026-08-24;
  guard-resolution behavior fixed 2026-09-02, Fix #3785, `46cb7515` — the
  framing below is the POST-fix behavior, don't re-flag it)**:
  handled as a special case at the *top* of the per-effect loop, not inside
  `apply_effect` (`apply_effect`'s own `Effect::Conditional { .. } =>
  unreachable!(...)` arm is a defense-in-depth assertion that the special case
  always intercepts it first — verify it's never actually hit). Each
  `StageDoneGuard` resolves its `QuestRef` via `resolve_quest_logged` and
  compares `stages.get_stage_done(quest, guard.stage) == guard.done`; ALL
  guards must pass (`.all(...)`) to take `then_effects`, otherwise
  `else_effects` runs — **unless any guard's `QuestRef` fails to resolve at
  all**, in which case the whole `Conditional` declines (neither branch runs;
  the loop `continue`s to the next sibling effect in the same fragment body,
  it does not abort the fragment). Fix #3785 closed a real bug here: the prior
  `is_some_and` collapsed "guard evaluated false" and "guard's quest ref
  couldn't be resolved" into the same `false`, which — because `Conditional`
  (unlike every other `resolve_quest_logged` caller, which just skips the one
  effect via `?`) has an `else` arm — silently ran `else_effects` on an
  unresolvable guard instead of running neither. Verify (a) a `resolved: bool`
  (or equivalent) tracks resolution separately from the boolean guard value,
  and an unresolved guard short-circuits to declining the *whole* Conditional,
  not to selecting `else_effects`; (b) `log::warn!` fires on the decline path
  (`resolve_quest_logged`'s own `debug!` stays silent for its many inert
  callers, so this needed its own louder diagnostic); (c) the chosen branch
  recurses into `apply_effects` reusing the *same*
  `&mut stages`/`&mut objectives`/`world`/`deferred` — no new resource or
  component lock is acquired for the recursion; (d) the recursion has no
  depth/cycle hazard distinct from the outer `MAX_CASCADE` cascade guard (a
  `Conditional` itself never emits a `SetStage` loop back into the dispatch
  queue — only a `SetStage`/quest-lifecycle effect inside a branch does, and
  that re-enters the *outer* cascade queue, not this recursion) — note this
  runtime recursion is a *different* recursion from `lower_statements`'s
  lowering-time one (Dim 5 above, now capped by `MAX_CONDITIONAL_DEPTH`), but
  is transitively bounded by it since a dispatched `Conditional`'s nesting can
  never exceed what lowering allowed. Guard:
  `apply_effects_declines_conditional_with_unresolvable_guard`
  (`crates/scripting/src/fragment/tests.rs`).
- **`Effect::Disable` / `ReferenceEnableState` — BOTH 2026-08-24 gaps are now
  CLOSED (#3278, `26f8738d` + `265f0c9b`). This bullet is a regression guard,
  not an open finding; do not re-file either half.**
  * *Alias-aware receiver (half b, `26f8738d`)*: `Effect::Disable` used to
    resolve its `object: ObjectRef` through the narrow
    `resolve_property_form_id(vmad, object.property_name())` while
    `AddItem`/`MoveTo`/`EquipItem` went through the alias-aware
    `resolve_object`/`resolve_actor` (`deferred.scene_actor_bindings`) — so
    `<AliasBoundMarker>.Disable()` silently declined in exactly the cases where
    the same alias-bound receiver resolves fine for every sibling effect.
    Dispatch now classifies the receiver through the same `receiver_object`.
    Note the asymmetry that remains and is *correct*: `ReferenceEnableState` is
    deliberately **FormID-keyed, not entity-keyed**, so a disable survives its
    reference's cell being unloaded — an alias-bound receiver resolves to an
    entity and must therefore come back to a form id via `entity_global_form_id`
    (`fragment.rs`). A "simplification" that keys the sink by entity is the
    regression. `Effect::SetGlobalValue` keeping `resolve_property_form_id` is
    also correct (a GLOB is a top-level resource, never alias-bound).
  * *Runtime consumer (half a, `265f0c9b`)*: `ReferenceEnableState::is_enabled`
    used to have no production call site, so a script-disabled reference stayed
    fully visible, collidable and interactive. `spawn_placed_instances`
    (`byroredux/src/cell_loader/spawn.rs`) now consults it, gating **after the
    placement root but before any mesh, collider or light** — that position is
    load-bearing, because one check there covers all three (an unspawned mesh
    cannot render, an unspawned collider cannot block, an unspawned light cannot
    contribute). A render-side visibility flag would have covered only the
    first: `AnimatedVisibility` is honoured in `render/static_meshes.rs` but not
    in `render/skinned.rs`. Regression = moving the gate to the render side, or
    after collider/light spawn. Guard: `byroredux/src/cell_loader/reference_enable_gate_tests.rs`.
- **`Effect::SetGlobalValue` (`fragment.rs::apply_effect`)**: resolves the
  `global: ObjectRef` the same strict way as `Disable`
  (`resolve_property_form_id`, no alias branch — correct here, since a GLOB
  is a top-level resource never bound through a quest alias) and writes
  through `world.try_resource_mut::<crate::Globals>()`. `Globals` (
  `crates/scripting/src/globals.rs`) gained `#[cfg_attr(feature = "save",
  derive(Serialize, Deserialize))]` in the same commit and IS registered —
  `byroredux/src/save_io.rs` calls `.register_resource::<Globals>("Globals")`
  — so this is not the #1862-class "serde derive with no registry entry" gap;
  confirm that registration still holds on future `save_io.rs` refactors
  rather than re-deriving it each time.
- **Multi-fragment-per-stage ordering (`fragment.rs::populate_quest_fragments_from_script`,
  2026-08-24, `cee35507`)**: a QUST stage can carry several `QSDT` log
  entries, each with its own `Fragment_N` binding (the module doc cites
  MQ101 stage 0 having five). The function now accumulates all of a stage's
  lowered `Effect` chains into one `effects_by_stage: HashMap<u16, Vec<Effect>>`
  in first-seen (VMAD) order via `stage_order`, and installs each stage's full
  merged chain into `frags` exactly once at the end — replacing, not
  appending to, whatever was previously installed for that `(quest, stage)`.
  This replaces the prior last-write-wins behavior (each `Fragment_N` binding
  used to overwrite the previous one's effects for the same stage via a bare
  `frags.insert`). Verify (a) the merge preserves authoring order across
  bindings, not just within one binding's own statement sequence; (b) a
  repeated call (re-population on a subsequent cell load) still replaces the
  installed chain rather than duplicating it — `stage_order`/`effects_by_stage`
  are function-locals rebuilt from scratch each call, so this should hold, but
  confirm no caller accumulates across calls.
- **Hole binding**: `QuestRef::{OwningQuest, SelfRef, Property(name)}` must FULLY
  resolve. `OwningQuest` needs `ctx.owning_quest` (decline if `None` —
  `declines_when_owning_quest_unavailable`); `Property(name)` needs the VMAD
  `script_instance` to carry that property as a form-id (decline if unbound —
  `declines_when_quest_property_unbound`); `SelfRef` on a REFR is declined
  (quest scripts attach to a quest, not a REFR). Verify each binding failure
  declines, never defaults to form-id 0.
- **`ObjectRef` hole binding (2026-07-21, object-targeting effects)**: unlike
  `QuestRef`, `ObjectRef` has **no unambiguous bare-receiver case at all** —
  no `Self`/`GetOwningQuest()` equivalent, since the fragment script always
  `extends Quest` and is never itself the `ObjectReference`/`Actor` being
  acted on. `receiver_object` (`effects.rs`) must: (a) explicitly reject a
  bare `Self` identifier (does NOT rely on no VMAD property ever being named
  "self" — verify the explicit `key == "self"` guard is still there); (b)
  decline any local-variable receiver, including a side-effect-free ident
  copy (`ObjectReference k = SomeProperty; k.AddItem(...)`) — **except this
  is now stale**: `effects.rs` carries a third map, `object_locals:
  HashMap<String, ObjectRef>` (introduced by `0ff8612b`, MQ101 cinematic
  effects), populated from `Binding::Object(via)` and consulted first in
  `receiver_object`, so an object-typed local **does** resolve today. Verify
  against the live three-map behavior (`scope.quest_locals` /
  `scope.decl_locals` / `scope.object_locals`), not the two-map "always
  declines" claim this bullet used to make. At *dispatch* time, `fragment.rs::resolve_object`
  (M47.3 Phase 2, updated 2026-08-07) branches on the VMAD
  `PropertyValue::Object`: `alias == -1` still resolves via
  `resolve_entity_by_global_form_id` (the same M42.5–8/M47.1 resolver,
  unchanged); `alias >= 0` now resolves through
  `world.try_resource::<crate::scene::SceneActorBindings>().resolve(context, alias)`
  instead of declining — the "needs the (unbuilt) quest-alias-fill
  subsystem" framing this bullet used to carry is **stale**, that subsystem
  (`crates/scripting/src/scene.rs`) is built and wired. Verify (a) no path
  still trusts the raw `form_id` sitting beside a live `alias >= 0` index —
  the historical wrong-object-application hazard this decline guarded
  against, now avoided by resolving through the binding table instead; (b)
  a not-yet-loaded/not-yet-filled alias still declines cleanly
  (`SceneActorBindings::resolve` returns `None`, never fabricates an
  entity); (c) `resolve_property_form_id` — a *different*, narrower
  function `fragment.rs` uses for `QuestRef::Property`/scene/idle property
  lookups elsewhere — is unaffected: it has no alias branch at all and
  correctly keeps requiring an exact form-id match, since quests and those
  other lookups are never alias-fillable (don't conflate the two functions).
  Guards: `add_item_declines_on_local_receiver`, `declines_on_unmodeled_effect`,
  `dispatch_add_item_via_registered_vmad`, `dispatch_move_to_via_registered_vmad`,
  `dispatch_activate_then_set_open_updates_mq101_style_gate` (alias-bound
  `ObjectRef::Property` resolving live) (`crates/scripting/src/fragment/tests.rs`).
- **`AddItem`/`MoveTo` conservative-shape declines**: `AddItem`'s optional
  3rd arg (`abSilent`) is accepted only as a literal (`bool_arg`'s `None` on
  a present-but-non-literal value must decline the whole primitive, mirroring
  `SetObjectiveDisplayed`'s existing discipline) and a 4th+ arg declines
  outright; `MoveTo` accepts *only* the 2-arg shape (receiver + destination)
  — any offset/match-rotation argument declines rather than silently
  dropping it and misplacing the object. Guards:
  `add_item_declines_with_non_literal_silent_arg`,
  `move_to_declines_with_offset_args` (`crates/scripting/src/translate/effects.rs`).
- **`quest_stage_gate` cross-check**: when the condition's quest and the
  `SetStage` target's quest disagree, the recognizer declines (don't advance the
  wrong quest). Verify `recognizes_da10_and_reproduces_hand_builder` (`.psc`-side,
  `quest_stage_gate.rs`) and `da10_pex_reproduces_hand_builder_byte_for_byte`
  (`.pex`-side, `crates/scripting/tests/pex_recognize_e2e.rs`, `#[ignore]`-gated
  on Skyrim SE game data, #1740) both assert byte-equality against
  `da10_main_door(...)` — together they are the full `.psc`-vs-`.pex` fidelity
  gate for this recognizer (the `.psc`-side test alone never touches
  `decompile_script`).
- **`rumble` per-script recognizer**: matches script name `defaultRumbleOnActivate`
  (case-insensitive) and extracts 5 auto-property float/bool initial values with
  `.psc` defaults; declines a non-literal property value and a different script
  name. Verify the literal-only extraction (a property initialized by an
  expression must decline, not coerce).
- **`CanonicalEvent::from_papyrus`** (`tables.rs`): a fixed lowercase-keyed
  catalog; unknown → `CanonicalEvent::Unknown` (a safe long-tail bucket, not an
  error). Verify the case-insensitive match and that `Unknown` callers treat it as
  "no consumer", never as a wildcard match.
- **`translate_pex` clean-`None` on bad bytes AND on panic**: `byroredux_pex::parse` /
  `decompile_script` `Err` → `log::debug` + `return None` (never a panic
  escaping into the cell loader). Guards: `translate_pex_on_empty_bytes_is_a_clean_none`,
  `translate_pex_on_garbage_bytes_is_a_clean_none`,
  `translate_pex_on_truncated_after_magic_is_a_clean_none`. A `decompile_script`
  **panic** is also caught via `catch_unwind` (`crates/scripting/src/translate/mod.rs`,
  #1816/SCR-D5-NEW-02) and degraded to the same `None` — verify the wrap is
  still present, not removed by a future refactor (no corpus `.pex` or
  characterized input currently triggers it — this is a safety net, not an
  active-bug regression test).
- Regression guards: `unrecognized_script_is_a_silent_miss` (`crates/scripting/src/translate/mod.rs`);
  `split_and_flattens_conjunction_keeps_disjunction_whole`, `unmodeled_atom_declines`,
  `stage_done_primitive_binds_holes`, `player_gate_primitive_matches_both_orders`
  (`crates/scripting/src/translate/compose.rs`); `declines_on_unmodeled_effect`,
  `declines_unmodeled_conditional_guard`, `empty_fragment_is_understood_as_noop`
  (`crates/scripting/src/translate/effects.rs`); the 20 `quest_stage_gate.rs`
  tests incl. `declines_unmodeled_condition_term`, `declines_handler_without_set_stage`,
  `declines_when_quest_property_unbound`, `declines_unconditional_with_extra_statements`
  (`crates/scripting/src/translate/recognizers/quest_stage_gate.rs`);
  `recognizes_rumble_and_extracts_psc_defaults`, `declines_a_different_script`
  (`crates/scripting/src/translate/recognizers/rumble.rs`);
  `canonical_event_unknown_for_long_tail` (`crates/scripting/src/translate/tables.rs`).
**Output**: `/tmp/audit/scripting/dim_5.md`

### Dimension 6: Scripting Runtime Systems — Lifecycle, Stage & Lock Ordering
**Entry points**: `crates/scripting/src/lib.rs` (`register`);
`crates/scripting/src/events.rs` (the marker structs);
`crates/scripting/src/timer.rs` (`timer_tick_system`, `ScriptTimer`);
`crates/scripting/src/cleanup.rs` (`event_cleanup_system`);
`crates/scripting/src/condition.rs` (`evaluate`, `evaluate_condition`,
`evaluate_function`, `ConditionFunction`, `ConditionContext`);
`crates/scripting/src/trigger.rs` (`trigger_detection_system`, `TriggerVolume`,
`TriggerShape`, `contains`, and — added 2026-08-24, `7473a387`/`cee35507` —
`intersects_sphere`, `TETHERED_HORSE_TRIGGER_RADIUS`, `TriggerOccupancyState`,
`actor_quest_trigger_is_in_sequence`); `crates/scripting/src/quest_stages.rs`
(`QuestStageState`, `QuestObjectiveState`, `set_stage`, `get_stage_done`, and
— added 2026-08-23, `eb2e2445` — `QuestAliasReadinessGate`,
`QuestAliasReadinessGateRegistry`, `install_quest_alias_readiness_gate`,
`quest_alias_readiness_stage_system`);
`crates/scripting/src/fragment.rs` (`quest_fragment_dispatch_system`,
`QuestStageFragments` incl. `insert_vmad`/`vmad` (2026-07-21), `apply_effects`,
`apply_effect`, `apply_quest_scoped_effect`, `resolve_quest_logged`,
`resolve_property_form_id`, `resolve_object`, `MAX_CASCADE`, and — added
2026-08-23/24 — `SceneFragments`, `populate_scene_fragments_from_pex`/
`_from_script`, `scene_fragment_dispatch_system`, `ReferenceEnableState`);
`crates/scripting/src/globals.rs` (`Globals` — read/write surface for
Papyrus `GlobalVariable`, now save-serialized); `crates/scripting/src/recurring_update.rs`
(`recurring_update_tick_system`, `RecurringUpdate`, `OnUpdateEvent`);
`crates/scripting/src/registry.rs` (`ScriptRegistry`).
**Checklist**:
- **Two-phase lock-drop discipline**: `timer_tick_system`,
  `trigger_detection_system`, and `recurring_update_tick_system` each Phase-1
  hold a `query_mut::<T>()`, collect a `Vec` of entities to act on, **`drop()` the
  lock**, then Phase-2 acquire a *different* `query_mut` to insert markers. Verify
  the explicit `drop()` precedes the second acquisition in every one — holding two
  component-mut locks at once forces the TypeId-sorted-acquisition contract and is
  a deadlock vector. `quest_fragment_dispatch_system` clones `QuestStageFragments`
  (`world.resource::<QuestStageFragments>().clone()`) *before* taking the mutable
  resources — the source's own comment says this is deliberate, "to avoid a
  read→write nested resource-lock order" — so only two *resource* locks
  (`QuestStageState` mut + `QuestObjectiveState` mut, via `resource_2_mut`) are
  held across the dispatch loop, not three. Verify the clone-before-lock
  ordering is still there; reintroducing a live `QuestStageFragments` read guard
  held alongside the two mutable ones would reopen the read→write nesting the
  comment says was deliberately avoided.
- **NEW nested-lock surface (2026-07-21, `AddItem`/`MoveTo`; grown 2026-08-24)
  — verify, don't assume**: `apply_effect` now ALSO takes `world: &World` and,
  for the object-targeting variants, acquires a *component* lock
  (`world.query_mut::<Inventory>()` for `AddItem`; `world.get::<GlobalTransform>()`
  then `world.query_mut::<Transform>()` for `MoveTo`) **while the two resource
  locks above are still held** (they're bound in the outer scope for the whole
  `while let Some((quest, stage, is_cascade)) = queue.pop_front()` loop —
  see the cascade-queue bullet above for the `Vec`→`VecDeque` rework). This is
  a real change to the lock-nesting shape this dimension previously described
  as "resource-locks-only, no component lock held across them" — that framing
  is now stale, and `apply_effect`'s own doc comment (as of 2026-08-24) is the
  authoritative running list: it now also names a `Globals` write (1, for
  `SetGlobalValue`) alongside the `PlayerControlState` writes and "12
  component-storage acquisitions" — re-read that doc comment rather than this
  bullet's own count, since it is the thing this bullet is transcribing and
  will drift again. `Effect::Conditional`'s branch recursion (Dim 5) does NOT
  add to this list — it reuses the caller's existing `&mut stages`/
  `&mut objectives` rather than re-acquiring. `scene_fragment_dispatch_system`
  (below) is now a **second caller** of this same `apply_effect`/
  `apply_effects` machinery, under the identical `resource_2_mut::<QuestStageState,
  QuestObjectiveState>()` pattern — the nested-lock safety argument ("only
  safe because every system that touches those quest resources is registered
  `add_exclusive`") must hold for both callers, not just
  `quest_fragment_dispatch_system`; verify `scene_fragment_dispatch_system` is
  also `add_exclusive` in `byroredux/src/boot.rs`. Investigate rather than
  assume safe: (a) does any *other* code path acquire `Inventory`/`Transform`/
  `Globals` first and then try to acquire `QuestStageState`/`QuestObjectiveState`
  — the reverse order — on a path the scheduler could run concurrently with
  either caller; (b) does the engine's scheduler ever run
  `quest_fragment_dispatch_system` or `scene_fragment_dispatch_system`
  concurrently with anything else that touches these same resources/components,
  or with EACH OTHER (both hold the same two resource locks — if the
  scheduler ever parallelizes them the ABBA argument breaks) (check
  `sys.accesses` / the scheduler's declared-access report for both systems —
  Dimension 6's own §"ECS lock held across a second resource/component
  mutation" severity row already rates this class HIGH if it's a real
  deadlock vector, not merely theoretical).
- **Scene-fragment dispatch parallels quest-fragment dispatch (`fragment.rs::
  scene_fragment_dispatch_system`, 2026-08-23, `27875a02`)**: a second
  fragment-execution pipeline, structurally mirroring
  `quest_fragment_dispatch_system` but for SCEN `Begin`/`End`/phase
  lifecycle events instead of QUST stage advances. It drains
  `SceneFragmentInvocationBatch` (Pattern-A, above), looks up each
  invocation's `(scene_form_id, SceneFragmentEvent)` in `SceneFragments`
  (populated at cell load by `populate_scene_fragments_from_pex`/
  `_from_script`, keyed the same conservative way as `QuestStageFragments` —
  same decline-on-unmodeled lowering via `lower_fragment_with_quest_properties`,
  no separate recognizer), and applies via the SAME shared `apply_effects`/
  `DeferredFragmentEffects` used by quest fragments — any `SetStage` a scene
  fragment performs enters the canonical `QuestStageAdvancedBatch` sink exactly
  like a quest fragment's would. Scheduling order in `byroredux/src/boot.rs` is
  load-bearing and explicitly documented in-line: `scene_playback_system` →
  `scene_fragment_dispatch_system` → … → `quest_fragment_dispatch_system`
  (called `quest_fragment_dispatch` there), all `add_exclusive` in
  `Stage::Update` — a scene fragment's `SetStage` this frame is guaranteed
  visible to `quest_fragment_dispatch_system` the SAME frame, not the next.
  Verify that ordering hasn't drifted (a reorder would turn a same-frame
  cascade into a one-frame-late one, silently) and that `SceneFragments` is
  populated idempotently across repeated cell loads the same way
  `QuestStageFragments` is (Dim 5's multi-fragment-per-stage-merge bullet is
  QUST-side only — `SceneFragments::insert` has no analogous merge, it's one
  binding per `(scene, event)` by construction since a scene event has at
  most one fragment in the VMAD format, so confirm that premise still holds
  rather than assuming it from this note).
- **Marker lifetime: two sanctioned patterns, not one (#2672 — predates this
  session but this bullet's "MUST be drained by `event_cleanup_system`" framing
  never reflected it; correcting it now because the new markers below land in
  both patterns)**: `cleanup.rs`'s module doc names two legitimate marker
  lifecycles. **Pattern A** — registered in `event_cleanup_system`'s drain
  list, for a marker with no single owning consumer: `ActivateEvent`,
  `HitEvent`, `TimerExpired`, `AnimationTextKeyEvents`, `OnUpdateEvent`,
  `QuestStageAdvancedBatch` (the actual drained `Component`; the bare
  `QuestStageAdvanced` struct it wraps carries no `Component` impl and is
  never inserted directly — and since **#3277** (`26f8738d`) **every producer
  must go through `push_quest_stage_advances` (`quest_stages.rs`), never a bare
  `insert`**: the batch is one `SparseSetStorage` slot on one shared entity, so
  an `insert` replaces it and silently drops any same-frame producer's events.
  #1864 documented that but left it a convention each writer re-implemented —
  five of six open-coded `get_mut`-then-`extend` correctly and
  `quest_fragment_dispatch_system`'s tail did not, which was harmless only
  while it happened to be scheduled last, and stopped being harmless once
  `quest_alias_readiness_stage_system` and `scene_fragment_dispatch_system`
  were scheduled ahead of it. All six writers now route through the helper, and
  the helper's own `insert` is the only one left. It deliberately carries **no**
  emptiness guard — every call site already returns early on an empty batch —
  so adding one is dead code that reads as load-bearing. Regression = a seventh
  producer inserting directly. Guard:
  `push_quest_stage_advances_merges_same_frame_producers`), the rumble/camera/UI command markers,
  `SceneEventBatch`, `SceneFragmentInvocationBatch` (added 2026-08-23,
  `27875a02` — the invocation marker `scene_fragment_dispatch_system`
  consumes), `OnTriggerEnterEvent`, `OnCellLoadEvent`, `OnEquipEvent`.
  **Pattern B** — drained unconditionally at the head of the *one* owning
  consumer instead, a stronger same-frame guarantee: `SceneStartRequest`/
  `SceneStopRequest`/`SceneActionCompletionBatch` (`scene_playback_system`),
  `DialoguePresentationEventBatch`/`DialogueLineCompletionBatch`
  (`dialogue::scene_dialogue_system`), `ScenePackageEventBatch`/
  `ScenePackageCompletionBatch`/`EvaluatePackageRequest`
  (`package::scene_package_system`), `TwoStateTransitionBatch`
  (`vm_state::two_state_activator_system`), `MotionTypeChangeRequest`
  (`byroredux::systems::cinematic`). For a Pattern-A marker, verify
  `event_cleanup_system` drains EVERY one the runtime emits (cross-check the
  `cleanup.rs` drain list against every `world.insert` of a marker across the
  crate) and that `cleanup` is the LAST scripting system in the schedule. For
  a Pattern-B marker, verify the drain is unconditional — no early return may
  sit between the top of the owning system and the drain, or the marker is
  stranded and its consumer re-fires forever. A marker in neither list (new
  or renamed) is the actual bug to report — not "which list should it be in"
  in the abstract, but whether its *actual* drain site matches whichever
  pattern its docstring claims. Guards: `cleanup_removes_all_event_types`,
  `cleanup_preserves_non_event_components`, and `cleanup.rs`'s own
  `every_drained_marker_is_a_documented_pattern_a_marker` contract test.
- **Producer→consumer cross-stage ordering**: `quest_advance_system` (Dim 7)
  and `quest_startup_system`/`quest_alias_readiness_stage_system` (above)/
  `quest_fragment_dispatch_system` itself all emit `QuestStageAdvanced`;
  `quest_fragment_dispatch_system` consumes every source and may re-emit
  (cascade). The cascade queue was
  reworked 2026-08-24 (`25a0aabd`): it is now a `VecDeque<(QuestFormId, u16,
  bool)>` (FIFO `push_back`/`pop_front`, not the prior `Vec`
  `push`/`pop` LIFO stack), and the `bool` (`is_cascade`) distinguishes
  authored ingress (journal/legacy-batch events, pushed `false`) from a
  `SetStage` a fragment itself emitted (pushed `true`). `MAX_CASCADE = 64`
  (`cascade_steps`) now bounds **only** `is_cascade == true` entries — the
  prior scheme counted every dequeue including ingress, which the commit's
  own comment flags as a false-cap risk ("a Skyrim bootstrap can legitimately
  deliver hundreds of independent Start Game Enabled quest events in one
  tick"). Verify (a) `cascade_steps` is genuinely gated on `is_cascade` and
  not incremented for plain ingress; (b) a WARN still fires on overflow (an
  unbounded `SetStage`→fragment→`SetStage` loop hangs the frame); (c) only
  *genuine* transitions cascade (a no-op re-set of the same stage —
  `adv.previous_stage == adv.new_stage` — is skipped, not re-queued); (d) the
  FIFO reorder is intentional and doesn't invert an ordering assumption a
  test or a fragment author relies on (the old LIFO `pop` processed the most
  recently queued cascade continuation before older sibling ingress; the new
  FIFO processes strictly in arrival order, so a cascade continuation now
  runs *after* every currently-queued sibling rather than immediately).
- **CTDA OR-precedence (`condition.rs::evaluate`)**: Bethesda's **inverted**
  precedence — consecutive `or_next`-flagged conditions form an OR block that
  binds *tighter* than the surrounding AND chain (`A AND B OR C AND D` =
  `A AND (B OR C) AND D`). The block scan walks while `conditions[i].or_next`,
  OR-combines the block with `.any()`, AND-combines blocks with early-return on a
  false block. Verify the block-boundary logic (the last condition of a block has
  `or_next == false`) and the empty-list → `true` contract. Guards:
  `or_precedence_quirk_a_and_b_or_c_and_d_groups_b_or_c`,
  `or_precedence_quirk_swap_test_a_true`, `and_chain_short_circuits_on_first_false`,
  `or_block_returns_true_when_any_member_true`, `empty_condition_list_returns_true`.
- **Condition stubs are KNOWN (#1663–#1668, #1316)**: `GetActorValue`/`GetDistance`/
  `GetFactionRank`/`GetIsID`/`HasPerk` return documented safe-defaults (the
  Bethesda "unknown-function safe-default" / "not in faction" = -1.0 sentinels).
  Do NOT re-file these. DO verify the *safe-default values* are correct (a wrong
  sentinel flips a condition) and that `RunOn` resolution declines (condition
  fails) on an unresolvable target rather than defaulting to subject.
- **Edge-triggered trigger detection (`trigger.rs`)**: `trigger_detection_system`
  fires `OnTriggerEnterEvent` ONLY on the outside→inside transition
  (`inside && !was_inside`), updates `occupant_inside` each frame, fires
  again on re-entry. `occupant_inside` is `Option<bool>`, not a bare `bool` —
  `None` means "never checked" and the seed contract is enforced by *skipping*
  the enter check entirely on that first tick (SCR-D6-NEW-02/#1817), not by
  seeding a synthetic `Some(true)`/"was inside" default: a fresh volume writes
  `Some(inside)` without ever comparing against the `None`. This player-only
  path is unchanged; verify the seed contract (a player loaded already inside
  a volume must NOT spuriously fire on frame 1 — the `None` branch never
  pushes to `entered`) and the `contains` math: Sphere =
  `(p-center).length_squared() <= r*r` with `half_extents.x` as radius; Box
  (OBB) = `rotation.inverse() * (p-center)` then per-axis
  `local.abs() <= half_extents`. Guards: `edge_triggered_not_level_triggered`,
  `re_entry_fires_again`, `sphere_contains_by_radius`, `obb_rotation_is_respected`,
  `aabb_contains_interior_and_rejects_exterior`.
- **`OnTriggerEnterEvent` is now multi-triggerer (2026-08-24, `7473a387`) —
  the "the event lands on the volume entity with THE triggerer in the marker
  field" framing above is stale for the field shape**: the component's field
  is `triggerers: Vec<EntityId>` (was a single `triggerer: EntityId`).
  `trigger_detection_system` now scans TWO independent populations per
  volume in the same frame: the player (via `occupant_inside`, unchanged) and
  every non-player `crate::scene::SceneAliasCandidate` entity with a
  `GlobalTransform`, tracked in a NEW `TriggerOccupancyState` resource keyed
  `(trigger, actor) -> bool` (a sparse side table — player occupancy stays on
  `TriggerVolume.occupant_inside` for save compatibility, per the file's own
  comment). If the volume already carries an event this frame, a new
  triggerer is appended (`if !event.triggerers.contains(&triggerer)`) rather
  than overwriting — verify no path still assumes singular delivery (e.g. a
  consumer that reads `triggerers[0]` and ignores the rest would silently
  drop simultaneous multi-actor crossings). A tethered horse (present in
  `crate::HorseTetherState`) is tested via `TriggerVolume::intersects_sphere`
  (a body-radius contact test, `TETHERED_HORSE_TRIGGER_RADIUS = 96.0`) instead
  of the point-containment `contains` the player/other actors use — verify
  the sphere math (Sphere: combined-radius distance check; Box (OBB): closest-
  point-then-radius, both in the volume's local rotated space) and that ONLY
  tethered horses get the sphere widening (a non-mover actor using
  `intersects_sphere` would fire triggers it hasn't actually reached). Three
  re-fire conditions feed `entered`, not just the plain edge
  (`was_inside == Some(false)`): a first-observed tethered horse already
  inside on a freshly-streamed volume (`was_inside.is_none() && active_mover`
  — the deliberate exception to the player-side "first tick never fires"
  seed contract, justified because exterior streaming can materialize a
  trigger around an already-moving native actor rather than that actor
  crossing an edge), and `became_ready_inside` — a `BaseForm`-gated trigger
  a tethered horse is ALREADY inside (`was_inside == Some(true)`) re-fires
  once its `QuestAdvanceOnActivate` gate becomes newly satisfiable (target
  stage not yet done AND its `conditions` now evaluate true), so a horse that
  entered before the quest was ready gets a fresh entry the moment it
  becomes ready rather than being stuck un-signaled. Verify `became_ready_inside`
  re-evaluates every frame while stuck-inside-and-not-yet-ready (expected —
  it's a level condition, not an edge) but stops firing once the target stage
  is actually set (checked via `get_stage_done`, so this is bounded by how
  fast the consumer applies the `SetStage`, not by this system) — and that
  `occupancy.inside.retain(|key, _| observed.contains(key))` at the end of
  the actor scan correctly prunes `(trigger, actor)` keys for actors that
  streamed out or despawned, so `TriggerOccupancyState` doesn't grow
  unboundedly over a long play session. Guards:
  `quest_actor_crossing_emits_triggerer_identity`,
  `freshly_streamed_trigger_emits_for_tethered_horse_already_inside`,
  `tethered_horse_inside_reemits_when_quest_prerequisite_becomes_ready`,
  `preserves_all_actors_entering_one_volume_in_the_same_frame`,
  `actor_sphere_contacts_box_even_when_root_point_misses` (`crates/scripting/src/trigger.rs`).
- **`actor_quest_trigger_is_in_sequence` (`trigger.rs`, 2026-08-24, `cee35507`)
  — a second, independent gate layered on top of the above**: after
  `entered` is computed, it's filtered through this function before any
  `OnTriggerEnterEvent` is emitted, so an actor genuinely crossing (or a
  tethered horse becoming ready) can still be held back. Only applies to
  `BaseForm`-gated triggers (`ActivatorGate::BaseForm`) — anything else
  passes through (`return true`) unfiltered. Two regimes, keyed off whether a
  `ScenePlayer` for a scene owned by the trigger's `owning_quest` is
  currently running: (1) **during a running scene** — only triggers whose
  `target_stage` is `<=` one of the CURRENT phase's `GetStageDone(quest,
  stage) == 1` completion-condition stages may fire (`awaited_stages`,
  scraped from `ScenePhase::completion_conditions` by literal CTDA shape —
  function 59, `Eq`, comparand `1.0`, `param_1 == owning_quest`); (2)
  **between scenes** (the owning scene has finished and none is running) —
  only the numerically LOWEST `target_stage` among all `BaseForm`-gated
  triggers for that quest that is `>= current_stage`, not yet
  `get_stage_done`, AND whose own `conditions` currently evaluate true may
  fire (`next_ready == Some(advance.target_stage)`) — i.e. strict monotonic
  ordering between scenes, not "any ready trigger". If NEITHER a running nor
  a finished scene is found for the quest, the gate is a no-op (`return
  true`). This duplicates — as a SEPARATE implementation — the "which
  trigger is next" logic `byroredux/src/systems/cinematic.rs`'s
  `scene_trigger_actor_approach_system_inner` (Dim 8) uses to pick where to
  *route* a tethered horse; verify the two never disagree (a horse routed
  toward a trigger this gate would then refuse to fire is a real, silently-
  broken cart sequence — cross-reference Dim 8). Guard:
  `actor_triggers_follow_scene_phase_and_between_scene_stage_order`
  (`crates/scripting/src/trigger.rs`).
- **`QuestAliasReadinessGate` (`quest_stages.rs`, 2026-08-23, `eb2e2445`)**:
  an engine-authored substitute for a quest-alias script's own `SetStage`
  call — `install_quest_alias_readiness_gate` registers `(quest,
  required_aliases, target_stage, only_below_stage)`;
  `quest_alias_readiness_stage_system` (scheduled in `Stage::Update` right
  after `quest_alias_refresh_system`, before `scene_playback_system`) advances
  the quest to `target_stage` the frame every `required_aliases` entry first
  resolves through `SceneActorBindings`, mirroring Skyrim's
  `RegisterStartingCellLoad` same-frame callback timing. Verify the three
  guard conditions in `quest_alias_readiness_stage_system` all hold before
  advancing: `stages.is_running(quest)`, `get_stage(quest) < only_below_stage`
  (an already-advanced-past quest must NOT be pulled backward or re-fired),
  and `!get_stage_done(quest, target_stage)` (idempotent — a gate that has
  already fired must not re-fire every frame just because all its aliases
  remain bound). `install_quest_alias_readiness_gate` is upsert-by-quest
  (one gate per quest, last-installed-wins on a repeat call with the same
  `quest`) — verify that's the intended shape for a quest with multiple
  independent alias-readiness triggers, or confirm it's documented as
  one-gate-per-quest by design. Guard:
  `alias_readiness_gate_advances_once_after_every_alias_binds`
  (`crates/scripting/src/quest_stages.rs`).
- **Quest stage history (`quest_stages.rs`)**: `set_stage` updates `current_stage`
  AND inserts into `stages_done` (history retained across advances —
  `GetStageDone(37)` stays true after advancing to 40); `set_stage` returns the
  previous current; backward set is allowed; `reset` clears one quest only. Guards:
  `get_stage_done_retains_history_across_advances`,
  `set_stage_on_already_done_stage_remains_idempotent`, `reset_leaves_other_quests_intact`.
- **`recurring_update_tick_system`**: a fresh `RecurringUpdate` does NOT fire on
  the registering frame / zero dt; fires once per interval; re-arms after fire; a
  long-frame dt overshoot fires once (not a burst); `UnregisterForUpdate` inside a
  handler terminates cleanly. Guards: `fresh_subscription_does_not_fire_on_zero_dt`,
  `dt_overshoot_fires_only_once_per_tick`, `subscription_re_arms_after_fire`,
  `unregister_inside_handler_terminates_cleanly` (`crates/scripting/src/recurring_update/tests.rs`).
- **`ScriptRegistry`** (M47.0 static path, being retired in favor of the dynamic
  attach): case-SENSITIVE editor-id keys, re-register replaces. Verify no live
  call path still depends on the hardcoded `papyrus_demo::register_spawners` for a
  vanilla REFR (the demos should be test fixtures only — `m47-2-design.md` §"Engine
  integration" says the hardcoded registration is retired). Flag a surviving
  hardcoded-attach call site as a tech-debt / correctness mismatch.
**Output**: `/tmp/audit/scripting/dim_6.md`

### Dimension 7: Engine Attach Path & Trigger-Volume Wiring (engine-side)
**Entry points**: `byroredux/src/cell_loader/references/attach.rs` (`attach_vmad_scripts`,
`attach_script_for_refr`, `trigger_volume_from_primitive`, the invisible-trigger
REFR spawn path — split out of `references/mod.rs` under #1877, which now only
re-exports them and keeps their call sites); `crates/plugin/src/esm/records/index.rs`
(`base_record_script_instance`); `crates/plugin/src/esm/records/script_instance.rs`
(`ScriptInstanceData`, `ScriptInstance`); `byroredux/src/asset_provider/script.rs`
(`build_script_provider`, `extract_pex`, the `--scripts-bsa` parse);
`crates/scripting/src/papyrus_demo/quest_advance.rs` (`quest_advance_system`,
`QuestAdvanceOnActivate`, `ActivatorGate` incl. `ActivatorGate::BaseForm`
(2026-08-24), `QuestTriggerApproachRegistry`/`QuestTriggerApproach`/
`install_quest_trigger_approach` — the process-lifetime catalog of
actor-gated triggers whose cells may not be resident, consumed by
`byroredux/src/systems/cinematic.rs`'s `scene_trigger_actor_approach_system_inner`,
Dim 8); `byroredux/src/cell_loader/references/mod.rs`
(`stamp_quest_reference`, `spawn_logical_quest_reference`,
`attach_quest_reference_script` — added 2026-08-07, `a844c26b`, "integrate
canonical reference identities through cell loading"); `byroredux/src/commands/quest.rs`
(the M47.3 debug-console surface: `QuestStartCommand`/`QuestSetStageCommand`/
`QuestAliasesCommand` and, added 2026-08-23, `SceneShowCommand`
(`scene.show <formid>`) — reports a SCEN's live `ScenePlayer`/
`ScenePackagePlayback` state plus authored phase/action data side by side).
**Why this dimension**: the decompiler + recognizer chain (Dims 1–5) are the
*producer* of canonical components; the cell-loader attach path is the only live
*driver* that feeds them real VMAD + `.pex` from game data. None of the crate
dimensions covers it.
**Checklist**:
- **Silent-miss everywhere (graceful degradation)**: the attach path must NEVER
  panic on missing data — no `--scripts-bsa` (early out), VMAD absent
  (`base_record_script_instance` → `None` → return), `.pex` not in archive
  (`extract_pex` → `None`, trace-log, continue), parse/decompile fail
  (`translate_pex` → `None`, debug-log), recognizer miss (trace-log). Verify every
  branch is a `continue`/`return false`, not an `unwrap`/`expect`. Untrusted-Input:
  Yes (the `.pex` bytes come from a possibly-modded archive).
- **VMAD retention + accessor (`index.rs::base_record_script_instance`)**: checks
  **seven** arms in order — ACTI/CONT/NPC/CREA, then (#2189) the item family
  (WEAP/ARMO/AMMO/MISC/KEYM/ALCH/INGR/BOOK/NOTE, via `self.items.get(base_form_id)
  .and_then(|r| r.common.script_instance.as_ref())`), then (#2663) the
  MODL-only statics family (`self.cells.statics` —
  STAT/MSTT/FURN/DOOR/LIGH/FLOR/IDLM/BNDS/ADDN/TACT), then (#2663)
  `self.terminals` (FO4 ships 207 VMAD-bearing TERM records) — returning the
  first hit. Both #2663 additions have their own guards
  (`base_record_script_instance_resolves_a_statics_familys_vmad`,
  `…_resolves_a_terminals_vmad`); verifying "covered == VMAD-bearing set"
  against only the first five arms re-derives #2663 as a false gap. Confirm the
  accessor is keyed by `base_form_id` (the REFR's base, not the REFR's own form
  id) and that a REFR's *own* VMAD (Skyrim+ supports per-REFR scripts) is also
  resolved — flag if only base-record VMAD is consulted (per-REFR override
  scripts would be dropped). Guard: `base_record_script_instance_resolves_an_item_records_vmad`.
- **`.pex` resolution (`asset_provider/script.rs`)**: `extract_pex` normalizes a
  VMAD script name → `scripts\<name>.pex` (backslash, lowercase, `scripts\`
  prefix). `--scripts-bsa` is **repeatable, first-listed-archive-hit-wins**
  (searched in flag order, first hit returned) — this is the documented
  *inverse* of typical mod-manager load order (there, later = higher
  priority): list override/mod archives *before* the vanilla one on the
  command line (#1743/SCR-D7-03). Verify the path normalization matches the
  on-disk archive convention (a wrong prefix/separator → every `.pex` miss →
  zero scripts attach silently) and that the iteration order actually
  implements first-listed-wins, not the reverse.
- **XPRM → `TriggerVolume` half-extent convention** (`trigger_volume_from_primitive`):
  XPRM `bounds` are Bethesda **z-up HALF-extents** (CK Primitive convention,
  consistent with `bhkBoxShape` half-extents) — the code must NOT divide by 2.
  Verify (a) no `/ 2.0`; (b) the z-up→y-up permute is `[x, z, y]` (bounds[0],
  bounds[2], bounds[1]) with `.abs()` (extents are magnitudes); (c) the REFR
  `scale` is baked in (world-space volume); (d) sphere uses `bounds[0]` as radius
  into `half_extents.x`; (e) shape dispatch `1 → Box`, `3 → Sphere`, other →
  `None` (line/portal/plane are non-containment). A wrong half/full or a wrong
  permute makes every trigger box the wrong size/shape → quests fire at the wrong
  position or never. Guards: `trigger_volume_from_box_primitive_permutes_and_scales`,
  `trigger_volume_from_sphere_primitive_uses_radius`.
- **Invisible (MODL-less) trigger REFR spawn**: a scripted trigger REFR with no
  mesh spawns an entity with `Transform`/`GlobalTransform`/`TriggerVolume` (no
  render component) and attaches its script. Verify the volume is built in
  **world space** (REFR position + rotation + scale composed once at load), so
  `trigger_detection_system` can test against the post-propagation player
  `GlobalTransform` without per-frame composition.
- **`quest_advance_system` unifies OnActivate + OnTriggerEnter**: both
  `ActivateEvent` (doors/levers, `activator` field) and `OnTriggerEnterEvent`
  converge on one `QuestAdvanceOnActivate` component. `OnTriggerEnterEvent`'s
  field is now `triggerers: Vec<EntityId>` (2026-08-24, Dim 6) — this system's
  own collect loop already handles that plural shape correctly
  (`triggered.extend(ev.triggerers.iter().map(|triggerer| (entity, *triggerer)))`,
  one `(entity, triggerer)` pair per triggerer), so the "the design relies on a
  given entity receiving only one signal" framing this bullet used to carry is
  now ALSO about "only one signal *source*" (Activate xor TriggerEnter), not
  "only one triggerer" — a trigger volume legitimately fans out to several
  `(entity, triggerer)` pairs from one `OnTriggerEnterEvent` in one frame now,
  and each is evaluated independently against `QuestAdvanceOnActivate`'s
  single `activator_gate`/`conditions`. Verify nothing can deliver both an
  `ActivateEvent` AND an `OnTriggerEnterEvent` to one entity in one frame
  (double-advance) — the plural triggerers change doesn't affect this
  cross-source invariant, only the trigger-side fan-out. Confirm condition
  gating runs per `(entity, triggerer)` pair (`ConditionContext::for_subject`
  + `evaluate_condition_list`) and that the gate — now three-way
  (`ActivatorGate::Any`/`PlayerOnly`/`BaseForm(u32)`, the last added
  2026-08-24, matched against the triggerer's `SceneAliasCandidate::base_form_id`)
  — is honored per-triggerer, not just for the first one collected. Guards:
  `trigger_enter_advances_quest`, `trigger_enter_respects_player_only_gate`,
  `activate_and_trigger_in_same_frame_both_advance`
  (`crates/scripting/src/papyrus_demo/quest_advance/tests.rs`).
- **Canonical reference identity stamping (`stamp_quest_reference`,
  2026-08-07)**: every synthetic REFR-load path (NPC actor, invisible
  trigger volume, missing-mesh/logical, and static-mesh) now calls
  `stamp_quest_reference` — a `FormIdComponent` + `SceneAliasCandidate`
  (Dim 5/6's alias-fill input, `crates/scripting/src/scene.rs`) stamp,
  followed by `mark_scene_actor_bindings_dirty` — widened from the
  NPC-only stamping this dimension previously covered. Verify the
  `is_primary_synth`/`synth_idx == 0` gate is applied at every call site: a
  SCOL/PKIN-expanded placement fans one authored REFR into N synthetic
  children, and only the first (`synth_idx == 0`) may carry the REFR's own
  canonical identity — stamping it on every fanned-out sibling would
  register N `SceneAliasCandidate`s for one authored alias-fillable
  reference (a many-candidates-for-one-alias correctness bug, not a
  decompiler-domain issue but a real hazard for `SceneActorBindings`'s
  fill logic in Dim 5/6). Confirm `spawn_logical_quest_reference` (the
  no-mesh/stat-miss fallback path) still spawns a `Transform`/
  `GlobalTransform`-bearing entity even with no renderable mesh, so a
  quest-alias-only REFR (e.g. an unmeshed quest marker) isn't silently
  dropped from alias-fill candidacy just because it has nothing to render.
- **The `M47.2 scripts:` cell-load summary**: the smoke gate
  `docs/smoke-tests/m47-triggers.sh` keys on the `N REFRs recognized, M trigger
  volumes spawned` line. Verify the counters are wired (recognized++ on a
  `translate_pex` Some, trigger_volumes++ on a volume spawn) so the smoke harness
  has a real signal — a counter that never increments makes the gate vacuous.
- **`scene.show` debug command (`SceneShowCommand`, `byroredux/src/commands/quest.rs`,
  added 2026-08-23)**: a diagnostic-only command (not on any live game-state
  write path — `execute` takes `&World`, not `&mut World`) that renders a
  SCEN's authored definition next to its live `ScenePlayer`/
  `ScenePackagePlayback` state and resolves each authored actor alias through
  `SceneActorBindings`. Lower audit priority than the write-path bullets above
  since it can't corrupt game state, but verify it stays read-only (a
  diagnostic command that mutates would be a much higher-severity finding
  given it's reachable from the debug console) and that the alias resolution
  it displays (`bindings.resolve(quest, actor.actor_id as i32)`) uses the same
  entry point `fragment.rs::resolve_object`/`condition.rs::RunOn::QuestAlias`
  do (Dim 5/6), so a debugging session against this command's output isn't
  looking at a different resolution path than the one actually driving
  gameplay.
**Output**: `/tmp/audit/scripting/dim_7.md`

### Dimension 8: Havok Idle / Cinematic Slice — `.hkx` Decode → Playback (added 2026-08-13)
**Entry points**: `crates/hkx/src/packfile.rs` + `crates/hkx/src/animation.rs`
(`decode_skeleton`, `decode_spline_animation`, `HkxSkeleton`, `HkxBone`,
`HkxAnimation`, `HkxTransform`, `HkxAnnotation`);
`byroredux/src/asset_provider/animation.rs` (`populate_havok_idle_runtime`,
`convert_hkx_clip`, `idle_animation_candidates`, `behavior_completion_events`) —
the crate's **only** consumer; `crates/scripting/src/cinematic.rs`
(`HorseTetherState`, `ActorCinematicState`) and
`byroredux/src/systems/cinematic.rs` (`havok_idle_playback_system`,
`cinematic_root_motion_system`, `cinematic_animation_event_system`,
`scripted_motion_type_system`, `vehicle_attachment_system`, and — added
2026-08-24, `7473a387`/`5f38402e` — `scene_trigger_actor_approach_system_inner`,
new in this range, not a pre-existing function this dimension previously
covered); `byroredux/src/cell_loader/unload.rs` (`cinematic_retained_entities`,
also added 2026-08-24).
**Why this dimension exists**: the M47.2 MQ101 cart cinematic is the first
scripted sequence that drives *animation* rather than ECS state, and it crosses
three previously-unaudited surfaces — an untrusted binary parser (`hkx`), an
asset-resolution catalog, and five playback systems. Dims 1–7 cover none of it.
**Checklist**:
- **Untrusted binary input.** `crates/hkx` is a deliberately safe reader (no
  `unsafe`, no behavior-graph execution). Apply the same discipline `/audit-nif`
  Dim 1 applies: every offset read is bounds-checked, a lying count cannot
  pre-allocate unbounded memory, and a malformed packfile returns `Err` rather
  than panicking. Confirm the "no behavior-graph execution" scope claim still
  holds — executing a guest behavior graph would be a categorically different
  trust surface.
- **Spline decompression**: `decode_spline_animation` handles static *and*
  dynamic transform tracks. Verify the static/dynamic split is driven by the
  file's own flags, that the quantization decode matches the documented Havok
  2010 layout, and that a track count mismatch against `HkxSkeleton` is rejected
  rather than zip-truncated (a silently short zip is a limb frozen at bind pose).
- **Bone binding**: `convert_hkx_clip` maps Havok bone names onto the engine
  skeleton. Verify unmatched bones are reported, not dropped silently, and that
  the Z-up→Y-up conversion happens exactly once (the same double-convert trap
  `/audit-physics` Dim 4 checks on the ragdoll side).
- **Catalog resolution**: `idle_animation_candidates` builds a name-candidate
  list per idle event. Verify a miss is diagnosable and that the candidate order
  is deterministic — a set/hash-ordered candidate list makes playback
  irreproducible run-to-run.
- **Playback lifecycle**: `havok_idle_playback_system` is documented to start a
  scoped player **once per serial** (guard:
  `idle_request_starts_scoped_havok_player_once_per_serial`). Verify the request
  is drained after consumption, so a stuck request cannot restart the clip every
  frame.
- **Root motion**: `cinematic_root_motion_system` applies and then drains a
  delta (guard: `cart_exit_root_motion_moves_and_orients_actor_then_drains_delta`).
  Verify apply-then-drain ordering — an undrained delta integrates every frame
  and launches the actor.
- **Completion events**: `behavior_completion_events` /
  `cinematic_animation_event_system` translate clip annotations into scripted
  completions. Verify an annotation the catalog doesn't know is ignored safely
  and that a missing completion event cannot deadlock a quest stage waiting on it
  — cross-reference Dim 6's quest-stage gating.
- **Attachment**: `vehicle_attachment_system` / `scripted_motion_type_system`
  reparent and re-classify bodies mid-sequence. Verify the motion-type flip is
  the same `Keyframed` discipline `byroredux/src/npc_spawn.rs` uses for live
  ragdoll bones — cross-reference `/audit-physics` Dims 3–4 and report the
  physics half there.
- **`scene_trigger_actor_approach_system_inner` (new, 2026-08-24; body of the
  closure `make_scene_trigger_actor_approach_system` returns, which is what
  `boot.rs` registers — renamed by #3838 when it gained persistent scratch)**: routes an
  offscreen actor-gated trigger's approach target for cataloged (not
  necessarily cell-resident) triggers registered via
  `QuestTriggerApproachRegistry` (Dim 7). Computes, per quest with a live
  `ScenePlayer`, either the CURRENT scene phase's awaited `GetStageDone`
  stages (`awaited`) or — new this commit, for quests **between** scenes
  (scene `Finished`, none `is_running`) — a `u16::MAX` sentinel cap meaning
  "any `BaseForm`-gated `target_stage >= current_stage`" (`between_scenes`).
  For each cap it picks, per candidate base-form actor, the single
  LOWEST-stage reachable trigger (`min_by_key`) — and for the `u16::MAX`
  between-scenes case, an extra `retain` narrows the whole candidate set down
  to only the globally-lowest `stage` found across all bases, so it never
  routes an actor toward a stage 2+ triggers ahead of the true next one. This
  is a SEPARATE reimplementation of the same "what's the next allowed
  `BaseForm` trigger stage for this quest" question `trigger.rs`'s
  `actor_quest_trigger_is_in_sequence` (Dim 6) answers to decide whether to
  fire a trigger. **Verify the two agree** — trace both against the same
  scene-phase/between-scenes inputs and confirm they'd always pick/allow the
  same stage; a drift means the horse can be routed toward (or past) a
  trigger the OTHER function would then refuse to fire, silently breaking the
  cart sequence with no panic or error to surface it. Guards: the
  `mod tests` block in `byroredux/src/systems/cinematic.rs` (search
  `scene_trigger_actor_approach_system_inner` — no single canonical test name is
  documented here, confirm current coverage directly).
- **`cinematic_retained_entities` (`byroredux/src/cell_loader/unload.rs`,
  2026-08-24; reshaped by two 2026-09-03 fixes, #3690 `3f213038` and #3254
  `90f81e8e` — the single-function framing this bullet used to carry is
  stale, it's now split across two functions with a fixed scope bug)**:
  `cinematic_retained_entities` (the SCAN half — unchanged in spirit)
  collects every entity reachable from a live `HorseTetherState` (`cart`,
  `tether.horse`) or `ActorCinematicState.vehicle` (`actor`, `vehicle`),
  then transitively walks `Children` from that seed set so a retained
  root's whole render/bone hierarchy survives too; `unload_cells` (the
  whole-batch caller) now computes it exactly **once** for the whole unload
  batch (#3690 — it was previously recomputed once per victim cell, a
  whole-world scan repeated up to 121× for a `DEFAULT_TRANSITION_RADIUS=5`
  door transition) and passes the result down. The `CellRoot`-strip half is
  now a separate function, `strip_retained_cell_root(world, victims,
  retained)` (renamed from the #3690-extracted *release_cinematic_retention*),
  called from `unload_cell_inner` **per cell**, scoped to `retained ∩
  victims` — **fixing a real bug** (#3254): the strip used to run against
  every retained entity in the WHOLE WORLD from wherever it was called, so
  since `HorseTetherState` is never cleared anywhere in production code (see
  below), the very first unload anywhere after a cart was tethered
  permanently orphaned it (its `CellRoot` never came back, rendered forever,
  GPU handles held forever) — don't re-flag this specific mechanism as a
  new finding, it's fixed. Verify (a) the walk is genuinely transitive (a
  grandchild two `Children` hops from the horse root must be retained, not
  just direct children — guard: `active_tether_retains_horse_cart_rider_and_hierarchy`);
  (b) `strip_retained_cell_root` only strips `CellRoot` from entities that
  are BOTH currently retained AND victims of the specific cell unloading
  right now (guard:
  `strip_leaves_a_retained_entity_untouched_when_it_is_not_a_victim_of_this_cell`,
  `byroredux/src/cell_loader/unload.rs`); (c) stripping
  `CellRoot` doesn't orphan the entity from some OTHER index that still
  expects every live entity to carry a `CellRoot`; (d) **the retention set's
  lifetime is a KNOWN, currently-open gap, not a hypothesis to
  re-discover**: `HorseTetherState`/`ActorCinematicState.vehicle` are never
  cleared anywhere in production code today, so once tethered, an entity is
  retained (and, post-#3254, correctly re-adopts `CellRoot` on return to its
  origin cell rather than being globally orphaned) but never returns to
  ordinary cell-scoped lifetime — tracked as **open** issue #3817 ("ECS
  followup: HorseTetherState/ActorCinematicState never terminate, so
  cinematic-retained entities have no re-adoption path"), explicitly
  deferred by #3254's own commit message pending research into the vanilla
  cart script. Cite #3817 rather than re-filing a duplicate; do verify it's
  still open before citing it as settled going forward. Cross-reference the
  completion-event bullet above, so a finished cinematic's actors return to
  normal cell-scoped
  unload rather than being retained forever as a leak.
**Output**: `/tmp/audit/scripting/dim_8.md`

## Phase 3: Merge

1. Read all `/tmp/audit/scripting/dim_*.md` files.
2. Combine into `docs/audits/AUDIT_SCRIPTING_<TODAY>.md` with structure:
   - **Executive Summary** — what shipped (M30.2 `.psc` parser; M47.0 event
     hooks; M47.1 condition eval; M47.2 `.pex` reader + 5-phase decompiler +
     recognizer chain + dynamic attach path + XPRM trigger volumes + the
     fragment-lowerer wired-and-live-verified dispatch + the QUST VMAD
     property-table fix + the `AddItem`/`MoveTo` object-targeting effects, all
     2026-07-21; plus M47.3 quest-alias-fill Phases 0–3 — `SceneActorBindings`
     alias resolution, alias-injected faction/inventory application, the
     permanent inventory-grant save ledger, and alias-bound
     `ObjectRef::Property`/`RunOn::QuestAlias` resolution — and the
     quest-lifecycle effects (`Start`/`Stop`/`CompleteQuest`/`Reset`/
     `SetActive`/`FailAllObjectives`), all 2026-08-07; plus, 2026-08-23/24 (six
     same-day commits — verify each against current source, not this
     summary): scene-lifecycle fragment dispatch
     (`SceneFragments`/`scene_fragment_dispatch_system`, mirroring
     quest-fragment dispatch for SCEN `Begin`/`End`/phase events);
     actor-specific trigger gating (`ActivatorGate::BaseForm`,
     `QuestTriggerApproachRegistry`) and tethered-horse trigger detection
     (multi-triggerer `OnTriggerEnterEvent`, `TriggerVolume::intersects_sphere`,
     the scene-phase/between-scenes `actor_quest_trigger_is_in_sequence` gate,
     and its Dim-8 navigation counterpart `scene_trigger_actor_approach_system_inner`);
     `ReferenceEnableState` + the `Disable` fragment effect (BOTH the
     alias-aware-receiver half and the runtime-consumer half are now CLOSED,
     #3278, `26f8738d`+`265f0c9b` — see Dim 5's own bullet for the mechanism;
     **do not describe this as "no production consumer yet", that framing is
     stale**); `Effect::Enable` (added 2026-09-02, Fix #3489, `prim_enable`
     — the counterpart `Disable` shipped without, mirroring its receiver
     treatment and optional-literal-bool-argument shape); `Effect::SetGlobalValue`
     (`Globals`, save-registered); `Effect::Conditional` (a narrow
     `GetStageDone`-guarded `If`/`Else` now lowers, where previously ALL `If`
     declined; its guard-unresolvable-quest edge case was itself a bug fixed
     2026-09-02, Fix #3785, and its lowering recursion gained its own
     `MAX_CONDITIONAL_DEPTH` cap 2026-09-03, Fix #3279 — see Dim 5); the
     cascade-queue FIFO + ingress-vs-cascade rework (`MAX_CASCADE` now bounds
     only fragment-emitted `SetStage`s, not authored ingress); the
     multi-`Fragment_N`-per-stage merge fix (previously last-write-wins); and
     `QuestAliasReadinessGate` (an engine-authored alias-readiness-driven
     `SetStage`)) vs. deferred (Obscript/SCTX Phase 5 — the `.psc`-side
     frontend specifically, now distinct from the unrelated
     `crates/scripting/src/obscript.rs` compiled-bytecode reader that landed
     2026-09-01, see the SDK/extender coverage-gap note near the top of this
     file; the M47.1 condition resolvers' live-cell re-verification; M47.3
     Phase 4+ — Created Object alias spawn, Story Manager event fills, true
     `LCTN` alias traversal, reference-collection aliases, unloaded-world
     Find-Matching search, and the injected packages/spells/keywords overlay
     families staying parsed-not-applied). **Settled, not deferred**: the
     `AddItem`/`MoveTo` real-corpus yield re-measurement — done 2026-08-27,
     `AddItem` non-zero (54 emissions), `MoveTo` structurally zero and
     tracked as open issue #3487 (see the Future-phase-gaps bullet above);
     `ReferenceEnableState`/`Disable` no longer lacks a runtime consumer
     (see above).
     Findings count by severity. **Untrusted-input
     robustness verdict** (can a hostile/corrupt `.pex` or `.psc` panic, OOB, or
     OOM the cell loader — MUST be NO). **The 99.996% decompile-rate claim
     verdict** (is the corpus-smoke harness measuring what it claims). **The
     `.psc`-vs-`.pex` fidelity-gate verdict** (do `recognizes_da10_and_reproduces_
     hand_builder` AND `da10_pex_reproduces_hand_builder_byte_for_byte` (#1740)
     both actually pin byte-equality).
   - **Decompiler Soundness Matrix** — per pass (reader / cfg / lift+copy-prop /
     boolean / control-flow / lower): bounds-safe? terminates? total (no panic)?
     fidelity-tested? — with the two documented Champollion departures (no
     debug-line guard in `boolean.rs`; the deliberate `||`-skip in `control_flow.rs`)
     adjudicated as benign-or-bug.
   - **Decline-Invariant Audit** — every recognizer/composer/effect decline point
     × verified-conservative vs. leaks-a-partial-lowering.
   - **Runtime Lifecycle Invariant Matrix** — marker drain coverage; two-phase
     lock-drop per system; cascade bound; edge-trigger seed; CTDA OR-precedence.
   - **Findings** — grouped by severity (CRITICAL first), deduplicated.
   - **Future-Phase Readiness** — which invariants this audit pinned for Obscript
     (Phase 5), the fragment lowerer (b2), and the condition-resolver issues.
3. Remove cross-dimension duplicates: marker-drain coverage is owned by Dim 6
   (pointers from Dims 1–5 if they emit markers); the `translate_pex` clean-`None`
   contract is owned by Dim 5 (pointer from Dim 7); the half-extent convention is
   owned by Dim 7.

## Phase 4: Cleanup

1. `rm -rf /tmp/audit/scripting`
2. Inform user the report is ready.
3. Suggest: `/audit-publish docs/audits/AUDIT_SCRIPTING_<TODAY>.md`
   (domain label: `scripting`; add `quests` for QUST/alias findings and the matching
   `game:*` when the finding is specific to one title's scripts).
