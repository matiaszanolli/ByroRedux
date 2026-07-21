# M47.2 — Recognizer-catalog scaling: corpus characterization + tiered design

**Status:** engine landing in progress (corpus measured 2026-06-22; b1 guard
engine + b2 fragment lowerer + QUST VMAD property-table wiring +
`AddItem`/`MoveTo` object-targeting effects all shipped 2026-07-21 —
the latter measured at ~0% real-corpus yield pending alias resolution,
see "Shipped" below. `Enable`/`Disable`, `EvaluatePackage`, `Start`/`Stop`
remain out of scope — see Backlog)
**Companion to:** [`m47-2-design.md`](m47-2-design.md) (the `.pex` decompiler +
recognizer-chain that this scales)

## The question

The M47.2 recognizer catalog
([`crates/scripting/src/translate/recognizers/`](../../crates/scripting/src/translate/recognizers/))
is deliberately a *catalog* — each behavior shape is hand-written Rust. That
has a linear cost ceiling against the tens of thousands of shipping scripts.
Before choosing an architecture we measured the actual corpus: how many
distinct structural shapes exist, and what fraction the top-N cover.

The measurement tool is
[`crates/pex/examples/pex_corpus_shapes.rs`](../../crates/pex/examples/pex_corpus_shapes.rs).
It decompiles every `.pex` to the shared `byroredux_papyrus` AST and abstracts
each script to a **structural fingerprint**: control flow + called API names +
operators + argument arity, with the *holes a recognizer binds* erased (literal
values → `#`, ref identities — locals/params/properties/decompiler temps → `$`,
casts unwrapped). Two scripts share a template only if everything but those
holes matches — so the distinct-template count is a faithful proxy for "how many
exact-match recognizers it would take."

Run it (raw output checked in at [`docs/r5/corpus-shape-survey.txt`](../r5/corpus-shape-survey.txt)):

```bash
cargo run --release -p byroredux-pex --example pex_corpus_shapes -- \
  "<Skyrim SE>/Data/Skyrim - Misc.bsa" \
  "<Fallout 4>/Data/Fallout4 - Misc.ba2" \
  "<Starfield>/Data/Starfield - Misc.ba2"
```

Those three vanilla base archives *are* the canonical corpus: **26,641 `.pex`**,
of which 26,640 decompile to AST with zero panics.

## What the corpus actually is

The first result redraws the problem. By population:

