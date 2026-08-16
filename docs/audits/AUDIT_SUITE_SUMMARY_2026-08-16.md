# Audit Suite Summary — comprehensive — 2026-08-16

25/25 audits complete. **0 CRITICAL · 25 HIGH · ~68 MEDIUM · ~51 LOW** (~144 findings).

Severity counts below are grepped from the reports themselves, not relayed. The
HIGH column is exact and matches every agent's self-report. MEDIUM/LOW carry a
±2 variance from per-report formatting differences.

| Audit | C | H | M | L | Report |
|---|---|---|---|---|---|
| ESM | 0 | 3 | 0 | 0 | `AUDIT_ESM_2026-08-16.md` |
| FO4 | 0 | 3 | 5 | 0 | `AUDIT_FO4_2026-08-16.md` |
| Runtime | 0 | 3 | 6 | 1 | `AUDIT_RUNTIME_2026-08-16.md` |
| FNV | 0 | 2 | 2 | 3 | `AUDIT_FNV_2026-08-16.md` |
| Physics | 0 | 2 | 1 | 1 | `AUDIT_PHYSICS_2026-08-16.md` |
| Save | 0 | 2 | 5 | 2 | `AUDIT_SAVE_2026-08-16.md` |
| Scripting | 0 | 2 | 6 | 2 | `AUDIT_SCRIPTING_2026-08-16.md` |
| Skyrim | 0 | 2 | 2 | 0 | `AUDIT_SKYRIM_2026-08-16.md` |
| FO3 | 0 | 1 | 1 | 1 | `AUDIT_FO3_2026-08-16.md` |
| Legacy-compat | 0 | 1 | 3 | 0 | `AUDIT_LEGACY_COMPAT_2026-08-16.md` |
| Renderer | 0 | 1 | 1 | 3 | `AUDIT_RENDERER_2026-08-16.md` |
| SpeedTree | 0 | 1 | 2 | 2 | `AUDIT_SPEEDTREE_2026-08-16.md` |
| Starfield | 0 | 1 | 3 | 2 | `AUDIT_STARFIELD_2026-08-16.md` |
| UI | 0 | 1 | 4 | 6 | `AUDIT_UI_2026-08-16.md` |
| Character | 0 | 0 | 4 | 0 | `AUDIT_CHARACTER_2026-08-16.md` |
| ECS | 0 | 0 | 4 | 3 | `AUDIT_ECS_2026-08-16.md` |
| Regression | 0 | 0 | 4 | 1 | `AUDIT_REGRESSION_2026-08-16.md` |
| Tech-debt | 0 | 0 | 4 | 9 | `AUDIT_TECH_DEBT_2026-08-16.md` |
| NIF | 0 | 0 | 3 | 0 | `AUDIT_NIF_2026-08-16.md` |
| NIFAL | 0 | 0 | 2 | 2 | `AUDIT_NIFAL_2026-08-16.md` |
| Safety | 0 | 0 | 2 | 3 | `AUDIT_SAFETY_2026-08-16.md` |
| Audio | 0 | 0 | 1 | 2 | `AUDIT_AUDIO_2026-08-16.md` |
| Concurrency | 0 | 0 | 1 | 2 | `AUDIT_CONCURRENCY_2026-08-16.md` |
| Oblivion | 0 | 0 | 1 | 1 | `AUDIT_OBLIVION_2026-08-16.md` |
| Performance | 0 | 0 | 1 | 5 | `AUDIT_PERFORMANCE_2026-08-16.md` |

---

## The finding behind the findings

The dominant result is not any single bug. It is that **this codebase's
verification layer is systematically green by construction** — checks that
structurally cannot fail, certifying health they never examined.

`/audit-regression` sampled 27 closed fixes and found **27/27 physically present,
zero code regressions**. The fixes are real. What is failing is the layer that
proves they still work.

Thirteen confirmed instances:

