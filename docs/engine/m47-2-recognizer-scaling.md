# M47.2 — Recognizer-catalog scaling: corpus characterization + tiered design

**Status:** design (corpus measured 2026-06-22; engine landing in progress)
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

## Why not a general AST→ECS interpreter / VM

Explicitly rejected in [`m47-2-design.md`](m47-2-design.md). The compositional
engine is *not* an interpreter: it has no runtime evaluation of arbitrary
Papyrus, no VM, no fallback. It is a finite table of recognized primitives, each
with a static lowering to an existing canonical ECS surface, and it declines
everything outside the table. The corpus data is what justifies the table being
*finite and useful*: a few hundred primitives capture the behaviorally-important
head of both populations, and the irreducibly bespoke tail is safe to decline.
