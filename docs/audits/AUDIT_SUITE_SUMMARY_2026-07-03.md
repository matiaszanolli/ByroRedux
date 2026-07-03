# Audit Suite Summary — comprehensive — 2026-07-03

Full `--preset comprehensive` sweep: every subsystem audit, every per-game audit,
the regression pass, and the runtime telemetry diff. 21 audits, run against HEAD
`8498e559`.

## ✅ No CRITICAL findings.

Only **2 HIGH** findings in the whole sweep, and they're of very different
character: one is a genuinely new, previously-untracked data-loss bug in the
save subsystem; the other is a re-confirmed reproduction of an already-open
performance regression. Everything else is either zero findings, or LOW/MEDIUM
items that are pre-existing and already tracked as open issues.

## Results

| Audit | Findings | CRIT | HIGH | MED | LOW | Report |
|-------|---------:|-----:|-----:|----:|----:|--------|
| renderer | 2 | 0 | 0 | 0 | 2 | AUDIT_RENDERER_2026-07-03.md |
| ecs | 0 | 0 | 0 | 0 | 0 | AUDIT_ECS_2026-07-03.md |
| safety | 1 | 0 | 0 | 1 | 0 | AUDIT_SAFETY_2026-07-03.md |
| concurrency | 8 | 0 | 0 | 1 | 7 | AUDIT_CONCURRENCY_2026-07-03.md |
| performance | 16 | 0 | 0 | 4 | 12 | AUDIT_PERFORMANCE_2026-07-03.md |
| nif | 0 | 0 | 0 | 0 | 0 | AUDIT_NIF_2026-07-03.md |
| nifal | 0 | 0 | 0 | 0 | 0 | AUDIT_NIFAL_2026-07-03.md |
| audio | 0 | 0 | 0 | 0 | 0 | AUDIT_AUDIO_2026-07-03.md |
| speedtree | 0 | 0 | 0 | 0 | 0 | AUDIT_SPEEDTREE_2026-07-03.md |
| scripting | 3 | 0 | 0 | 2 | 1 | AUDIT_SCRIPTING_2026-07-03.md |
| **save** ⚠️ | 1 | 0 | 1 | 0 | 0 | AUDIT_SAVE_2026-07-03.md |
| legacy-compat | 1 | 0 | 0 | 1 | 0 | AUDIT_LEGACY_COMPAT_2026-07-03.md |
| tech-debt | 1 | 0 | 0 | 0 | 1 | AUDIT_TECH_DEBT_2026-07-03.md |
| fnv | 0 | 0 | 0 | 0 | 0 | AUDIT_FNV_2026-07-03.md |
| fo3 | 0 | 0 | 0 | 0 | 0 | AUDIT_FO3_2026-07-03.md |
| skyrim | 0 | 0 | 0 | 0 | 0 | AUDIT_SKYRIM_2026-07-03.md |
| oblivion | 1 | 0 | 0 | 0 | 1 | AUDIT_OBLIVION_2026-07-03.md |
| fo4 | 0 | 0 | 0 | 0 | 0 | AUDIT_FO4_2026-07-03.md |
| starfield | 0 | 0 | 0 | 0 | 0 | AUDIT_STARFIELD_2026-07-03.md |
| regression | 0 | 0 | 0 | 0 | 0 | AUDIT_REGRESSION_2026-07-03.md |
| **runtime** ⚠️ | 4 | 0 | 1 | 1 | 2 | AUDIT_RUNTIME_2026-07-03.md |
| **Total** | **38** | **0** | **2** | **10** | **26** | |

⚠️ = audit containing a HIGH finding this pass.

## HIGH findings (read these first)

1. **save — SAVE-07 (NEW, HIGH)**. `QuestStageState`/`QuestObjectiveState`
   (`crates/scripting/src/quest_stages.rs`) — the live Papyrus
   `SetStage`/`GetStageDone`/objective runtime — carry no `serde` derive and
   are absent from `build_save_registry` (`byroredux/src/save_io.rs:157-187`).
   Every quest-stage/objective change is silently lost on a save→load cycle.
   This is a distinct root cause from the already-open `#1834`/`#1835`
   (`ActorValues`/`PerkList`/`FactionRanks` in `crates/core`, missing
   Component registry entries) — SAVE-07 is a `Resource` pair missing both the
   derive *and* the registration. Highest-priority fix in this sweep: quest
   progress is core gameplay state.
2. **runtime — HIGH**. Confirmed live reproduction of already-**OPEN #1698**
   (Skyrim Dragonsreach scheduler stall, 321→32.7 fps warm-up collapse). Not
   new — tracked, still unresolved.

## Everything else

- **performance (16, 0 HIGH)** — the Session 53→54 fix sprint already closed
  the prior HIGH (D6-01 skinned bind-inverse corruption) plus 8 other
  findings; remaining 4 MEDIUM + 12 LOW are explicitly deferred
  measurement/repro items, not newly discovered regressions.
- **concurrency (8, 0 HIGH)** — prior CRITICAL BLAS-scratch-buffer
  use-after-free (`#1782`) confirmed fixed with no regressions; re-confirmed
  MEDIUM `#1783` (skin_palette/skin_compute init-failure coupling) plus 7 LOW
  (mostly pre-existing doc-rot).
- **scripting (3, 0 HIGH)** — two genuinely new MEDIUM findings: SCR-D7-NEW-01
  (`QuestStageAdvanced` markers collide on a shared sink entity — dormant but
  live-reachable) and SCR-D6-NEW-03 (`Globals` resource silently rebuilt on
  every interior load, dormant until a `SetGlobalValue` writer lands).
- **legacy-compat (1 MEDIUM)** — the VWD "Has Distant LOD" flag now parses
  (closed `#1731`) but its runtime full-model-cull consumer was deferred and
  has no open issue tracking it yet — worth filing.
- **safety / renderer / tech-debt / oblivion** — all single-digit LOW/MEDIUM
  findings, all either pre-existing tracked issues or trivial doc-hygiene
  notes; no code defects.
- **ecs / nif / nifal / audio / speedtree / fnv / fo3 / skyrim / fo4 /
  starfield / regression** — zero findings. Recent fix commits across the
  board (BSGeometry sentinel-slot `#1828`/`#1829`, BGSM blend-factor swap
  `#1823`, foliage PBR `#1819`, ragdoll bone-drop `#1718`, VWD flag `#1731`,
  SkinSlotPool rollback `#1791`/`#1796`) were independently re-verified
  correct across every consuming audit, with no cross-cutting regressions.
- Full workspace test suite: **3371 tests green** (per `/audit-regression`).

## Suggested follow-ups

```
/audit-publish docs/audits/AUDIT_SAVE_2026-07-03.md
/audit-publish docs/audits/AUDIT_SCRIPTING_2026-07-03.md
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-07-03.md
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-07-03.md
/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-07-03.md
/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-07-03.md
/audit-publish docs/audits/AUDIT_RENDERER_2026-07-03.md
/audit-publish docs/audits/AUDIT_SAFETY_2026-07-03.md
/audit-publish docs/audits/AUDIT_OBLIVION_2026-07-03.md
/audit-publish docs/audits/AUDIT_RUNTIME_2026-07-03.md
```

(`/audit-publish` de-dupes against existing open issues automatically, so
re-running it on reports whose findings are mostly "Existing: #NNN" is safe
— only SAVE-07 and the two scripting MEDIUMs and the legacy-compat MEDIUM are
likely to actually open new issues.)
