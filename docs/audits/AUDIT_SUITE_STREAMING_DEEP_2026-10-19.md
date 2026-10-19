# Audit Suite Summary — `--area streaming-deep` — 2026-10-19

Paths: `byroredux/src/streaming/` `byroredux/src/npc_spawn/` `byroredux/src/cell_loader/`. 19 audits (10 routed owners + 6 per-game + renderer · physics · nif neighbors). Safety omitted: `AUDIT_SAFETY_2026-10-19.md` was produced earlier the same day. Every report is AREA-SCOPED (not a full baseline). No engine, smoke or Vulkan leg was run by any audit; findings are static reads plus data censuses unless stated in the report.

| Audit | Findings | CRITICAL | HIGH | MEDIUM | LOW | Report |
|---|---|---|---|---|---|---|
| exterior | 5 | 0 | 0 | 4 | 1 | AUDIT_EXTERIOR_2026-10-19.md |
| concurrency | 3 | 0 | 0 | 1 | 2 | AUDIT_CONCURRENCY_2026-10-19.md |
| performance | 5 | 0 | 0 | 1 | 4 | AUDIT_PERFORMANCE_2026-10-19.md |
| gameplay | 4 | 0 | 0 | 4 | 0 | AUDIT_GAMEPLAY_2026-10-19.md |
| ecs | 4 | 0 | 0 | 0 | 4 | AUDIT_ECS_2026-10-19.md |
| save | 4 | 0 | 0 | 3 | 1 | AUDIT_SAVE_2026-10-19.md |
| character | 3 | 0 | 0 | 1 | 2 | AUDIT_CHARACTER_2026-10-19.md |
| esm | 3 | 0 | 0 | 2 | 1 | AUDIT_ESM_2026-10-19.md |
| scripting | 3 | 0 | 1 | 2 | 0 | AUDIT_SCRIPTING_2026-10-19.md |
| nifal | 4 | 0 | 0 | 1 | 3 | AUDIT_NIFAL_2026-10-19.md |
| renderer | 6 | 0 | 0 | 3 | 3 | AUDIT_RENDERER_2026-10-19.md |
| physics | 4 | 0 | 1 | 1 | 2 | AUDIT_PHYSICS_2026-10-19.md |
| nif | 6 | 0 | 0 | 1 | 5 | AUDIT_NIF_2026-10-19.md |
| fo4 | 3 | 0 | 0 | 2 | 1 | AUDIT_FO4_2026-10-19.md |
| skyrim | 3 | 0 | 0 | 2 | 1 | AUDIT_SKYRIM_2026-10-19.md |
| fnv | 6 | 0 | 2 | 3 | 1 | AUDIT_FNV_2026-10-19.md |
| oblivion | 5 | 0 | 1 | 3 | 1 | AUDIT_OBLIVION_2026-10-19.md |
| starfield | 5 | 0 | 2 | 2 | 1 | AUDIT_STARFIELD_2026-10-19.md |
| fo3 | 10 | 0 | 2 | 6 | 2 | AUDIT_FO3_2026-10-19.md |

Total: 86 findings as counted per report (0 critical, 9 high, 42 medium, 35 low). Not a distinct-issue count: the per-game audits re-quantify shared defects (FO3 alone lists 7 of its 10 as sibling/existing), and several others extend a sibling finding.

## HIGH findings
- SCR-SD-01 `evaluate_guard_guard_free` takes QuestStageState → QuestObjectiveState in code order, not TypeId order; fails 2 `obscript_effects` tests under `BYRO_LOCK_ORDER_CHECK=1` with core+physics+scripting (4/4 runs).
- PHYS-SD-01 #4818 pickup-tombstone sweep raw-despawns parented collision entities → dangling `Children`, `validate_world` fails, every later save aborts.
- FNV-SD-01 / FO3-SD-01 model-less templated CREA shells spawn nothing (FNV 63%, FO3 60% of creature placements).
- FNV-SD-02 / FO3-SD-02 worldspace persistent CELL spawned once for the entry window (needs a ruling on the real engine's rule).
- OBL-SD-01 Oblivion humanoid idle path is FNV's `locomotion\mtidle.kf`; standing NPCs unanimated.
- SF-SD-01 Starfield small/medium master FormID remap uses the Skyrim/FO4 rule (61,477 of 67,217 own FormIDs collide).
- SF-SD-02 Starfield NPC_ ACBS (16 B) has no parse arm; 44% of placed actors spawn male.

## Cross-audit themes
- Local-vs-global FormID in facegeom paths: FO4-SD-01, SKY-SD-01, SF-SD-04 (one fix).
- Use-Traits template inheritance read from the shell record: CHAR-SD-01, SKY-SD-02, FO3-SD-06.
- Eviction/save state gaps: GAME-SD-01..04, SAVE-SD-01..04.
- XESP parent-enabled heuristic: FNV-SD-03, OBL-SD-02, FO3-SD-07.
- Pool/registry capacity: EXT-D6-01/02, REN-SD-01, NIF-SD-01.

## Next
`/audit-publish docs/audits/AUDIT_<TYPE>_2026-10-19.md` for each report with findings (all 19). Skill-maintenance notes for `/audit-sync`: audit-nifal SKILL.md:90 lists #4256 as open (closed); audit-fo3 lists #4772 as open (closed); audit-ecs SKILL says "five survive" (#5713 closed).