| Population | Count | % of corpus |
|---|---:|---:|
| **Quest/scene/dialogue/perk fragments** (`Fragment_*` fns) | **18,502** | **69.5%** |
| **Event-handler scripts** (the recognizer chain's domain) | 5,935 | 22.3% |
| Function libraries (callable, no events/fragments) | 1,658 | 6.2% |
| Pure data/property holders | 545 | 2.0% |

**The recognizer chain addresses ~22% of the corpus.** The dominant population —
fragments — is invoked by a *known contract* (quest stage N's fragment runs when
stage N is set; scenes/dialogue analogously), not by shape recognition. It is a
separate dispatch model the chain never touches.

## The distribution

### Event handlers are heavy-tailed

10,764 behavioral handler bodies (non-empty). Whole-body templates: **9,250
distinct**; 50% coverage needs **2,773 templates**. The "50 recognizers → 90%"
hope is false. Even decomposed to compositional primitives — guard atoms split
on `&&`/`||`, plus leaf effect statements, exactly how `quest_stage_gate` walks
the And-tree:

| Vocabulary K | handlers fully covered |
|---:|---:|
| 50 | 16.4% |
| 200 | 34.6% |
| 500 | 48.2% |
| 1000 | 59.3% |

50% needs K=560; **80% needs K=3,101**. Nearly every behavioral handler carries
≥1 script-unique call (its actual purpose); under decline-on-any-unknown that one
term kills the whole handler. There is no cheap exhaustive coverage here.

### Fragments are an order of magnitude more compressible

43,818 behavioral `Fragment_*` functions — ~4× the handler volume, far more
uniform:

| | top-10 templates | K=50 prims | K=500 prims | K=1500 prims |
|---|---:|---:|---:|---:|
| Handlers | 5.1% | 16.4% | 48.2% | 66.8% |
| **Fragments** | **25.7%** | **50.0%** | **78.5%** | **90.4%** |

Fragment body templates reach 50% with just **204 templates**. Fragment
primitives: 50%→K=51, 80%→K=562, 90%→K=1,441. The top templates are clean and
canonical — `{$=$;$.setstage(#)}` (8.4%),
`{self.setobjectivecompleted(#,#);self.setobjectivedisplayed(#,#,#)}`,
`{$.start()}` — and the top primitives are a small vocabulary that maps almost
1:1 to ECS ops the engine needs anyway: `SetStage`, `SetObjectiveDisplayed/
Completed`, `Start/Stop`, `GetOwningQuest`, `EvaluatePackage`, `Enable/Disable`,
`AddItem`, `MoveTo`, `AddToFaction`.

The **effect vocabulary overlaps heavily** between the two populations (SetStage,
Start/Stop, Enable/Disable, GetOwningQuest, SetValue). Handlers add *guards*
(player-gate, GetStageDone, GetStage comparisons) and *state machines*
(GoToState); fragments are mostly unguarded effect sequences — they are
pre-gated by the stage contract.

## Tiered architecture

The data redraws the picture: the recognizer catalog (handlers) is the smaller,
harder half. There are two lowering problems with opposite economics and one
shared mechanism underneath.

### One compositional engine, parameterized by a shared primitive table

A *primitive* is a table entry:

```
PrimitiveTable = {
  guards:  [ GuardPrimitive  { match: &Expr → Option<Bind>, lower: (Bind, Ctx) → Condition } ],
  effects: [ EffectPrimitive { match: &Stmt → Option<Bind>, lower: (Bind, Ctx) → Effect    } ],
}
```

The engine walks a statement body, classifies each guard atom and each leaf
statement against the table, **declines the whole unit on the first unmatched
node or unbindable hole**, and otherwise assembles the collected canonical
fragments. This is `quest_stage_gate` generalized from one hand-coded shape into
a data-driven table — adding a primitive lifts *every* body that uses it, so the
table grows sub-linearly in the corpus while coverage tracks the curves above.

The engine has two front-ends, differing only in dispatch trigger and which AST
nodes they walk:

- **(b1) Event-handler recognizer** — one entry in the existing `RECOGNIZERS`
  chain. Walks event bodies; needs the guard primitives. Target the **head**
  (~100–200 primitives ≈ the gameplay-critical doors/triggers/quest-activators,
  the value-dense ~35%) and **decline the bespoke tail**. Do not chase 90% — the
  tail is set-dressing whose absence is harmless (decline = no behavior, safe).
- **(b2) Fragment lowerer** — *new*, and the real scaling lever. Walks
  `Fragment_*` bodies, dispatched by the quest-stage / scene-phase contract (no
  shape recognition needed for *when*). A shared effect table of ~500 primitives
  reaches **78%** of 43,818 fragments; ~1,500 reaches **90%**. Investment here
  compounds.

### The (a)/(b) boundary

- **(a) hand-written recognizer** — only when the *lowering* needs judgment:
  multi-component, stateful, or where the canonical ECS mapping is not a flat
  per-primitive emission (true state machines; the `defaultRumbleOnActivate`
  multi-call latent sequence). Few of these.
- **(b) compositional engine** — everything that is a flat composition of
  independently-meaningful guard/effect primitives.

By that test `quest_stage_gate` is a (b)-shaped behavior currently hand-coded —
it becomes the engine's first table entries (player-gate guard, GetStageDone
guard, SetStage effect), proving the mechanism against its existing golden tests.

### Invariants preserved by construction

