# Audit Suite Summary — --preset comprehensive — 2026-10-31

HEAD `670f81917` · Baseline: the 2026-10-27 reports (HEAD `fa6be4a6b`, 136 commits earlier) · 30 of 30 audits completed (29 owner audits, then `runtime --game all` alone).

**No CRITICAL findings.** The 10-27 CRITICAL (`spawn_lod_block` `holes[i]` OOB) is fixed (`terrain_lod.rs:1125`) but still has no regression guard (EXT-D6-2026-10-31-01).

**HIGH findings to look at first**
- ECS-2026-10-31-01 — `improve_plan` (`crafting.rs:222-236`) takes `ItemInstancePool` under the `Inventory` guard, the reverse of the engine order. The CI ABBA lane is red. CONC-01 and REG-01 are duplicates of it.
- OBL-D3-2026-10-31-01 — Oblivion cobwebs and FX sheets still synthesize solid colliders (`spawn.rs:94-103`).
- RT-2026-10-31-01 — Skyrim exterior → interior door transition (Whiterun → Bannered Mare, 2nd visit) loses the Vulkan device (NVRM Xid 109). Reproduced in 3 of 4 display runs of `p1-character-traversal`. Not bisected.

| Audit | Findings | CRITICAL | HIGH | MEDIUM | LOW | Report |
|---|---|---|---|---|---|---|
| safety | 0 | 0 | 0 | 0 | 0 | [AUDIT_SAFETY_2026-10-31.md](AUDIT_SAFETY_2026-10-31.md) |
| renderer | 1 | 0 | 0 | 0 | 1 | [AUDIT_RENDERER_2026-10-31.md](AUDIT_RENDERER_2026-10-31.md) |
| regression | 4 | 0 | 1 (dup of ECS-01) | 3 | 0 | [AUDIT_REGRESSION_2026-10-31.md](AUDIT_REGRESSION_2026-10-31.md) |
| ecs | 2 | 0 | 1 | 0 | 1 | [AUDIT_ECS_2026-10-31.md](AUDIT_ECS_2026-10-31.md) |
| concurrency | 0 new (2 dups of ECS) | 0 | 0 | 0 | 0 | [AUDIT_CONCURRENCY_2026-10-31.md](AUDIT_CONCURRENCY_2026-10-31.md) |
| performance | 2 | 0 | 0 | 1 | 1 | [AUDIT_PERFORMANCE_2026-10-31.md](AUDIT_PERFORMANCE_2026-10-31.md) |
| nif | 0 | 0 | 0 | 0 | 0 | [AUDIT_NIF_2026-10-31.md](AUDIT_NIF_2026-10-31.md) |
| nifal | 0 | 0 | 0 | 0 | 0 | [AUDIT_NIFAL_2026-10-31.md](AUDIT_NIFAL_2026-10-31.md) |
| esm | 5 | 0 | 0 | 0 | 5 | [AUDIT_ESM_2026-10-31.md](AUDIT_ESM_2026-10-31.md) |
| parsers | 1 | 0 | 0 | 1 | 0 | [AUDIT_PARSERS_2026-10-31.md](AUDIT_PARSERS_2026-10-31.md) |
| physics | 0 | 0 | 0 | 0 | 0 | [AUDIT_PHYSICS_2026-10-31.md](AUDIT_PHYSICS_2026-10-31.md) |
| character | 3 | 0 | 0 | 1 | 2 | [AUDIT_CHARACTER_2026-10-31.md](AUDIT_CHARACTER_2026-10-31.md) |
| gameplay | 6 | 0 | 0 | 4 | 2 | [AUDIT_GAMEPLAY_2026-10-31.md](AUDIT_GAMEPLAY_2026-10-31.md) |
| exterior | 4 | 0 | 0 | 0 | 4 | [AUDIT_EXTERIOR_2026-10-31.md](AUDIT_EXTERIOR_2026-10-31.md) |
| ui | 0 | 0 | 0 | 0 | 0 | [AUDIT_UI_2026-10-31.md](AUDIT_UI_2026-10-31.md) |
| audio | 0 | 0 | 0 | 0 | 0 | [AUDIT_AUDIO_2026-10-31.md](AUDIT_AUDIO_2026-10-31.md) |
| speedtree | 0 | 0 | 0 | 0 | 0 | [AUDIT_SPEEDTREE_2026-10-31.md](AUDIT_SPEEDTREE_2026-10-31.md) |
| scripting | 3 | 0 | 0 | 0 | 3 | [AUDIT_SCRIPTING_2026-10-31.md](AUDIT_SCRIPTING_2026-10-31.md) |
| papyrus | 0 | 0 | 0 | 0 | 0 | [AUDIT_PAPYRUS_2026-10-31.md](AUDIT_PAPYRUS_2026-10-31.md) |
| save | 2 | 0 | 0 | 1 | 1 | [AUDIT_SAVE_2026-10-31.md](AUDIT_SAVE_2026-10-31.md) |
| tooling | 2 | 0 | 0 | 0 | 2 | [AUDIT_TOOLING_2026-10-31.md](AUDIT_TOOLING_2026-10-31.md) |
| legacy-compat | 0 | 0 | 0 | 0 | 0 | [AUDIT_LEGACY_COMPAT_2026-10-31.md](AUDIT_LEGACY_COMPAT_2026-10-31.md) |
| tech-debt | 9 | 0 | 0 | 1 | 8 | [AUDIT_TECH_DEBT_2026-10-31.md](AUDIT_TECH_DEBT_2026-10-31.md) |
| fnv | 5 | 0 | 0 | 1 | 4 | [AUDIT_FNV_2026-10-31.md](AUDIT_FNV_2026-10-31.md) |
| fo3 | 3 | 0 | 0 | 1 | 2 | [AUDIT_FO3_2026-10-31.md](AUDIT_FO3_2026-10-31.md) |
| skyrim | 1 | 0 | 0 | 1 | 0 | [AUDIT_SKYRIM_2026-10-31.md](AUDIT_SKYRIM_2026-10-31.md) |
| oblivion | 2 | 0 | 1 | 1 | 0 | [AUDIT_OBLIVION_2026-10-31.md](AUDIT_OBLIVION_2026-10-31.md) |
| fo4 | 2 | 0 | 0 | 1 | 1 | [AUDIT_FO4_2026-10-31.md](AUDIT_FO4_2026-10-31.md) |
| starfield | 0 | 0 | 0 | 0 | 0 | [AUDIT_STARFIELD_2026-10-31.md](AUDIT_STARFIELD_2026-10-31.md) |
| runtime | 11 | 0 | 1 | 6 | 4 | [AUDIT_RUNTIME_2026-10-31.md](AUDIT_RUNTIME_2026-10-31.md) |