| # | Where | The green signal | What it could not see |
|---|---|---|---|
| 1 | Physics | Each crate's tests pin its own half of a shared contract | `scale²` seam between #2860 + #2868 |
| 2 | Physics | Same, second site | `scale²` on ragdoll limbs |
| 3 | CHARAL | Fixtures build resolvers from the roster's own strings | Cannot falsify the roster |
| 4 | UI | `KNOWN_MISSING_ON_DESTROY_TRAIT` exclusion list | 4 vanilla FO4 menus that cannot load |
| 5 | SpeedTree | Corpus gate measures parse success | Billboards never face camera; #994 fixed the wrong entity |
| 6 | Save | `FORMAT_MAJOR` guard matches bare `#[serde(...)]` only | 2 live `serde(default)` fields shipped under the house `cfg_attr` form |
| 7 | Renderer | Verified on a single-heap dev GPU | Budget collapse on the multi-heap hardware #2928 targeted |
| 8 | Scripting | `m47-triggers.sh` passes `--cell` | Exterior QF_ fragments never run |
| 9 | NIF corpus | 14881/14881 "clean" — files parse | ~980 meshes discarded *after* parsing |
| 10 | Inventory | Guard test builds its fixture from the same wrong constant | `PLAYER_BASE_FORM_ID` is the player *ref*, not base |
| 11 | Tech-debt | Oversized-file recipe measures total LOC | Flags test files; misses production halves |
| 12 | Tech-debt | Dedup glob (hyphen vs underscore) | 3 prior reports invisible to dedup |
| 13 | Regression | `run_skinning_invariant` behind three `_check`-named tests | Contains zero assertions |

Two worse-than-blind cases:

- **Oblivion `OBL-BR-01`** — a regression test asserts the *wrong* `BSXFlags`
  bit-5 semantics and will actively **block** the `FNV-D1-01` fix. A guard
  enforcing the bug. (`/audit-regression` searched for siblings of this shape
  and found none, so it is isolated.)
- **Runtime `RT-02`** — `p0-door-interaction.sh` and `p1-character-traversal.sh`
  are deterministically RED since `eb5d76fe` (HEAD~2). `p2` is green only
  because it greps the new wording.

---

## Cross-cutting defects

### 1. `BSXFlags` bit 5 drops whole NIFs — ~980 meshes, 4 games

`nif.xml` documents bit 5 as "EditorMarkers **present**" (the file *contains* a
marker). The engine reads it as "this file *is* a marker" and discards the
entire NIF.

| Game | Meshes dropped | Notes |
|---|---|---|
| Skyrim | 687 | Apocrypha animated hallways, Dwemer traps |
| FNV | 223 | 39-mesh Vertibird, traps, terminals, furniture |
| Oblivion | 70 | placed **5,112×** across 536 interior + 505 exterior cells |
| FO3 | affected | unquantified |

Invisible to every corpus baseline, because the files parse perfectly and are
discarded afterward. Filed as `FNV-2026-08-16-D1-01`.

### 2. The P2 gameplay slice is inert on every game — four independent causes

| Cause | Scope | Finding |
|---|---|---|
| `AVIF` EditorIDs are `AV`-prefixed; `actor_value_form_id` does exact match, no prefix stripping exists | 1,647/1,647 FO3 + 3,816/3,816 FNV NPCs have **no** `ActorValues`/`ActorVitals` | `ESM-D7-01` |
| WEAP `DNAM` (132 B) never read; ARMO `weight`/`health` swapped | Every FO4 weapon 0 damage, every armor 0.0 weight | `FO4-D6-01`, `FO4-D6-02` |
| `PLAYER_BASE_FORM_ID = 0x14` is the player *reference*; base is `0x07` | **All games** — player spawns with empty inventory, no armor, no `EquippedWeapon` | `FO3-D4-01` |
| `EquippedWeapon` has no runtime writer; `combat.rs` bypasses CHARAL | All games | ECS + `CHAR-D1-01` |

Live probe: **`combat.approach` rejects every FNV NPC.** The `p2-melee-core`
gate passes anyway because `combat.rs` hardcodes `UNARMED_DAMAGE = 8.0` — the
one damage path that reads no game data. Runtime additionally measured the
fixture player falling to **y = −24,781** after `combat.approach`.