- **Single chain, same AST target** — the engine is one `Recognizer` fn; the
  fragment lowerer consumes the same AST. No new `ScriptSource` variant.
- **Decline-on-any-unmodeled-term** — enforced per node: an unmatched
  guard/statement, or any hole that cannot fully bind (quest via VMAD /
  owning-quest, target ref), fails the whole unit. As conservative as the
  hand-written path; a partial match declines, never approximates.
- **Fidelity-gated** — each primitive's `lower` carries a golden test (the R5
  pattern). The survey tool is the coverage-regression instrument.

## Empirical validation (landed increment)

The first effect table — **5 primitives** (`SetStage` + the three objective
setters + `CompleteAllObjectives`) — measured against the real corpus by
[`crates/scripting/examples/fragment_coverage.rs`](../../crates/scripting/examples/fragment_coverage.rs):

```
behavioral fragments: 43818
fully lowered (claimed): 10435 (23.8% of behavioral)   declined: 33383 (76.2%)
canonical effects emitted: 12536  (SetStage 7856, SetObjectiveDisplayed 2670,
                                   SetObjectiveCompleted 1868, CompleteAllObjectives 100,
                                   SetObjectiveFailed 42)
```

Five primitives claim ~a quarter of the entire fragment population — the
steep head the primitive-frequency curve predicted. The example doubles
as a coverage-regression gate as the table grows toward the ~500-primitive
/ ~78% target. The lowerer is fully tested
([`crates/scripting/src/translate/effects.rs`](../../crates/scripting/src/translate/effects.rs))
and dispatched through [`crates/scripting/src/fragment.rs`].

**Runtime population landed.** The last missing piece — the **QUST VMAD
fragment-section decoder** (stage→`Fragment_N` binding) — is now built.
Its layout was derived empirically + cross-validated against `Skyrim.esm`
(version byte `2` on all 856 fragment-bearing QUST VMADs;
[`crates/plugin/examples/dump_qust_vmad_fragments.rs`](../../crates/plugin/examples/dump_qust_vmad_fragments.rs)),
the same accepted method the scripts-section decoder used —
[`parse_quest_fragments`](../../crates/plugin/src/esm/records/script_instance.rs)
surfaces the bindings on `QustRecord.fragments`. At cell load the binary's
`populate_quest_fragments` resolves each quest's compiled `QF_` `.pex`,
decompiles it, and lowers each bound fragment body via
[`populate_quest_fragments_from_pex`](../../crates/scripting/src/fragment.rs)
into `QuestStageFragments` for `quest_fragment_dispatch_system`. End-to-end
on real data (`crates/scripting/examples/quest_fragment_populate.rs`):
**845 scripted Skyrim quests → 5,108 stage bindings → 742 stage fragments
lowered + registered** (14.5%, gated by the current 5-primitive effect
table; the remainder decline safely pending more b2 primitives + the
FormID→entity resolver).

## Sequencing

1. **Refactor `quest_stage_gate` into the compositional engine + primitive
   table.** Risk-free (existing tests pin it); builds the mechanism everything
   reuses. *First.*
2. **Fragment lowerer (b2).** The 69.5% lever. Start with the top effect
   primitives wired to existing ECS systems (`QuestStageState`, objectives,
   enable/disable, `GetOwningQuest`).