Total: 68 findings (0 critical, 4 high, 23 medium, 41 low); 67 distinct, since REG-01 repeats ECS-01.

Unchanged-since-baseline dimensions skimmed (per report header; exact lists in each report):
safety 1, 2, 8 · renderer 1-6, 9-11 · regression n/a (delta pass) · ecs 2, 3, 5 (scheduler core), 6 · concurrency 1, 2, 6, 7 · performance 2, 3, 4, 6 · nif 1, 2, 3, 5 · nifal 2, 4, 5, 9 · esm 1, 6 · parsers 2, 3, 6 (+ bgsm, sfmaterial, facegen, menuxml, game-detect) · physics 4, 6, solver half of 2 · character 2 · gameplay 3, 6 · exterior none wholesale · ui 1-5, 7 · audio 1-3 · speedtree 1, 2, 5 · scripting 7, Dim 3 core, Dim 6 VM core · papyrus 1-4 · save 3, `commands.rs` half of 4 · tooling 4, 5, 6 + debug-server · legacy-compat 1 (paths), 3 · tech-debt 4, 5, 6, 9 · fnv 3, 4 · fo3 4 · skyrim 1, 2 · oblivion 1, 4 · fo4 2, 3, 5 · starfield 1, 3, 5, 6 · runtime none.

## Runtime gate state (from AUDIT_RUNTIME)

- All 5 telemetry baselines PASS (entities within ±2 %). Starfield was not run (no baseline).
- Red gates that are not telemetry: HUD smokes m48-4/5-fnv/6 (RT-02), m47-3-obscript (RT-03), m47-2-bleedout (RT-04), m-exteriors cycle on fnv/fo3/skyrim (RT-05), p6-loading-model skyrim (RT-06), m34 (RT-09), interior-godrays (#6506).
- Known and not re-filed: #6506, #6493, #6500, REG-2026-10-31-04.
- Not run: Cornell oracle, check-bench-determinism, material-provider-matrix, fsr-bench-matrix, the 30-minute p5-soak, and the 4 Starfield real-data pins.

## Publish

For each report with findings: `/audit-publish docs/audits/AUDIT_<TYPE>_2026-10-31.md` for RENDERER, REGRESSION, ECS, PERFORMANCE, ESM, PARSERS, CHARACTER, GAMEPLAY, EXTERIOR, SCRIPTING, SAVE, TOOLING, TECH_DEBT, FNV, FO3, SKYRIM, OBLIVION, FO4, RUNTIME. CONCURRENCY duplicates ECS and needs no separate publish.
