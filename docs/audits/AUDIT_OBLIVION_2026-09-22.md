# Oblivion (TES4) Compatibility Audit — 2026-09-22

**HEAD**: `ee6d3fb39` · **Baseline**: `docs/audits/AUDIT_OBLIVION_2026-09-11.md` · **Audited**: Dimensions 1-5 (full delta re-verification; no dimension had zero commits since baseline) · **Unchanged since baseline (skimmed)**: none — every dimension had Oblivion-relevant commits in the window and was re-verified

Orchestrator audit covering the 5 dimensions defined in
`.claude/commands/audit-oblivion/SKILL.md`: NIF version handling & corpus
integrity, BSA v103 & ESM data slice, legacy-property rendering, exterior &
lighting data, and gameplay/UI data (M47.3 ObScript quests + MenuXml HUD).
Run solo (no sub-agents), one dimension at a time, against real vanilla
Oblivion + DLC data at `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/`.

A prior run of this audit was killed by a session restart after completing
only Dimension 1; that partial output (`/tmp/audit/oblivion/dim_1.md`) was a
complete, live-run analysis and was re-verified quickly (its two headline
gates re-confirmed green) rather than redone. Dimensions 2-5 were run fresh.

## Executive Summary

**Compatibility level, as measured live during this audit:**

- **NIF parse (incl. v10.x tail)**: corpus lane green — 0 truncating, 0
  unknown, per-block histogram parity across base + 8 DLC archives
  (`per_block_baseline_oblivion`, `oblivion_block_count_parity`,
  `parse_rate_oblivion`, `no_block_sizes_drift_detector_has_zero_false_positives_on_real_corpus`,
  all PASS, live-run 2026-09-22). No corpus regression since 2026-09-11.
- **BSA v103 archive extraction**: still 100% clean — 147,629 files across
  17 vanilla + DLC archives, 0 extraction errors, live-re-run this audit.
- **ESM parse**: both live real-data parity pins
  (`clas_oblivion_knight_against_vanilla`,
  `race_oblivion_data_and_subs_against_vanilla`) re-run and green. All
  Oblivion-specific decode branches touched since baseline (16-byte ACBS,
  XCLL game-validated ambient-cube arm, placement `group_type` re-derivation,
  ATXT/VTXT terrain-layer pairing) were reviewed against source and confirmed
  correctly fixed.
