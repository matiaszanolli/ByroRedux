# Character / CHARAL Audit — 2026-09-11 (re-verification pass, second run)

**Scope**: `/audit-character`, all 6 dimensions, `--depth deep`, every implemented
family. Run as an orchestrator: six dimension agents (batches of 3), merged here.
Owner slice `crates/core/src/character/` (14 modules) plus the CHARAL-adjacent
siblings `crates/core/src/combat.rs` and `crates/core/src/stealth.rs`, the
population boundary (`byroredux/src/npc_spawn.rs` + `npc_spawn/resumable.rs` +
`npc_spawn/ai_package.rs`, `crates/plugin/src/esm/records/actor_value_derive.rs`,
`crates/plugin/src/esm/records/actor/mod.rs`), and console/runtime consumers
(`byroredux/src/commands/actor_value.rs`, `crates/scripting/src/condition.rs`,
`byroredux/src/combat.rs`).

**Naming note**: `docs/audits/AUDIT_CHARACTER_2026-09-11.md` already exists from
earlier, unrelated work today (HEAD `8151cded`). This report is a second,
independent run started after that report's fixes (`6bcd1666`, `9d792226`,
`91528dc6`, `64f586a7`, `855d10cd`) had already landed on `main`, and is saved
with a `b` suffix per this project's same-day collision convention (observed
precedent: `AUDIT_CHARACTER_2026-08-27.md` / `AUDIT_CHARACTER_2026-08-27b.md`) so
as not to overwrite the earlier report.

**Repo state at time of this run**: `main`, HEAD `b3db49fa` — five commits ahead
of the prior report's `8151cded`:

```
6bcd1666  Fix #4091, Fix #4092, Fix #4093: resolve TPLT before reading creature
          damage, race, factions, and AI packages
9d792226  Fix #4105: reword three overstated CreatureAttack claims to what
          #3762 actually did
91528dc6  Fix #4103, Fix #4104: two real CHARAL defects from the 2026-09-11 audit
64f586a7  Fix #4094, Fix #4095, Fix #4106: close three CHARAL test-coverage gaps
855d10cd  Fix #4096..#4102, #4107..#4109: sweep CHARAL doc-rot found by the
          2026-09-11 audit
```

Every one of these five commits corresponds 1:1 to a finding filed by the
prior `AUDIT_CHARACTER_2026-09-11.md`. Each dimension agent in this run was
explicitly instructed to re-verify the fix on current HEAD by reading the
actual diff/code, not by trusting the commit message.

## Tests recorded (read-only; nothing launched, no engine process started)

| Command | Result |
|---|---|
| `cargo test -p byroredux-core --lib character` (orchestrator, Phase 1) | 117 passed, 0 failed, 638 filtered |
| `cargo test -p byroredux-core --features inspect character` (Dim 1) | 117 passed, 0 failed |
| `cargo test -p byroredux-core --features inspect -- stealth:: combat::` (Dim 2) | 32 passed, 0 failed |
| `cargo test -p byroredux-core --lib character::` (Dim 3) | 116 passed, 0 failed |
| `cargo test -p byroredux-core --lib -- character::regen character::affliction character::resistance character::reputation` (Dim 4) | 45 passed, 0 failed |
| `cargo test -p byroredux --bin byroredux templated` + `cargo test -p byroredux-plugin --lib actor_value_derive` (Dim 5) | 4 passed / 27 passed, 0 failed |
| `cargo test -p byroredux-core --lib mod_docstring` (Dim 6) | 2 passed, 0 failed |

All-green across every dimension's targeted test slice.

## Executive Summary

**2 findings — 0 CRITICAL · 0 HIGH · 1 MEDIUM · 1 LOW. Both are new to this
run, both in Dimension 5, and neither is a re-opening of a prior finding.**