Note: `_audit-common.md` itself documents the design as "seeds the player from
base NPC_ `0x00000014`", so the spec, the code and the test all agree with each
other and all disagree with the game data. Correct the doc alongside the code.

### 3. `slot_to_role` — one table, wrong wherever it runs

The 2026-08-14 texture-role unification collapsed six per-game tables into one
`slot_to_role(shader_type, slot, model_space_normals)` — **no `bsver`
parameter** (verified directly).

| Game | Verdict |
|---|---|
| Skyrim | **Wrong** — slot 2 bound as glow map without reading `SLSF2_Glow_Map`; 4,922/6,253 properties mis-roled |
| FO4 | **Wrong** — slot 3 (greyscale→palette gradient) routed into POM height on 31,303 properties; fully live (`material_reference` 0/810,489) |
| Starfield | **Moot** — unreachable in all three producers; CDB produces zero roles (#2359) |

Not "Skyrim's rules applied everywhere" — a table wrong on **both** games where
it actually runs.

---

## Coverage

All 24 crates in the `_audit-common.md` owner map were covered. Un-owned
subsystems were folded into sibling audits by explicit scope instruction, and
**every one of them yielded findings on first contact**:

| Subsystem | Folded into | Result |
|---|---|---|
| P2 gameplay slice | `/audit-ecs`, `/audit-runtime` | 2 MEDIUM + 3 HIGH |
| `crates/mod-runtime` | `/audit-safety` Dim 11 | 1 MEDIUM (**first pass ever**) |
| `crates/hkx` | `/audit-scripting` Dim 8 | 4 findings incl. 1 HIGH (**first pass ever**) |
| `crates/debug-server`/`protocol` | `/audit-concurrency` Dim 7 | full pass, 1 candidate withdrawn as dup of #2388 |
| `crates/facegen` | `/audit-safety`, `/audit-skyrim` | 1 MEDIUM; swept clean from Skyrim |
| `crates/fsr3-sys` | `/audit-safety` Dim 1, `/audit-renderer` Dim 23 | clean |

**Scope caveats:**
- `/audit-runtime`: `starfield` arm **SKIPPED** (no cell baseline, empty profile
  archives). `wall_fps` **NOT MEASURED** on 4 of 5 arms — reported as unmeasured
  rather than estimated.
- `/audit-legacy-compat`: the Gamebryo 2.3 mount was **unavailable**; Dim 7 ran
  against `docs/legacy/` + `nif.xml` instead.
- `/audit-skyrim`: no engine launched, so the Whiterun control bench did not run.

## Tracker corrections

- **#2574** — no longer reproducible; recommend **close**. Independently
  confirmed by `/audit-regression`, which also corrected `/audit-nif`'s commit
  attribution: the fix is `c1dd2e07` (2026-08-08), not `c41e87d8`.
- **#2564** — confirmed still valid (live 1 truncation vs baseline 6).
- Oblivion truncations are **1/8032**, not the documented 6.
- CHARAL's real footprint is **one game (FO4), three live derived rows** — not
  the "five complete rulesets, two wired" the 2026-08-15 matrix claims.
- `docs/feature-matrix.md` says native menus are "Not planned" while a
  three-page native game menu ships.
- `/audit-fo4` corrected a **false negative** in `AUDIT_LEGACY_COMPAT_2026-08-16.md`:
  `parse_armo`'s FO4 bucketing was cleared on byte-length alone.

## Method notes

- ~70 candidate findings were raised, disproved against current code, and
  recorded in per-report "Disproved Candidates" sections so later sweeps do not
  re-derive them. This directly targets the historical ~1-in-6 stale-finding rate.
- Two of the orchestrator's own working hypotheses were corrected by agent
  measurement: Skyrim was *not* accidentally correct on `slot_to_role`, and the
  Starfield CDB is *not* a dense role producer (it produces zero).
- `/audit-runtime` discarded its entire first sweep after reproducing the #1619
  port-collision mis-attribution, and re-ran. Structural metrics were
  byte-identical across two independent sweeps.