- **Render path**: the historical highest-yield dimension (legacy-property
  rendering / APPLY_HILIGHT2 parallax) holds — `parallax_alpha_gate` (5/5)
  and the full `byroredux-nif` material-translation suite (247/247) both
  green, live-run this audit. Disney-BSDF gate (`MAT_FLAG_PBR_BSDF` provably
  0 for Oblivion, #2570) unchanged.
- **Exterior & lighting**: the three gaps this audit's own checklist named as
  open at 2026-09-11 — the unconsumed HNAM sunlight dimmer, the wrong
  falloff-exponent default on pre-Skyrim LIGH, and Oblivion's broken
  default/`ICON`-rooted land-texture paths — are **all closed** since
  2026-09-18, and this audit re-verified all three live against real
  archives (`oblivion_ltex_paths_exist_in_vanilla_archives`: 229 paths, 3
  known unshipped; `default_land_textures_exist_in_vanilla_archives`: 11
  keys), not just by reading the diffs.
- **Gameplay & UI**: M47.3 (ObScript quest execution) and the MenuXml
  Oblivion HUD are both brand-new since baseline (landed 2026-09-18). Read
  the ObScript interpreter in full for untrusted-input safety (mod-authored
  compiled bytecode) — no panics, no unbounded loops, every read
  bounds-checked. Investigated a candidate target-resolution ambiguity in
  the quest-command host (SetStage/StartQuest/etc. reading `args[0]` instead
  of the VM's `caller` parameter for the dotted-call form) with a real-corpus
  census tool built for this audit; **disproved as a live Oblivion issue** —
  zero of 2,393 vanilla scripts use the dotted form for any of the six
  watched commands. Recorded for cross-audit awareness (shared VM, other
  games' corpora unverified here), not filed as an Oblivion finding.

**No new CRITICAL / HIGH / MEDIUM / LOW findings this run.** Every item this
audit's own 2026-09-11 checklist and dimension descriptions named as open
has been closed and independently re-verified against live tests and real
game data during this run.

## Dimension Findings

### Dimension 1: NIF Version Handling & Corpus Integrity
Corpus lane PASS (live-run). All three prior findings (APPLY_HILIGHT2
binding/alpha-gate mismatch HIGH, particle-emitter first-match mis-
attribution MEDIUM, missing `BsFaceGenNiNode` LOW) confirmed fixed by
`b3237e65a` (2026-09-12) and refined by `387e65616` (2026-09-21, authored-
zero emitter budget). All header/version-gating checklist items (dual
BSStreamHeader band #170, `has_object_group_id`, `uses_inline_block_type_names`,
`NiTexturingProperty` u32 shader-count, `NiGeomMorpherController` bsver
gate, sizeless-recovery plausibility check) re-confirmed unchanged in
source. New commits since baseline reviewed (`3dc127d0b` UB fix,
`ab8779a52` tangent pre-sizing, `3e798cacd` NaN/Inf gating, `c8dcb3694`
mipmap over-allocation hardening) — none introduce an Oblivion-specific
regression. Full detail: `/tmp/audit/oblivion/dim_1.md`.

### Dimension 2: BSA v103 & ESM Data Slice
Both headline real-data gates green (live-run). 18 commits since baseline
reviewed. Highlights: `efe15ceaf` correctly game-validates the >=92-byte
XCLL ambient-cube arm off Oblivion/FO3/FNV (a non-canonical 92+ XCLL could
previously misread its tail); `c7ff0703a` fixes placement `group_type` being
threaded unchanged through nested GRUPs (measured 1,791+33,766+11,134
mis-bucketed Oblivion placements pre-fix); `b93262c75` extends NaN/Inf/huge
gating to WRLD water heights and VTXT opacity. All already fixed and
correctly scoped. No new findings. Full detail: `/tmp/audit/oblivion/dim_2.md`.

### Dimension 3: Legacy-Property Rendering Path
`parallax_alpha_gate` (5/5) and the full material test suite (247/247) both
green, live-run. 39 commits reviewed (highest-churn dimension) — the bulk
are already-landed 2026-09-12 audit fixes independently confirmed by
Dimension 1; the remainder are FO4/Starfield/Skyrim-scoped BGSM/BGEM work
confirmed inert on Oblivion's legacy `NiTexturingProperty` path, plus one
Oblivion-relevant hardening fix (`c9d4f007c`, #4301: normal-map texture
flips now gate alpha-dependent reads on the *active frame's* alpha
presence, not spawn-time state) reviewed and confirmed correct including
its save-format-guard update. No new findings. Full detail:
`/tmp/audit/oblivion/dim_3.md`.

### Dimension 4: Exterior & Lighting Data (Tamriel)
All three checklist items open at baseline are now closed and re-verified
live against vanilla archives: HNAM sunlight dimmer (`df59c6362`, consumer
in `systems/weather.rs` confirmed, both steady-state and transition paths),
LIGH falloff-exponent sentinel (`6b4e6252c`, `canonical_light_falloff_exponent`
confirmed wired at 3 real call sites, not just defined-and-tested), and
Oblivion default/`ICON`-rooted land textures (`4da9db11c`, both real-data
gates re-run: 229 LTEX paths / 11 default-texture keys, all resolve against
real archives). No new findings. Full detail: `/tmp/audit/oblivion/dim_4.md`.

### Dimension 5: Gameplay & UI Data Slice (M47.3, MenuXml HUD)
Both subsystems are new since baseline (landed 2026-09-18). Real-data quest
gate green. Read the ObScript interpreter (1,101 lines) end to end for
untrusted-input discipline — clean: every read bounds-checked, every loop
provably terminates on malformed input via `BlockOutcome::Malformed`, no
panics in production code. Investigated and **disproved** (via a purpose-
built real-corpus census tool, `/tmp/audit/oblivion/scpt_scan/`, not
committed) a candidate HIGH finding: the quest-command host's target
resolution reads `args[0]` rather than the VM's `caller` parameter for
SetStage/StartQuest/StopQuest/GetStage/GetStageDone/GetQuestRunning, which
would misfire under the dotted explicit-caller call form (`<quest>.SetStage
<n>`) — the module's own doc comment describes this form, but a full walk
of all 2,393 vanilla `Oblivion.esm` scripts' compiled bytecode found **zero**
explicit-caller invocations of any of the six commands; every SetStage is
the 2-argument flat form the code handles correctly. Recorded for
cross-audit awareness (shared VM code; FO3/FNV/Skyrim corpora unverified
here) but not filed as an Oblivion finding. `HudGameProfile::oblivion()`
matches the SKILL checklist verbatim, including the documented (not hidden)
ActorValue-key limitation. No new findings. Full detail:
`/tmp/audit/oblivion/dim_5.md`.

## Regression Guard List (unchanged from 2026-09-11, all still hold)

- Stride-drift family #1506-#1509 — corpus lane green, 0 truncating.
- #170 dual BSStreamHeader band — unchanged in source.
- `NiTexturingProperty` unconditional `u32` shader-map count — unchanged.
- BSA v103 sweep — 147,629 files, 0 errors, live-re-run.
- `parallax_alpha_gate_tests` — 5/5, live-re-run.
- Disney-BSDF gate (`MAT_FLAG_PBR_BSDF` == 0 for Oblivion, #2570) — unchanged.
- 16-byte Oblivion ACBS (#1650) — unchanged.

## Open Work

No Oblivion-specific blockers. Interiors and the Tamriel exterior already
render end-to-end per `ROADMAP.md` and `docs/engine/exterior-readiness-plan.md`;
neither is a blocker. `docs/smoke-tests/p0-door-interaction.sh oblivion`
remains the only playable gate `oblivion.env` declares (confirmed current —
`docs/smoke-tests/fixtures/oblivion.env` matches the SKILL's description
verbatim); it needs a Vulkan device and was not run under this audit's
no-engine-launch constraint. M47.3 quest execution is explicitly phase 1
(Message/MessageBox UI and actor-state functions are documented as phase 2,
not a defect). Dedup'd against sibling 2026-09 audit reports per the task
brief: ESM-D2-01 (leveled item lists, `equip.rs:788`), COORD-03
(`--rotation-mode` doc claim), GAME-D5-2026-09-21-01 (ambient-package
misrouting), the exterior/FaceGen/MenuXml findings cited in
`AUDIT_EXTERIOR_2026-09-21.md` and `AUDIT_PARSERS_2026-09-21.md` — none
re-reported here.

## Statistics

- Dimensions audited: 5/5 (all had commits since the 2026-09-11 baseline).
- Commits reviewed across all dimensions: ~80 (with de-duplication across
  dimension boundaries where a commit touched more than one Path set).
- Real-data gates re-run live this session: 8 (`bsa_real` ×2,
  `parse_real_esm` ×2, `parallax_alpha_gate` ×5, `oblivion_ltex_paths_exist_in_vanilla_archives`,
  `default_land_textures_exist_in_vanilla_archives`,
  `vanilla_oblivion_quest_scripts_execute_and_setstage`, plus the
  4-test corpus lane and the 247-test material suite).
- New findings: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 0 LOW.
- Existing findings re-confirmed fixed: 3 (Dim 1 carryover) + 3 (Dim 4
  checklist) = 6.
- Candidate finding investigated and disproved via real-corpus measurement:
  1 (Dim 5, SetStage target resolution).

Suggest: `/audit-publish docs/audits/AUDIT_OBLIVION_2026-09-22.md` — though
with zero new findings there is nothing new to publish as issues this run.