All ten findings filed by the earlier `AUDIT_CHARACTER_2026-09-11.md`
(D1-01/02/03, D2-01/02/03, D3-01/02/03/04, D4-01/02, D5-01/02/04/05, D6-01/02 —
19 items total across six dimensions) were independently re-verified against
current source by every dimension agent that owned them, and **every one is
confirmed FIXED**, correctly and completely, with no partial or incorrect
landing found anywhere. Two dimensions (Dimension 2, "48/48 constant rows
PASS", and Dimension 4, "28 rows checked — 27 PASS, 1 self-declared
UNSOURCED already issue-tracked") re-derived every numeric constant in their
scope directly from the six capture documents rather than carrying values
forward from the prior report, and found zero drift.

**Which families' constants were actually verified against capture
documents**: FO3, FNV, FO4, Oblivion, and Skyrim — all five implemented
rulesets, across derived-stat formulas (Dimension 2), leveling/progression
(Dimension 3), and pools/afflictions/reputation (Dimension 4). FO76 and
Starfield have capture documents but no code (see Coverage Matrix) — their
formulas were not verified because there is nothing to verify against;
Dimension 1 confirmed the two new `#[ignore]` FO76/Starfield ROSTER_CASES
tests added by `855d10cd` are honest about this (Starfield's ActionPoints AVIF
absence is asserted as `None`, not papered over).

**The one substantive new finding (D5-06, MEDIUM) is a critique of how the
prior fix landed, not a fresh behavioral bug.** `6bcd1666` correctly fixed all
three MEDIUM findings the prior audit filed (creature-attack, race-mesh, and
faction/AI-package TPLT bypasses), and shipped a regression test for each —
Dimension 5 verified all four green. But the prior report's own "structural
remedy" paragraph specifically asked for the fix to hoist one shared
`resolve_inherited_stats`/`resolve_inherited_traits` pair into
`spawn_placement_root` so a future consumer could not repeat the defect class.
That hoist did not happen: `6bcd1666` added independent resolver calls at four
(now counted: nine) separate sites rather than consolidating any of them. This
is the same defect *class* that has now recurred seven times (#2956, #3381,
#3382, #3480, #4091, #4092, #4093) — every instance fixed correctly on
inspection, but the underlying "any new call site must remember to route
through `resolve_inherited_*`" habit remains unenforced by the type system.

## Constant Verification Table

Consolidated from Dimensions 2, 3, and 4 (Dimension 1 verifies structural/
doctrine properties, not formula constants; Dimensions 5/6 verify wiring and
documentation, not formulas). Every row below was independently re-derived
from its capture document this run, not carried forward from the prior
report — full per-row citations are preserved in the individual dimension
transcripts (`/tmp/audit/character/dim_{2,3,4}.md`, since removed per Phase
4 cleanup) and summarized here.

| Area | Rows checked | PASS | FAIL | UNSOURCED |
|---|---|---|---|---|
| Derived-stat formulas (Dim 2: `derived.rs`, `fallout.rs`, `tes.rs`, `skyrim.rs`, `combat.rs`, `stealth.rs`) | 48 | 48 | 0 | 0 |
| Leveling & progression (Dim 3: `leveling.rs`, `tes.rs`, `skyrim.rs`, `components.rs`) | 18 | 18 | 0 | 0 |
| Pools, afflictions, resistance, reputation (Dim 4: `regen.rs`, `affliction.rs`, `resistance.rs`, `reputation.rs`) | 28 | 27 | 0 | 1 (self-declared, already issue-tracked — `ReputationStanding::sentiment`, #2949 CLOSED, disclosed in-code) |
| Coverage/doctrine constants (Dim 6: `DerivedStatFormula` size, module count, roster sizes, `RulesetBuilder` arms) | 4 | 4 | 0 | 0 |
| **Total** | **98** | **97** | **0** | **1** |

Notable specific verifications:
- FO4 Health `floor(77.5 + 4.5·END + 2.5·L + 0.5·L·END)` — the reference
  cross-term shape — matches `fallout.rs:133-135` exactly against
  `charal-fo4-ruleset.md:88`.
- Oblivion's `oblivion_hand_to_hand_damage` Strength×Skill cross-term
  (`combat.rs:73-82`) is the only other shipped formula needing a cross term,
  correctly outside the `DerivedStatFormula` invariant since it lives in
  `combat.rs`, not a `DerivedStatFormula` row.
- `DerivedStatFormula` is confirmed `Copy` + exactly 36 bytes
  (`formula_is_thirty_six_bytes_and_copy`, `derived.rs:354-359`, passing).
- Skyrim's GMST overlay (`with_gmst`) requests exactly
  `["fXPLevelUpBase", "fXPLevelUpMult"]` and no third GMST — the
  `xp_per_skill_rank` engine-owned-coefficient split (settled 2026-08-24) was
  correctly *not* re-flagged as an authoring gap by Dimension 3.
- All 39 FNV faction-threshold numbers and 13 REPU FormIDs (Dimension 4) and
  the 4×4 Fame/Infamy standing grid axes (checked on the asymmetric
  off-diagonal, not just the corners) verified row-for-row against
  `charal-fnv-fo3-ruleset.md`.

## Coverage Matrix

From Dimension 6, independently re-derived this run from `profile.rs`,
`leveling.rs`, `boot/world.rs`, and `boot/schedule/update.rs` (unchanged from
the prior report's matrix — no code touched any of these sites in the five
intervening commits):

| Game family | Ruleset implemented | Ruleset **wired** | Derived stats implemented | Leveling model implemented | Regen wired | Affliction wired |
|---|---|---|---|---|---|---|
| **FO3** | ✓ `fallout3_ruleset` | ✓ `RulesetBuilder::Fallout3` | ✓ 8 rows | ✓ `150·L+50`, cap 20 | ✗ | ✗ |
| **FNV** | ✓ `falloutnv_ruleset` | ✓ `RulesetBuilder::FalloutNewVegas` | ✓ 8 rows | ✓ `150·L+50`, cap 30 | ✗ | ✗ |
| **FO4** | ✓ `fallout4_ruleset` | ✓ `RulesetBuilder::Fallout4` | ✓ 3 rows | ✓ `75·L+125`, uncapped | ✗ | ✗ |
| **FO76** | ✗ no builder | ✗ `RulesetBuilder::None` | ✗ capture LOCKED, uncoded | ✗ capture LOCKED, uncoded | ✗ | ✗ |
| **Oblivion** | ✓ `oblivion_ruleset` | ✗ `RulesetBuilder::None` — **#3848 OPEN** | ✓ 8 rows / 5 stats (synthetic resolver only) | ✓ 10 major-skill-ups | ✗ no caller reaches `oblivion_pool_regen_config` | ✗ |
| **Skyrim SE** | ✓ `skyrim_ruleset` | ✗ `RulesetBuilder::None` — **#3848 OPEN** | ✓ 2 rows (synthetic resolver only) | ✓ `25·L+75` | ✗ | ✗ |
| **Starfield** | ✗ no builder | ✗ `RulesetBuilder::None` | ✗ roster LOCKED, rows uncaptured | ✗ curve/thresholds PENDING | ✗ | ✗ |

`pool_regen_tick_system` is registered in `Stage::Update` and its accumulator
is inserted unconditionally at boot, but `PoolRegenConfig` has **zero
production insertion sites** — every `insert_resource(PoolRegenConfig` call is
inside `regen.rs`'s own `#[cfg(test)]` module. The system is therefore dead
code in production for all seven families today, root-caused by the same
issue that blocks Oblivion/Skyrim entirely (#3848 OPEN). This is documented at
three code sites plus `docs/feature-matrix.md` and is coverage information,
not a new defect (confirmed independently by both Dimension 4 and Dimension
6). `affliction_tick_system` similarly has no scheduler registration at all.

## Findings

### MEDIUM

#### D5-06: `6bcd1666` patched four TPLT-resolution call sites individually instead of hoisting the shared resolver the prior audit's own "structural remedy" specified — the defect class remains reproducible by a future consumer
- **Dimension**: Population Boundary
- **Game**: all templated (FNV/FO3/FO4/Skyrim families that ship `TPLT`/`template_flags`)
- **Location**: `byroredux/src/npc_spawn/resumable.rs:1418-1444` (`spawn_placement_root`, unchanged shape — still hands the raw shell to its four stamps); `byroredux/src/npc_spawn.rs:81,156,196`; `byroredux/src/npc_spawn/ai_package.rs:519-534`; `byroredux/src/npc_spawn/resumable.rs:432-438,619-625,1177-1183`; `byroredux/src/cell_loader/references/mod.rs:629-651`
- **Source**: not numeric — a process finding against `docs/audits/AUDIT_CHARACTER_2026-09-11.md`'s own text: *"The structural remedy is the same for all three: hoist one `resolve_inherited_stats` / `resolve_inherited_traits` pair into `spawn_placement_root` and hand every consumer the resolved record, so a sixth site cannot repeat it."*
- **Description**: `6bcd1666` fixed the three named MEDIUM defects (creature-attack damage, race meshes, factions/AI-packages all reading the unresolved NPC shell instead of the TPLT-resolved record) and shipped a passing regression test for each. But it did so by adding independent `resolve_inherited_*` calls at each of the affected sites rather than consolidating any of the calls already present. Counting every site in the spawn path that now independently walks the TPLT chain: `stamp_faction_ranks`, `stamp_actor_values`→`derive_npc_actor_values`, `stamp_creature_attack`, `stamp_character_components` (all four called from `spawn_placement_root` with the raw `&NpcRecord`, each resolving what it needs on its own), plus `apply_ai_package_behavior`, `prepare_runtime_state`, `prepare_creature_state`, `prepare_prebaked_state`, and `load_references_budgeted` — nine independent authors of the same three-line pattern (`effective_actor_level` → `resolve_inherited_*` → read a field).
- **Impact**: (1) Correctness risk, latent: every currently-reachable consumer is individually correct today (confirmed by four new green regression tests), but the type system gives a future contributor no signal to route a new population-boundary read through `resolve_inherited_*` — it can read `npc.<field>` directly and compile cleanly, exactly as `stamp_creature_attack` did when first introduced. This defect class has now recurred seven times (#2956, #3381, #3382, #3480, #4091, #4092, #4093). (2) Redundant work: one NPC spawn via the runtime/FaceGen path now walks the TPLT chain independently up to six times per spawn (bounded by `MAX_TPLT_DEPTH`, so not unbounded, but wasteful), and any future divergence between two of the six copies would reproduce the "one entity, two chains" symptom #3480 exists to document.
- **Suggested Fix**: In `spawn_placement_root`, resolve once — `let stats = resolve_inherited_stats(npc, level, index); let traits = resolve_inherited_traits(npc, level, index); let factions = resolve_inherited_factions(npc, level, index);` — and change every stamp's signature to take the already-resolved records instead of `npc` plus `index`. Apply the same pattern to the three `build_npc_equip_state` call sites and to `apply_ai_package_behavior`'s caller. This is a mechanical refactor with no behavior change (every current call site already resolves correctly), and it converts "which of the two record shapes do I have" from a habit into a compile-time question for the next contributor.

### LOW

#### D5-07: Six of the eleven documented `template_flags` bits are parsed and stored with no consumer at all — transparently disclosed, but still a population gap
- **Dimension**: Population Boundary
- **Game**: all (FO3/FNV/FO4/Skyrim — wherever `template_flags` is parsed)
- **Location**: `crates/plugin/src/esm/records/actor/mod.rs:445-448` (doc comment enumerating the bits: `0x0008` Actor Effects, `0x0010` AI Data, `0x0040` Model/Animation, `0x0080` Base Data, `0x0200` Script, `0x0400` Def Pack List)
- **Source**: not numeric — the code's own doc comment discloses this; cross-checked no live consumer exists for any of the six.
- **Description**: This is disclosure, not a silent gap — the flags are named and their non-consumption is stated in the same doc comment that documents the ones that *are* consumed (`0x0004` Factions and `0x0020` AI Packages were the two `6bcd1666` closed this session). No CHARAL-owned component (`ActorValues`/`CharacterLevel`/`Background`/`Perks`/`FactionRanks`/`AmbientPackageRuntime`) currently depends on any of the six remaining bits — `AIDT` (aggression/confidence) is not even parsed onto `NpcRecord` yet, and the `Use Script` flag's real consumer is a different subsystem (M47 scripting's VMAD attach path via `cell_loader`, not the NPC_-record TPLT walk).
- **Impact**: None currently measurable — no reader exists to read the wrong (unresolved) copy of any of these six categories, because no reader exists at all yet.
- **Suggested Fix**: None needed now. Flag for whoever adds the first consumer of one of these six categories to route it through `resolve_inherited_record` with the matching flag constant from the start — and, per D5-06 above, to receive an already-resolved record from `spawn_placement_root` rather than resolving it itself.

## Known-Open Register

Restated per the audit protocol; confirmed not re-filed by any dimension this run:

1. **FNV/FO3 tag-skill per-level formula** — still undocumented and deliberately
   deferred. Dimension 5 confirmed `actor_value_derive.rs`'s module doc still
   explicitly states the +15 flat bonus and Intelligence-scaled per-level
   growth are unimplemented. CLAS SPECIAL still lives in `ATTR`, not `DATA`.
2. **FO3↔FNV divergent player Health/AP** — still deferred pending
   master-name disambiguation, unchanged.
3. **VATS runtime** (AP pool/regen, time-pause, limb health, hit-chance roll)
   — still does not exist; only the AP formulas live in CHARAL. Not
   re-investigated as new by any dimension.
4. **Issue #3848** (Oblivion/Skyrim rulesets production-unreachable —
   `build_ruleset` returns `None` for both) — confirmed **still OPEN** via
   `gh issue view 3848` and confirmed still accurately describing current
   code (`profile.rs`'s `RulesetBuilder::None` arms for `OBLIVION`/`SKYRIM`).
   Cited by Dimensions 3, 4, and 6; re-filed by none.

## Prior-Report Findings — Final Disposition

| ID | Dimension | Prior severity | Verdict this run |
|---|---|---|---|
| D1-01 | 1 | MEDIUM | FIXED |
| D1-02 | 1 | LOW | FIXED |
| D1-03 | 1 | LOW | FIXED |
| D2-01 | 2 | (unsourced constant) | FIXED |
| D2-02 | 2 | (stale doc claim) | FIXED |
| D2-03 | 2 | (stale doc claim) | FIXED |
| D3-01 | 3 | LOW | FIXED |
| D3-02 | 3 | LOW | FIXED |
| D3-03 | 3 | LOW | FIXED |
| D3-04 | 3 | LOW | FIXED |
| D4-01 | 4 | MEDIUM (#4103) | FIXED |
| D4-02 | 4 | MEDIUM (#4104) | FIXED |
| D5-01 | 5 | MEDIUM (#4091/#4092) | FIXED (see D5-06 for a process critique of *how*) |
| D5-02 | 5 | MEDIUM (#4093) | FIXED (see D5-06 for a process critique of *how*) |
| D5-04 | 5 | LOW | CLOSED (fixed one commit earlier, `64f586a7`) |
| D5-05 | 5 | LOW (#4107) | FIXED as scoped (doc-only correction); underlying `PoolRegenConfig` unwired fact unchanged and undisputed — see Coverage Matrix |
| D6-01 | 6 | LOW (#4108) | FIXED |
| D6-02 | 6 | LOW (#4109) | FIXED |

**19 of 19 prior findings confirmed fixed. 2 new findings opened this run
(1 MEDIUM, 1 LOW), both in Dimension 5.**

## Cross-Audit Routing

Unchanged from the prior report's routing table: component storage shape →
`/audit-ecs`; AVIF/CLAS/NPC_/RACE/CREA byte accounting → `/audit-esm`; CTDA
condition evaluation → `/audit-scripting`; scheduler access declarations →
`/audit-concurrency` (Dimension 4 spot-checked the `pool_regen_tick_system`
declaration-matches-body property specifically and found it correctly
extended by `91528dc6`, but full concurrency-correctness ownership remains
with `/audit-concurrency`).
