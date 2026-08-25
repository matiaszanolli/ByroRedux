# Issues 2266, 2372, 3084, 3170

## #2266 (LOW, tech-debt) — orphaned sync NPC-spawn compatibility wrappers
`byroredux/src/npc_spawn.rs` — `spawn_npc_entity` (716-746) and
`spawn_prebaked_npc_entity` (815-846) have zero call sites; the only real spawn
path (`cell_loader/references/mod.rs`) constructs `NpcSpawnJob` directly and
drives `.advance()` itself. Delete both + orphaned doc-comment cross-refs in
save_io.rs, ai_package.rs, pack.rs, animation.rs, systems.rs. Domain: binary
(byroredux crate).

## #2372 (MEDIUM, epic) — EX-16 integration epic
Already handled in the prior fix-issue session (2026-08-24): broken into 4
scoped sub-issues (#3298 EX-07 SSBO yield, #3299 EX-16 item4 snapshot/restore,
#3300 EX-16 item3 NAVM decode, #3301 EX-16 items1+5 REGN audio), cross-linked
from the epic and the plan doc. Stays open by design as the epic tracker — no
further action this pass unless new information changes that.

## #3084 (LOW, tech-debt) — Oblivion creature-asset corpus guard missing #[ignore]
`byroredux/src/npc_spawn/tests.rs:773-876` — the only data-dependent corpus
test in the tree NOT `#[ignore]`d; self-skips without Oblivion data present but
cargo test still counts it `ok`. Add `#[ignore]` to match the house pattern
(parse_real_nifs.rs, per_block_baselines.rs, block_coverage_baselines.rs,
skinning_e2e.rs, crates/audio/src/tests.rs, pex_recognize_e2e.rs). Domain:
binary (byroredux crate).

## #3170 (MEDIUM, bug) — CHARAL with_gmst has zero production reach
`crates/core/src/character/leveling.rs:81-98` `with_gmst` only handles the
`SkillXp` (Skyrim-only) LevelingModel variant; but `RulesetBuilder` (profile.rs)
has no `Skyrim` arm (returns None before reaching with_gmst for Skyrim), and
the 3 arms that DO reach it (FO3/FNV/FO4) carry `XpCurve`, falling through
`other => other`. Net: GMST-sourcing seam from #2942 is unreachable on every
wired game. Suggested fix: extend with_gmst to XpCurve/SkillUse arms + derived
table; cheapest interim: add Skyrim RulesetBuilder arm (but that promotes
CHAR-2026-08-20-D2-01 from latent to live — must fix/pin first or same commit).
Domain: ecs → byroredux-core (crates/core/src/character), + byroredux/src/npc_spawn.rs
call site.

## Domain classification
- #2266, #3084 → `binary` → `byroredux`
- #3170 → `ecs` → `byroredux-core` (+ byroredux touch point)
- #2372 → epic tracker, no crate target this pass

## Resolution

### #2266 — deleted both orphaned functions
Removed `spawn_npc_entity`/`spawn_prebaked_npc_entity` and their doc-comment
cross-references, rewording each to point at the real current call path
(`NpcSpawnJob::advance` in `resumable.rs`) instead of the deleted names.
6 files touched (all doc/comment text, one `Cargo` re-export unaffected).
`cargo doc` confirms the new intra-doc link resolves; no new unused-import
warnings.

### #3084 — added `#[ignore]`
Matches the house pattern for the 6 sibling data-dependent corpus tests.
Verified both directions against real installed Oblivion data: normal
`cargo test` now reports it `ignored` (not a silent pass), and
`--ignored <name>` still runs and passes. Documented the invocation in
`docs/smoke-tests/README.md`'s table alongside `skinning_e2e`.

### #3170 — investigated, then fixed the reachable half
Independently probed real `Fallout3.esm`/`FalloutNV.esm`/`Fallout4.esm`/
`Oblivion.esm`/`Skyrim.esm` data (via a throwaway test, deleted after use)
before touching code, per this project's no-guessing policy:
- **XpCurve (FO3/FNV/FO4) and SkillUse (Oblivion) genuinely have no
  GMST to source.** No `fXPLevelUp*`-shaped setting exists on any of the
  three Fallout masters (their whole `XP*` GMST family is XP *awards*,
  never the level-up threshold curve); Oblivion has no numeric GMST for
  the major-skill-ups-per-level threshold at all (only UI string GMSTs).
  Documented this finding directly in `with_gmst`'s doc comment so a
  future reader doesn't re-attempt the same assumption.
- **Skyrim's `fXPLevelUpBase`/`fXPLevelUpMult` are real** — confirmed
  `75.0`/`25.0` on real `Skyrim.esm`, exact match to the hardcoded
  fallback. Wired the "cheapest honest interim step" the issue named:
  added `RulesetBuilder::Skyrim` (using the already-written
  `skyrim_ruleset`), making `with_gmst`'s one handled variant
  production-reachable for the first time.
- **Fixed #3169 in the same commit**, as #3170 explicitly required
  ("wiring a Skyrim arm makes SkillSet::SKYRIM production-reachable...
  fix that first, or in the same commit"): `SkillSet::SKYRIM`'s
  `Illusion` roster entry → `Mysticism` (real Skyrim.esm authors
  `AVMysticism` at the Illusion slot, not `AVIllusion` — confirmed via
  the same real-data probe). Extended `skyrim_health_resolves_to_
  authored_avif_form_id` to loop the roster the way FNV's sibling test
  already does, and verified pass/fail against real data.
- **SIBLING check**: probed FALLOUT3's roster + `AttributeSet::FALLOUT`
  against real `Fallout3.esm` — all 13 skills + 7 attributes resolve
  clean, no further Illusion-shaped divergence found. Oblivion's roster
  can't be checked this way (pre-AVIF legacy actor-value scheme, already
  documented as such) — not a new gap.
- Added a production-reach regression test
  (`skyrim_profile_builds_a_ruleset_and_actually_calls_gmst`) asserting
  `CharacterRulesProfile::SKYRIM.build_ruleset` both succeeds and
  actually invokes `gmst` for the curve settings — the exact completeness
  check #3170 asked for.
- Confirmed inert for gameplay today: no consumer currently reads
  Skyrim's new `CarryWeight`/`DamageResist` derived rows via
  `CharacterRuleset::derived_value` (only `combat.rs`'s Melee-Damage
  lookup does, and Skyrim authors no `MeleeDamage` AVIF) — this is
  infrastructure reachability, not a gameplay behavior change.

### #2372 — no action this pass
Already broken down into #3298/#3299/#3300/#3301 in the prior session;
stays open by design as the epic tracker.

### Unrelated pre-existing failure noted, not touched
`cargo test -p byroredux-plugin --test parse_real_esm -- --ignored
parse_rate_fo4_esm` fails on a `WATR`/`flowmap_scale` assertion — confirmed
via `git stash` to fail identically on clean `main`, unrelated to any change
here. Likely caused by a recent Fallout4.esm Steam update (file mtime
2026-08-22, days after the last known-good run). Out of scope for this
issue batch.