3. **`OnHit` emit site** — 376 scripts (#6 by frequency), well ahead of OnEquip.
4. **`OnEquip(ped)`** — 78 scripts (#29). Real but small; after OnHit.
5. **"136-event dispatch" → demand-driven, not exhaustive.** The event
   distribution is itself heavy-tailed: the top ~20 events cover the vast
   majority of behavioral handlers. Build emit sites down the frequency list;
   never speculatively build 136 rows.
6. **Perk entry-point composition — deferred.** Perks are PERK records / entry
   points, not Papyrus scripts; the survey shows no script-coverage leverage
   there, and there is still no authoritative per-game entry-point table (see
   [`tables.rs`](../../crates/scripting/src/translate/tables.rs) "Deferred").

### Event-handler frequency (top, scripts defining each)

```
onactivate 1310  onload 1003  ontriggerenter 955  ondeath 509  oneffectstart 426
onhit 376  onunload 346  ontimer 345  oninit 340  onupdate 265  ontriggerleave 239
ondying 200  oncontainerchanged 199  oncellattach 199  oncellload 196 ...
onequipped 78 ...
```

## Backlog & deferred (sequence items 3–7)

Status after the b1 (compositional guard engine) and b2 (fragment lowerer)
increments landed:

- **QUST VMAD property table wired (landed).** The `Property`-targeted
  branch of `QuestRef` resolution (`SomeOtherQuest.SetStage(..)` bound via
  a `Quest Property`) was shipped in `effects.rs`/`fragment.rs` from day
  one, but `quest_fragment_dispatch_system` always called `resolve_quest`
  with `vmad: None` — the QUST record's own VMAD *scripts section* (its
  declared property bindings) was decoded by `parse_quest_fragments`
  internally (via `ScriptInstanceData::parse_with_consumed`) and then
  discarded, only the fragment section's byte offset was kept. Fixed by
  decoding the same VMAD bytes a second time into `QustRecord.script_instance`
  and registering it via `QuestStageFragments::insert_vmad`, so dispatch now
  passes the real VMAD. Verified against live `Skyrim.esm`: 969 quests have
  a registered property table (of 845 with fragment bindings + others with
  VMAD but no fragments); the 742-fragments-lowered figure is unchanged (a
  pure resolution fix, not a lowering change). This does **not** add new
  effect primitives — it fixes a dead branch in already-shipped ones.

- **Correction — the FormID→entity resolver already exists.** This doc
  previously said object-targeting effects (`Enable`/`Disable`/`MoveTo`/
  `AddItem`/`EvaluatePackage`) were blocked on a FormID→entity resolver
  that didn't exist. That premise is stale, though not for the function
  first suspected: `World::find_by_form_id` (`crates/core/src/ecs/world.rs`)
  is test-only today (a plain `FormIdComponent` equality scan, no
  `FormIdPool` hop — never called from production code). The real,
  in-production resolver is
  [`byroredux_scripting::condition::resolve_entity_by_global_form_id`]
  (`crates/scripting/src/condition.rs`) — it resolves a raw plugin-local
  form ID through `FormIdPool` before matching against each entity's
  `FormIdComponent` (the multi-master-safe path `find_by_form_id` skips),
  and is already the load-bearing resolver for the M42.5–8 Follow/Escort/
  Guard/Travel AI packages (landed 2026-07-16, after this doc was
  written) and for M47.1's `GetIsID`/`GetDistance` condition functions.
  Being already in the `scripting` crate, it's directly callable from
  `fragment.rs`/`effects.rs` with no new cross-crate wiring. The resolver
  is not the blocker for object-targeting effects — see below for what
  actually is.

- **More b2 effect primitives → the ~78% target.** Still the
  highest-value continuation, but the next tier needs new modelling, not
  just a resolver:
  - `Quest.Stop()` / `Quest.Start()` are *polymorphic-named* — shared with
    `Scene`/`Sound`/`Package`/`ObjectReference` — so claiming them on a
    bare property receiver (whose declared type the AST alone doesn't
    carry) risks a misread; safe only for `Self`/`GetOwningQuest()`
    receivers (unambiguous — the fragment script always extends `Quest`)
    or now, with VMAD wired, a `Property` receiver whose VMAD entry is
    confirmed `Object`-typed. Also: even resolved correctly, there is
    nowhere to apply it — no "quest running" state exists anywhere in the
    engine yet, so `Start`/`Stop` effects would be inert bookkeeping until
    something consumes that state. Do this once a real consumer exists,
    not speculatively.
  - Object-targeting effects (`Enable`/`Disable`/`MoveTo`/`AddItem`/
    `EvaluatePackage`) need a new `Effect` shape (today's `Effect` enum is
    quest-scoped only — every variant carries a `QuestRef`) plus an
    object-reference resolution path: bind the property name to a FormID
    via the (now-available) VMAD, then to a live `EntityId` via
    `resolve_entity_by_global_form_id` at *apply* time (the entity may not
    be spawned/loaded when the fragment lowers). That's a real design
    increment — a second `Ref` enum parallel to `QuestRef`, new `Effect`
    variants, and the dispatch-time resolve-and-apply wiring — not a
    one-line unblock. Full design below.

## Design: object-targeting fragment effects

Scoped 2026-07-21, not yet implemented. Covers `AddItem` and `MoveTo` —
the two object-targeting effects with a real ECS consumer today.
`Enable`/`Disable`/`EvaluatePackage` are deliberately **not** designed
here; see "Explicitly out of scope," below.

### The two-hop resolution model

An object-targeting effect's receiver is always a bare `Property` — there
is no `Self`/`GetOwningQuest()`-equivalent unambiguous case the way a
`Quest`-typed receiver has, because the fragment script (`QF_…`, `extends
Quest`) is never itself the `ObjectReference`/`Actor` being acted on. So
resolution is VMAD-or-nothing, in two hops that happen at *different
times*:

1. **Lowering time** (`lower_fragment`, AST only, no `World`): resolve the
   call's receiver/argument name to a **symbolic** reference —
   `ObjectRef::Property(String)`, the exact shape `QuestRef::Property`
   already uses. No FormID lookup yet; lowering never sees a VMAD or a
   `World`.
2. **Dispatch time** (`apply_effect`, has both the quest's VMAD and
   `&World`): resolve the property name to a FormID, then to a live
   entity —
   ```
   fn resolve_object(vmad: &ScriptInstanceData, world: &World, name: &str) -> Option<EntityId> {
       let form_id = resolve_property_form_id(vmad, name)?;
       resolve_entity_by_global_form_id(world, form_id)  // crates/scripting/src/condition.rs
   }
   ```
   `resolve_property_form_id` is `receiver_quest`'s object-typed sibling:
   look up the named property, require `PropertyValue::Object { alias, .. }`,
   and **decline when `alias != -1`**. An alias-bound VMAD entry (a
   `ReferenceAlias Property`, common in Radiant/companion quests) needs
   the quest-alias-fill subsystem to resolve correctly — the raw `form_id`
   sitting next to a live alias index is not reliably the intended
   target, so guessing it would risk a wrong-object application. Declining
   is the same discipline the corpus survey's data justified everywhere
   else: a partial understanding is a full decline, never a guess.

   An entity not being found (unloaded cell, not-yet-spawned REFR, wrong
   FormID) also declines the *effect application* (not the whole
   fragment — see below), logged at `debug`, matching `resolve_quest`'s
   existing "skip, never guess" contract.

### Signature change

`apply_effect`/`apply_effects` gain a `world: &World` parameter. This is
**not** the disruptive `&mut World` ripple it might look like — the
engine's established convention (per the ECS architecture invariants) is
that systems take `&World` and mutate through `world.query_mut::<T>()`'s
interior-mutable `RwLock`-backed storage, exactly how `trigger_detection_system`
inserts `OnTriggerEnterEvent` today. `quest_fragment_dispatch_system`
already holds `world: &World`; it just starts threading it one level
deeper.

A per-effect apply failure (target unresolved, component missing) skips
*that effect* and continues the rest of the fragment's effects — unlike
`lower_fragment`'s decline-the-whole-fragment contract. This matches the
existing `resolve_quest` behavior for `Property`-targeted quest effects
(one skip in `apply_effects`'s `filter_map` chain, not an abort), and is
the correct level: lowering is static (either the *shape* is understood
or not), while dispatch-time resolution failure is a runtime data fact
(this particular save/cell/load order didn't have the target loaded) that
one fragment can't retroactively un-recognize.

### New `Effect` variants

```rust
pub enum Effect {
    // ...existing quest-scoped variants unchanged...

    /// `<container>.AddItem(<item>, <count>[, abSilent])`. `abSilent` is
    /// parsed (declines if a 4th arg or a non-literal count/silent value
    /// is present — same over-conservative-decline discipline as
    /// `SetObjectiveDisplayed`'s `bool_arg`) but not applied — no pickup
    /// notification UI exists yet, so a silent vs. noisy AddItem look
    /// identical today. `item` resolves only to a FormID (never to an
    /// entity — it names a *base record*, not a placed reference).
    AddItem { container: ObjectRef, item_form_id_ref: ObjectRef, count: u32 },

    /// `<moved>.MoveTo(<destination>)`. The conservative 2-arg shape only
    /// — declines if offset/match-rotation args are present (a snap with
    /// silently-dropped offsets would misplace the object). Both operands
    /// are `ObjectRef`s resolved to full entities: `moved`'s `Transform`
    /// is overwritten with `destination`'s `GlobalTransform` translation,
    /// mirroring `resolve_destination`'s existing
    /// `GlobalTransform.translation` read in `byroredux/src/systems/travel.rs`.
    MoveTo { moved: ObjectRef, destination: ObjectRef },
}

/// The object-targeting sibling of `QuestRef` — see "Design" above.
/// Always VMAD-or-decline; no unambiguous bare-receiver case exists.
pub enum ObjectRef {
    Property(String),
}
```

`AddItem`'s apply: resolve `container` to an entity, resolve
`item_form_id_ref` to a bare FormID (stop at hop 1 — no entity lookup),
then `world.query_mut::<Inventory>().get_mut(container_entity)?.push(ItemStack::new(item_form_id, count))`
— reusing `Inventory::push` (`crates/core/src/ecs/components/inventory.rs`)
exactly as written today, no new inventory-side code.

`MoveTo`'s apply: resolve both operands to entities, read
`destination`'s `GlobalTransform.translation`, write it onto `moved`'s
`Transform.translation` via `world.query_mut::<Transform>()`.

### Shipped (2026-07-21) — and an empirical yield finding

`AddItem`/`MoveTo` landed exactly as designed above: `ObjectRef` in
`compose.rs`, the two lowering primitives + `receiver_object` (bare-
property-only, declining any local-variable receiver — including a
side-effect-free ident copy, since this increment doesn't trace a local
back to the property it came from) in `effects.rs`, and the dispatch-time
`resolve_property_form_id` / `resolve_object` pair in `fragment.rs` using
`resolve_entity_by_global_form_id`. `AddItem` creates an `Inventory` on
the container on demand (an interior-mutable `QueryWrite::insert`, not a
structural `&mut World` mutation — every object can receive items in
Bethesda's runtime, so this is the correct default); `MoveTo` requires a
pre-existing `Transform` on the moved entity and declines otherwise (a
"moved" entity with no `Transform` isn't a placed spatial entity, so
nothing to snap). 9 new tests (lowering shapes + decline cases + full
dispatch through a live `World` with `FormIdComponent`-bound entities),
all green; full workspace suite unaffected.

**Live-corpus measurement, though, found the real yield is ~0% today** —
`fragment_coverage` against both `Skyrim - Misc.bsa` (14,026 `.pex`) and
`Fallout4 - Misc.ba2` (7,875 `.pex`) claimed zero `AddItem`/`MoveTo`
effects in either corpus (all lowered effects were still the original
five quest-scoped ones). The mechanism is correct and dispatch-tested,
but real quest content's dominant idiom binds the object via an alias
accessor first — `ObjectReference k = SomeAlias.GetActorRef()` (the
`$=$.getactorref()` primitive at 2.5% frequency in the original corpus
survey), *then* `k.AddItem(...)` — not a bare `ObjectReference Property`
receiver. `bind_local` already declines that binding's *whole fragment*
regardless (`GetActorRef()` is a side-effecting call, #1907's discipline),
so extending `receiver_object` to trace local-to-property copies (a
smaller change) would not have helped — the real blocker is the same one
flagged in the resolution model above: alias-bound references need the
quest-alias-fill subsystem, which does not exist yet. **This makes
alias resolution the highest-leverage next step for object-targeting
effects** — more so than adding further effect primitives to a
receiver-resolution shape the live corpus rarely uses directly. The
implementation ships now (correct, tested, and dormant) so it activates
immediately once alias resolution lands, rather than needing to be built
twice.

### Explicitly out of scope (not designed here)

- **`Enable`/`Disable`.** No visibility/collision-suppression component
  exists anywhere in the ECS, renderer, or physics today — grepped for
  `Disabled`/`Hidden`/an enabled marker and found nothing. Wiring this
  correctly means a new marker component **and** teaching the renderer's
  draw collection, the physics world, and (for skinned meshes) BLAS
  build/eviction to skip a disabled entity — a real feature in its own
  right (visible in-game, not just an internal effect), not a fragment-
  effect primitive. Do this as its own scoped milestone if/when a real
  workload needs object visibility toggling, not as a rider on this
  design.
- **`EvaluatePackage`.** Per the AI-package Known Issue, packages are
  selected once at NPC spawn with no re-evaluation trigger as game time
  or world state advances — `EvaluatePackage`'s entire meaning (force an
  immediate re-pick) has no hook to attach to. Needs the AI package
  system's re-evaluation mechanism built first (a Tier 7 concern), not a
  fragment-effect addition.
- **`Quest.Start()`/`Quest.Stop()`** — covered above; blocked on a
  currently-nonexistent "quest running" state, not on resolution.

- **OnHit emit site (item 3) — blocked on a combat system.** 376 scripts
  define `OnHit`, and the `HitEvent` marker + cleanup already exist, but
  *nothing emits it*: there is no combat / projectile / melee / damage
  system in the engine to detect a hit. An emit site can't be written
  without fabricating a trigger source, so it is deferred until a combat
  subsystem exists. The consumer contract (`HitEvent`) is in place for
  that day.

- **OnEquip emit site (item 4) — blocked on runtime equip dispatch.**
  78 scripts define `OnEquip(ped)`. `OnEquipEvent` exists; the only equip
  code today is the M41 *spawn-time* NPC outfit application
  (`byroredux/src/npc_spawn.rs`), not a runtime equip *action* that a
  script would observe. A faithful emit site needs the runtime
  inventory/equip-action path (and the equipped item's VMAD script
  binding) — deferred with the contract in place.

- **136-event dispatch (item 5) — demand-driven, not built ahead.** The
  event-frequency table above is the priority order; build a marker +
  emit site per event only when real content needs it (most top events
  already have markers). No speculative 136-row table.

- **Perk entry-point composition (item 6) — deferred, no script-coverage
  leverage.** Perks are PERK records / entry points, not Papyrus scripts,
  so the fragment/handler survey doesn't bear on them, and there is still
  no authoritative per-game entry-point index table (see
  [`tables.rs`](../../crates/scripting/src/translate/tables.rs)). Deferred
  until that table is sourced — same no-guessing discipline as VMAD.

## Why not a general AST→ECS interpreter / VM

Explicitly rejected in [`m47-2-design.md`](m47-2-design.md). The compositional
engine is *not* an interpreter: it has no runtime evaluation of arbitrary
Papyrus, no VM, no fallback. It is a finite table of recognized primitives, each
with a static lowering to an existing canonical ECS surface, and it declines
everything outside the table. The corpus data is what justifies the table being
*finite and useful*: a few hundred primitives capture the behaviorally-important
head of both populations, and the irreducibly bespoke tail is safe to decline.
