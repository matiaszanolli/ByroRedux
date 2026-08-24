# Character / CHARAL Audit — 2026-08-24

**Scope**: `/audit-character` — all 6 dimensions, all implemented families, `--depth deep`.
Run **solo** (single agent, no sub-agent fan-out) per explicit run instruction.

**Repo state**: HEAD `048a8bd8`, branch `main`. Delta since the last sweep
(`docs/audits/AUDIT_CHARACTER_2026-08-20.md`, HEAD `bb0b92f2`): **108 commits**, but only
**two** touched `crates/core/src/character/`: `1d0c5d4b` (leveling doc clarification +
Skyrim Magicka/Stamina population, 2026-08-24) and `4e1afcbe` (an `actor_values.rs`
docstring fix). None of `ruleset.rs`, `derived.rs`, `fallout.rs`, `tes.rs`, `skyrim.rs`,
`regen.rs`, `affliction.rs`, `resistance.rs`, `reputation.rs`, `components.rs`, `skill.rs`,
`attribute.rs`, or `profile.rs` changed at all.

**Tests**: `cargo test -p byroredux-core character` → **112 passed, 0 failed**.
`cargo test -p byroredux-plugin actor_value_derive` → **11 passed, 0 failed**.
(Workspace-wide bare `cargo test` was not run — a known unrelated `E0004` in
`crates/scripting/examples/fragment_coverage.rs:59` blocks it; per-crate checks used
instead, per this run's briefing.)

**Verification method**: static analysis + capture-document cross-check (as usual) **plus**
direct binary extraction from four shipped masters (`Skyrim.esm`, independent Python
walker written this session at `/tmp/audit/character/esm_walk.py`, since cleaned up) to
re-verify the AVIF space and one RACE record's `DATA` layout against the 2026-08-24 Skyrim
Magicka/Stamina addition.

| Dimension | Area | New findings this session |
|---|---|---|
| 1 | Ruleset Seam & CHARAL Doctrine | 0 |
| 2 | Derived-Stat Formulas | 0 (all unchanged, re-verified clean) |
| 3 | Leveling & Progression | 0 (leveling.rs change verified correct, matches "do not re-flag" note) |
| 4 | Pools, Afflictions, Resistances & Reputation | 0 (file unchanged since last sweep) |
| 5 | Population Boundary | 0 new (the 2026-08-24 Skyrim Magicka/Stamina addition verified correct against real game data; the one live bug in this area, `effective_npc_level`, is `Existing: #3171`) |
| 6 | Coverage, Documentation & Doctrine Drift | **1 LOW** (this skill file's own scope list) |
| **Total (NEW this session)** | | **0 CRITICAL · 0 HIGH · 0 MEDIUM · 1 LOW** |

Five findings from the 2026-08-20 sweep were re-checked against HEAD: **four are still
OPEN and unfixed** (`#3169`–`#3172`), **one (`#3216`) is fixed but not yet closed** — see
Cross-Audit Dedup below. None are re-filed.

---

## Executive Summary

### What changed since 2026-08-20, and is it correct?

The character crate had almost no churn this cycle — 106 of the 108 intervening commits
touched other subsystems (WATAL convergence, exterior streaming, AI pathing, morph
blending, renderer fixes). The two real changes:

1. **`1d0c5d4b` (2026-08-24, same day as this audit) — Skyrim NPC Magicka/Stamina
   population.** `derive_skyrim_actor_values` (`crates/plugin/src/esm/records/
   actor_value_derive.rs`) now resolves Health, Magicka, and Stamina **independently**,
   each from its own `RACE.DATA` starting value (`RaceRecord::starting_magicka` /
   `starting_stamina`, new `Option<f32>` fields at byte offsets 40/44, immediately after
   the existing Health field at offset 36) plus its own signed `NPC_.ACBS` offset
   (`magicka_offset` / `stamina_offset`, which were already-parsed fields with no prior
   consumer). **Verified correct**, three ways:
   - **Real-data byte layout**: I independently extracted `Skyrim.esm`'s `NordRace`
     (`0x00013746`) `RACE` `DATA` record with a from-scratch parser and decoded
     `f32 @ 36/40/44` = `50.0 / 50.0 / 50.0`. This confirms the three-float layout the
     code assumes (sequential `f32` triplet immediately after `race_flags`) — the byte
     offsets are structurally sound, not guessed.
   - **AVIF resolution**: the same independent parser confirms `Skyrim.esm` authors
     `AVMagicka` at `0x000003E9` and `AVStamina` at `0x000003EA` (`AVHealth` sits at
     `0x3E8`, already verified in the prior sweep) — exactly the FormIDs the new unit
     test (`skyrim_pools_are_race_starts_plus_signed_npc_offsets`) hardcodes.
   - **Per-pool independence** (the specific property this skill's Dimension 5 checklist
     calls out): the new loop does
     `index.actor_value_form_id(name).zip(starting)` per pool and `continue`s only that
     iteration on a miss — a race missing `starting_magicka` (or a load order missing the
     Magicka AVIF) cannot suppress Health or Stamina. Confirmed by inspection; there is no
     shared early-return across the three pools.

   The only gap: the "real thing" real-data test
   (`vanilla_skyrim_nordrace_data_decodes_to_its_documented_racials`,
   `crates/plugin/src/esm/records/actor/tests.rs:1162`) copies only the **first 36 bytes**
   of the real record and zero-fills the rest, so it never actually exercises the new
   Magicka/Stamina bytes against real data — the only test that does
   (`skyrim_race_data_uses_the_tes5_layout_not_tes4`) is hand-constructed. This is the same
   coverage gap `#3172` already tracks (extended to two more fields); not filed separately.

2. **`1d0c5d4b`'s leveling-doc change** — `LevelingModel::with_gmst` no longer reads
   `fXPPerSkillRank` (a GMST that, per this skill's "known-open, do NOT report as new"
   instruction, was withdrawn 2026-08-24 as a deliberate design settlement, not a bug).
   Verified the code (`leveling.rs:92-109`) and its regression test
   (`skyrim_gmst_overlay_reads_only_authored_curve_settings`) match the settled design
   exactly: only `fXPLevelUpBase`/`fXPLevelUpMult` are requested from `gmst`, pinned by an
   assertion on the exact set of GMST names the closure was called with.

Everything else in the crate — all Fallout/Oblivion/Skyrim derived-stat formulas, the
leveling models, regen, affliction, resistance, reputation, and the CHARAL-adjacent
`combat.rs`/`stealth.rs` siblings — is byte-for-byte unchanged since the previous sweep's
deep verification and was re-inspected this session without finding any regression.

### The one new finding

The skill file's own **Scope** section (`.claude/commands/audit-character/SKILL.md`) and
every dimension's **Entry points** list omit `crates/core/src/character/profile.rs`
entirely — the module that owns `CharacterRulesProfile`, `NpcHealthCurve`, `NpcStatModel`,
and `RulesetBuilder`, i.e. the exact data row Dimension 1's doctrine check, Dimension 3's
coverage matrix, and Dimension 5's population-boundary trace all depend on. It is only
reachable today via one Dimension 5 checklist sentence that names `CharacterRulesProfile`
in passing without citing its file. The in-code `mod.rs` docstring has the opposite
problem already fixed (`mod_docstring_indexes_every_sub_module`, #2958) — this is the same
drift class one level up, in the skill file that directs the audit rather than in the code
being audited. See CHAR-2026-08-24-D6-01 below.

### Verification honesty

- **Verified against shipped game data this session**: `Skyrim.esm`'s complete `AVIF`
  EditorID space (149 records, re-confirming `AVHealth`/`AVMagicka`/`AVStamina` at
  `0x3E8`/`0x3E9`/`0x3EA` and the still-wrong `Illusion`→`AVMysticism` retention at
  `0x45B`), and `NordRace`'s (`0x13746`) full 164-byte `RACE` `DATA` record.
- **Verified against capture documents**: every derived-stat formula in `fallout.rs`,
  `tes.rs`, `skyrim.rs`, and the `combat.rs`/`stealth.rs` CHARAL-adjacent siblings —
  unchanged since the last sweep's line-by-line verification; re-read in full this session,
  no drift found.
- **Not re-verified from scratch**: `FalloutNV.esm`/`Fallout3.esm`/`Fallout4.esm`'s AVIF
  spaces (unchanged code, unchanged prior verification — re-deriving would duplicate the
  2026-08-20 sweep's work for zero new information) and `crates/core/src/stealth.rs`
  (unchanged since the 2026-08-16 sweep verified it, consistent with that report's own
  "Not Covered" note).
- **Not re-derived**: the deferred FNV/FO3 tag-skill per-level formula, still absent, still
  not fabricated (`actor_value_derive.rs:23-28`'s deferral note is unchanged).

---

## Constant Verification Table (deltas only — see `AUDIT_CHARACTER_2026-08-20.md` for the
full 22-row table, which is unchanged and not reproduced here)

| # | Constant / lookup | Code value | Authoritative value | Source | Verdict |
|---|---|---|---|---|---|
| 23 | Skyrim `AVMagicka` FormID | `derive_skyrim_actor_values` resolves `"Magicka"` | `AVMagicka 0x000003E9` | game-data (`Skyrim.esm`, this session's independent parse) | **PASS** |
| 24 | Skyrim `AVStamina` FormID | `derive_skyrim_actor_values` resolves `"Stamina"` | `AVStamina 0x000003EA` | game-data (`Skyrim.esm`) | **PASS** |
| 25 | `RACE.DATA` Magicka/Stamina byte offsets | `f32 @ 40` / `f32 @ 44`, immediately after Health `@ 36` | `NordRace` (`0x13746`) decodes to `50.0 / 50.0 / 50.0` at those three offsets — a clean, plausible sequential triplet, consistent with the OpenMW-derived layout comment already covering the Health offset | game-data (`Skyrim.esm`, this session) | **PASS** — structural layout confirmed, no documented "correct value" to compare against (raw parsed data, not an engine-owned coefficient — the CHARAL no-guessing rule governs formula *coefficients*, not faithfully-transcribed record bytes) |
| 26 | Skyrim `Illusion` skill roster entry | `SkillDef::ungoverned("Illusion")` (`skill.rs:139`) | Vanilla `Skyrim.esm` authors this AVIF slot (`0x45B`) as `AVMysticism`, not `AVIllusion`/`Illusion` | game-data (`Skyrim.esm`, re-confirmed this session) | **FAIL, still unfixed** → `Existing: #3169` |

---

## Coverage Matrix

Unchanged from `AUDIT_CHARACTER_2026-08-20.md` in every column except the Skyrim
"NPC stat model" note, which now reads independently per pool:

| Game | Capture doc | Profile row | Ruleset builder | Ruleset **wired** | Derived rows in code | NPC stat model | Leveling model | Regen wired | Affliction wired |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| **Oblivion** | ✓ | `OBLIVION` | `oblivion_ruleset` | ✗ (`RulesetBuilder::None`) | 8 (5 stats) | `None` | `OBLIVION` | builder exists, no caller | ✗ |
| **FO3** | ✓ | `FALLOUT3` | `fallout3_ruleset` | ✓ | 8 | `ClassAutoCalc` (90/20/10) | `FO3` | ✗ | ✗ |
| **FNV** | ✓ | `FALLOUT_NEW_VEGAS` | `falloutnv_ruleset` | ✓ | 8 | `ClassAutoCalc` (95/20/5) | `FNV` | ✗ | ✗ |
| **Skyrim SE** | ✓ | `SKYRIM` | `skyrim_ruleset` | ✗ (`RulesetBuilder::None`) | 2 (unreachable) | `RaceBaseOffsets` — **Health, Magicka, Stamina now each land independently** (new this cycle) | `SKYRIM` (unreachable) | ✗ | ✗ |
| **FO4** | ✓ | `FALLOUT4` | `fallout4_ruleset` | ✓ | 3 | `Stored` (`PRPS` + `DNAM`) | `FO4` | ✗ | ✗ |
| **FO76** | ✓ | `FALLOUT76` | ✗ | ✗ | — | `Stored` | ✗ | ✗ | ✗ |
| **Starfield** | ✓ | `STARFIELD` | ✗ | ✗ | — | `Stored` | ✗ | ✗ | ✗ |

The Skyrim NPC-pool change is the only cell that moved. Skyrim's *ruleset* (the derived-
stat/leveling `Resource`) remains unwired (`RulesetBuilder::None`, per `#3170`); the
*population* half (raw actor-value stamping from race + ACBS) now covers three pools
instead of one.

---

## Findings

### CHAR-2026-08-24-D6-01: `/audit-character`'s own Scope section and dimension entry-point lists never name `profile.rs`

- **Severity**: LOW
- **Dimension**: Coverage, Documentation & Doctrine Drift
- **Game**: all
- **Location**: `.claude/commands/audit-character/SKILL.md` — the `## Scope` section
  (`crates/core/src/character/` file bullets) and the **Entry points** lines for
  Dimensions 1, 3, and 5
- **Status**: NEW
- **Description**: `crates/core/src/character/profile.rs` defines `CharacterRulesProfile`
  (the single per-game policy row selected at the parser boundary — attribute/skill
  roster, `NpcStatModel`, `RulesetBuilder`, `NpcHealthCurve`), re-exported from
  `character::mod` and indexed in that module's own docstring (which is regression-tested
  by `mod_docstring_indexes_every_sub_module`, #2958's fix). The skill file that directs
  this audit never lists `profile.rs` as a file to read: the `## Scope` bullet list names
  `ruleset.rs`, `derived.rs`, `leveling.rs`, `attribute.rs`/`skill.rs`,
  `fallout.rs`/`tes.rs`/`skyrim.rs`, `regen.rs`, `affliction.rs`/`resistance.rs`,
  `reputation.rs`, `components.rs`, and the substrate files — fourteen sub-modules exist,
  thirteen are named there. Dimension 1's entry points list `ruleset.rs`; Dimension 3's
  list `leveling.rs`/`skyrim.rs`/`tes.rs`; Dimension 5's list mentions
  `CharacterRulesProfile` by name in one checklist sentence about
  `derive_skyrim_actor_values`, but never cites `profile.rs` as a file to open. An auditor
  working strictly from the skill's own file lists (rather than from `grep`-ing the crate,
  as this session did) would never read the file that owns the single construction seam
  Dimension 1's doctrine check and Dimension 3's coverage matrix both depend on.

  This is the same drift class the code side already caught and fixed for itself
  (`mod_docstring_indexes_every_sub_module`, regression-tested in `mod.rs`) — but the audit
  skill directing the audit has no equivalent self-check, and it now has the identical gap.
- **Evidence**: `grep -c "profile.rs" .claude/commands/audit-character/SKILL.md` → `0`.
  `ls crates/core/src/character/*.rs | wc -l` → 14 files (13 modules + `mod.rs`); the
  Scope section's file bullets enumerate 13 non-`mod.rs` files, none of them `profile.rs`.
- **Impact**: Process risk, not a runtime bug. `profile.rs` happens to have been read this
  session anyway (via `grep`-driven exploration rather than the skill's own file list), so
  no verification gap resulted this cycle — but a future shallower run (or one following
  the skill's explicit file lists more literally) could skip it entirely, exactly the
  failure mode this skill exists to prevent for the *code* under audit.
- **Related**: #2958 (CLOSED — the code-side equivalent of this drift, in `mod.rs`'s own
  docstring).
- **Suggested Fix**: Add a `profile.rs` bullet to the `## Scope` section (its role:
  `CharacterRulesProfile`, `NpcHealthCurve`, `NpcStatModel`, the per-game policy row
  selected at the parser boundary) and cite it explicitly in Dimension 1's and Dimension
  5's entry-point lists, since both dimensions' checklists already describe behavior that
  lives there.

---

## Cross-Audit Dedup

Re-checked at HEAD `048a8bd8`, all five 2026-08-20 findings:

| Finding | Status this session |
|---|---|
| `#3169` (CHAR-2026-08-20-D2-01) — `SkillSet::SKYRIM` spells Illusion `"Illusion"`; vanilla `Skyrim.esm` authors `AVMysticism` at that slot | **Still OPEN, re-confirmed against real `Skyrim.esm` bytes this session** (`skill.rs:139` unchanged; `0x45B` = `AVMysticism`, no `Illusion`/`AVIllusion` skill AVIF exists). Not re-filed. |
| `#3170` (CHAR-2026-08-20-D3-01) — the GMST-sourcing seam (`with_gmst`) has zero production reach because `RulesetBuilder::None` for Skyrim | **Still OPEN, re-confirmed**. `profile.rs:106` still sets `ruleset: RulesetBuilder::None` for `SKYRIM`; `build_ruleset` still returns `None` before reaching `with_gmst` for that arm. `1d0c5d4b`'s GMST-comment fix narrowed *what* `with_gmst` would read if it ever ran, but did not add a `Skyrim` arm to `RulesetBuilder` — the seam is still unreachable in production. Not re-filed. |
| `#3171` (CHAR-2026-08-20-D5-01) — `effective_npc_level` is a third copy of `effective_actor_level` carrying the `.max(1)` divergence #3081 declared wrong | **Still OPEN, re-confirmed**. `actor_value_derive.rs:215-221`'s `effective_npc_level` still reads `npc.level.max(1) as u16` on the non-multiplier branch, diverging from `npc_spawn.rs:142-148`'s canonical `effective_actor_level`'s `.max(0)`. Also newly confirmed as the level source feeding `derive_npc_actor_values`'s `resolve_inherited_stats` call (`actor_value_derive.rs:160`) — the template-tier impact the original finding described. Not re-filed. |
| `#3172` (CHAR-2026-08-20-D6-01) — the real-data existence test covers one roster (FNV) out of five, and no derived-row output key on any game | **Still OPEN, re-confirmed, marginally worse in scope**. `crates/plugin/tests/parse_real_esm.rs` still loops only `SkillSet::FALLOUT_NV`. Additionally, the new `vanilla_skyrim_nordrace_data_decodes_to_its_documented_racials` real-data test (added alongside the Magicka/Stamina fields) copies only the first 36 real bytes and zero-fills the rest, so the two new fields (`starting_magicka`/`starting_stamina`) join `starting_health` as parsed-correctly-per-independent-verification-this-session but *not* covered by any in-repo real-data test. Not re-filed — this is the same finding, with two more untested fields under it. |
| `#3173` (CHAR-2026-08-20-D2-02) — the two GMST names cited for skill auto-calc coefficients (`fAVDSkillPrimaryBonusMult`/`fAVDSkillLuckBonusMult`) are not authored by any shipped Fallout master | **Still OPEN, re-confirmed**. `actor_value_derive.rs:87-90`'s `SKILL_ATTR_MULT`/`SKILL_LUCK_MULT` constants still carry the same comment naming those two GMSTs; the code itself is unchanged since 2026-08-20. Not re-filed. |
| `#3216` (REG-2026-08-20-D6-01) — `#2987` removed the Skyrim engine-enum key space, but `ActorValues`' contract doc still declared it | **FIXED, not yet closed on GitHub.** `4e1afcbe` rewrote `actor_values.rs:13-19`'s docstring to state the restored single-key-space rule and cite `#2987`, and `byroredux/src/commands_tests.rs:563`'s `ActorVitals { health: 24 }` fixture was changed to `health: 0x0000_03E8`. Both completeness-check items from the issue body are satisfied. **Recommend closing `#3216`** — not this audit's job to do so directly, flagged here for `/audit-publish` or manual close. |

No new component-storage, ESM/CTDA, or scheduler-access findings this session — Dimensions
1/2/4's re-inspection found nothing to hand off to `/audit-ecs`, `/audit-esm`, `/audit-scripting`,
or `/audit-concurrency` beyond what those audits already own.

---

## Known-Open Register (confirmed NOT re-filed)

| Deferred item | Status this audit |
|---|---|
| FNV/FO3 **tag-skill per-level** formula (undocumented) | Still absent, still not fabricated. `actor_value_derive.rs`'s deferral note unchanged. |
| FO3↔FNV divergent **player** Health/AP | NPC half resolved (per-profile Health curves); player actor still deferred. Not re-filed. |
| **VATS runtime** (AP pool/regen, time-pause, limb health, hit-chance roll) | Not re-filed. Still formulas only — no new VATS-related commits this cycle. |
| CLAS SPECIAL lives in `ATTR`, not `DATA` | Confirmed correct in code, unchanged. |

---

## Disproved Candidates (investigated, not reported)

- **"The new `starting_magicka`/`starting_stamina` `> 0.0` guard incorrectly excludes a
  legitimately-authored `0.0` Magicka/Stamina race."** True as a theoretical edge case, but
  this exactly mirrors the pre-existing `starting_health` guard (same file, same pattern,
  never flagged for Health) and the per-pool independence design means a `0.0`-Magicka race
  simply skips *that one pool*, not the whole NPC. Not a regression, not a new defect —
  consistent with established precedent. Not filed.
- **"`derive_skyrim_actor_values`'s three-pool loop could double-count if `actor_value_form_id` resolves the same FormID for two different names."** Checked: `"Health"`/`"Magicka"`/`"Stamina"` resolve to three distinct, non-colliding AVIF FormIDs on real `Skyrim.esm` data (`0x3E8`/`0x3E9`/`0x3EA`, verified this session) — no collision, and the loop pushes at most one `(key, value)` pair per iteration into a `Vec`, not a map, so even a hypothetical collision would produce two entries rather than silently overwriting. Not a defect.
- **"`RaceRecord::starting_magicka`/`starting_stamina` should also be threaded into `oblivion_magicka_formula`'s TES-classic Magicka path."** Different family entirely — Oblivion's Magicka is attribute-derived (`2×Intelligence`, `tes.rs`), not race-stored. No cross-wiring gap; the two mechanisms are correctly kept separate per each family's own model.

---

## Not Covered

- **FO76 / Starfield**: no ruleset builder or NPC stat model exists beyond the explicit
  `Stored` / `RulesetBuilder::None` placeholders in `profile.rs` — unchanged from the prior
  sweep, nothing new to verify.
- **Oblivion's pre-`AVIF` legacy actor-value index resolution**: `Oblivion.esm` has no
  `AVIF` group; the legacy-index resolver the roster docstring describes still does not
  exist. Unchanged, out of reach of a data probe.
- **`crates/core/src/stealth.rs`**: re-read in full this session (module docs + all 15
  tests) but not independently re-derived against `charal-fnv-fo3-ruleset.md`'s Sneak
  Detection section — unchanged since the 2026-08-16 sweep verified it line-by-line, and
  the file has had zero commits since.
- **`FalloutNV.esm`/`Fallout3.esm`/`Fallout4.esm` AVIF spaces**: not independently
  re-extracted this session (only `Skyrim.esm` was, to verify the one real code change).
  The 2026-08-20 sweep's extraction of all three stands unchanged, since none of the
  consuming code (`fallout.rs`) changed.

---

## Suggested Fix Order (spanning this session's one new finding + the still-open carry-forward)

1. **`#3172`** — generalize the real-data existence loop to all five rosters and every
   derived-row output key; now also cover `starting_magicka`/`starting_stamina` in the
   Skyrim real-data test rather than only the first 36 bytes. Cheapest fix, validates the
   other roster-identity findings, and is the only one that stops this class recurring.
2. **`#3169`** — `"Illusion"` → `"Mysticism"`; two-line change, pinned by #1's generalized loop.
3. **`#3171`** — collapse `effective_npc_level` into the shared `effective_actor_level` on
   `.max(0)`, finishing `#3081`.
4. **`#3170`** — give `RulesetBuilder` a `Skyrim` arm so the (now-narrowed, correctly
   scoped) GMST seam is reachable at all.
5. **`#3173`** — correct the two unauthored GMST names in the skill auto-calc comment.
6. **`#3216`** — close on GitHub; the fix is verified in place.
7. **CHAR-2026-08-24-D6-01** — add `profile.rs` to this skill's own Scope section and the
   Dimension 1/5 entry-point lists.

TALLY (NEW this session): CRITICAL=0 HIGH=0 MEDIUM=0 LOW=1
