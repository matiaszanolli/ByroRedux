# Audit Suite Summary — `--preset comprehensive` — 2026-10-08

**HEAD**: 00f580e09 · 29 of 30 audits completed. `runtime --game all` was stopped mid-run (no report; see below). Every audit ran solo (no sub-agents), with a shared rules file, a shared open-issue snapshot, and cross-audit dedup context passed forward as findings landed. A mechanical reconciliation matched 22 audits' scratch-file finding IDs against their reports with 0 dropped. The other 7 use non-ID scratch formats; since they ran solo, there was no relay hazard.

**No CRITICAL findings.**

| Audit | Findings | CRITICAL | HIGH | MEDIUM | LOW | Report |
|---|---:|---:|---:|---:|---:|---|
| renderer | 4 | 0 | 1 | 1 | 2 | [AUDIT_RENDERER_2026-10-08.md](AUDIT_RENDERER_2026-10-08.md) |
| performance | 6 | 0 | 0 | 1 | 5 | [AUDIT_PERFORMANCE_2026-10-08.md](AUDIT_PERFORMANCE_2026-10-08.md) |
| concurrency | 6 | 0 | 2 | 0 | 4 | [AUDIT_CONCURRENCY_2026-10-08.md](AUDIT_CONCURRENCY_2026-10-08.md) |
| safety | 2 | 0 | 0 | 0 | 2 | [AUDIT_SAFETY_2026-10-08.md](AUDIT_SAFETY_2026-10-08.md) |
| ecs | 4 | 0 | 1 | 1 | 2 | [AUDIT_ECS_2026-10-08.md](AUDIT_ECS_2026-10-08.md) |
| nif | 5 | 0 | 0 | 0 | 5 | [AUDIT_NIF_2026-10-08.md](AUDIT_NIF_2026-10-08.md) |
| nifal | 3 | 0 | 0 | 1 | 2 | [AUDIT_NIFAL_2026-10-08.md](AUDIT_NIFAL_2026-10-08.md) |
| esm | 5 | 0 | 0 | 2 | 3 | [AUDIT_ESM_2026-10-08.md](AUDIT_ESM_2026-10-08.md) |
| parsers | 5 | 0 | 1 | 2 | 2 | [AUDIT_PARSERS_2026-10-08.md](AUDIT_PARSERS_2026-10-08.md) |
| physics | 2 | 0 | 0 | 0 | 2 | [AUDIT_PHYSICS_2026-10-08.md](AUDIT_PHYSICS_2026-10-08.md) |
| character | 2 | 0 | 0 | 0 | 2 | [AUDIT_CHARACTER_2026-10-08.md](AUDIT_CHARACTER_2026-10-08.md) |
| gameplay | 12 | 0 | 1 | 5 | 6 | [AUDIT_GAMEPLAY_2026-10-08.md](AUDIT_GAMEPLAY_2026-10-08.md) |
| exterior | 6 | 0 | 1 | 2 | 3 | [AUDIT_EXTERIOR_2026-10-08.md](AUDIT_EXTERIOR_2026-10-08.md) |
| ui | 4 | 0 | 0 | 1 | 3 | [AUDIT_UI_2026-10-08.md](AUDIT_UI_2026-10-08.md) |
| audio | 5 | 0 | 0 | 2 | 3 | [AUDIT_AUDIO_2026-10-08.md](AUDIT_AUDIO_2026-10-08.md) |
| speedtree | 0 | 0 | 0 | 0 | 0 | [AUDIT_SPEEDTREE_2026-10-08.md](AUDIT_SPEEDTREE_2026-10-08.md) |
| scripting | 6 | 0 | 1 | 4 | 1 | [AUDIT_SCRIPTING_2026-10-08.md](AUDIT_SCRIPTING_2026-10-08.md) |
| papyrus | 4 | 0 | 0 | 1 | 3 | [AUDIT_PAPYRUS_2026-10-08.md](AUDIT_PAPYRUS_2026-10-08.md) |
| save | 5 | 0 | 1 | 1 | 3 | [AUDIT_SAVE_2026-10-08.md](AUDIT_SAVE_2026-10-08.md) |
| tooling | 4 | 0 | 0 | 0 | 4 | [AUDIT_TOOLING_2026-10-08.md](AUDIT_TOOLING_2026-10-08.md) |
| legacy-compat | 2 | 0 | 0 | 2 | 0 | [AUDIT_LEGACY_COMPAT_2026-10-08.md](AUDIT_LEGACY_COMPAT_2026-10-08.md) |
| tech-debt | 9 | 0 | 0 | 0 | 9 | [AUDIT_TECH_DEBT_2026-10-08.md](AUDIT_TECH_DEBT_2026-10-08.md) |
| fnv | 5 | 0 | 1 | 1 | 3 | [AUDIT_FNV_2026-10-08.md](AUDIT_FNV_2026-10-08.md) |
| fo3 | 2 | 0 | 0 | 0 | 2 | [AUDIT_FO3_2026-10-08.md](AUDIT_FO3_2026-10-08.md) |
| skyrim | 4 | 0 | 0 | 1 | 3 | [AUDIT_SKYRIM_2026-10-08.md](AUDIT_SKYRIM_2026-10-08.md) |
| oblivion | 1 | 0 | 0 | 1 | 0 | [AUDIT_OBLIVION_2026-10-08.md](AUDIT_OBLIVION_2026-10-08.md) |
| fo4 | 2 | 0 | 0 | 1 | 1 | [AUDIT_FO4_2026-10-08.md](AUDIT_FO4_2026-10-08.md) |
| starfield | 3 | 0 | 1 | 0 | 2 | [AUDIT_STARFIELD_2026-10-08.md](AUDIT_STARFIELD_2026-10-08.md) |
| regression | 1 | 0 | 0 | 0 | 1 | [AUDIT_REGRESSION_2026-10-08.md](AUDIT_REGRESSION_2026-10-08.md) |
| runtime | — | — | — | — | — | not written: stopped mid-bisect |

Total: 119 raw findings (0 critical, 11 high, 30 medium, 78 low). After cross-audit dedup, 114 distinct (0 / 11 / 29 / 74).

**Unchanged-since-baseline dimensions skimmed:**
- ecs: Dims 2, 3, 9
- nif: Dims 1 and 5
- nifal: Dims 5 and 6
- physics: Dims 1, 3, 5 and 6
- character: Dims 2 and 3
- performance: Dims 6 and 8
- safety: Dim 8
- renderer: the Dim 3 struct layer, plus Dims 7, 8, 9 and 12
- exterior: Dims 2, 3 and 7
- ui: Dims 2, 3, 5 and 7
- audio: Dims 2 and 3
- speedtree: Dims 1, 2 and 5
- papyrus: Dims 1 and 2
- scripting: Dims 6 and 7
- tooling: Dim 6
- legacy-compat: Dims 1 and 2
- fo3: Dim 4
- skyrim: Dim 1
- gameplay: Dim 3

## The 11 HIGH findings

1. **REN-D5-2026-10-08-01** — `parse_dds` ignores 32-bpp DDPF_RGB channel masks, so 9,326 Skyrim SE distant-terrain atlases upload with R and B swapped.
2. **CONC-D5-2026-10-08-01** — `forcegreet_system` / `eat_sleep_system` hold `PhysicsWorld` across other storages (lock-order inversion).
3. **CONC-D3-2026-10-08-01** — `raise_hello_story_event` reads `LoadedCellIndex` under the `StoryEvent` guard, closing the cycle #5066 predicted.
4. **ECS-2026-10-08-D6-01** — the force-greet and Eat/Sleep movers write the walk step into `GlobalTransform`, so propagation discards it.
5. **PAR-D2-2026-10-08-01** — the menuxml DDS decoder underflows a `u32` on zero or narrow channel masks (panic in dev, wrong colour in release).
6. **GAME-D5-2026-10-08-01** — 60 FNV and 58–60 FO3 NPCs get fail-open ambient Dialogue packages, so they force-greet the player perpetually.
7. **EXT-D5-2026-10-08-01** — Oblivion's 30 child worldspaces get no default water.
8. **SCR-D5-2026-10-08-01** — Story Manager nodes pass on uncatalogued condition functions that evaluate to 0.0.
9. **SAVE-D5-2026-10-08-01** — the #3817 re-adoption cell-owns the player, so the next unload or load despawns it.
10. **FNV-2026-10-08-D2-01** — DIAL overrides replace the master's whole INFO list (FNV/FO3 DLCs, Skyrim + Update.esm, FO4 DLC).
11. **SF-2026-10-08-D6-01** — Starfield CDB slot-6 height maps arm parallax occlusion that nothing authors.

All 11 were published and fixed in the #5371–#5381 batch (2026-10-08).

## Cross-audit dedup and links

- REN-D5-2026-10-08-03 = SAFE-D4-2026-10-08-01 (stale `pipeline.rs` SAFETY comment).
- AUD-2026-10-08-D4-01 ⊂ CONC-D4-2026-10-08-01 (the same `npc_dialogue_selection` Access row).
- LC-D3-02 = GAME-D2-02 (the voice plugin-slot inversion).
- TD3-02 ≈ FNV D5-03 (Eat/Sleep/Dialogue doc rot).
- FO4-2026-10-08-D2-01 ⊂ SF-2026-10-08-D3-02 (the misplaced `single_boundary_tests` doc).
- Linked rather than merged:
  - SAVE-D5-01 and ECS D7-01 (the same function, `retry_cinematic_readoption`).
  - PHYS-D4-01, ECS-D6-01 and CONC-D5-01 (the same two movers).
  - FNV D5-01 and LC-D3-01 (DLC voice naming).
  - NIFAL-D8-01 and PAR-D3-02 (the same loose `.mat` decoder).
- ESM-2026-10-08-D4-01 (Story Manager `SNAM` = Previous Node) was corroborated independently by legacy-compat: CommonLibSSE-NG `previousSibling // 30 - SNAM`, plus UESP. Scripting found it undecidable from the runtime side but leaning the same way.

## Runtime audit (stopped)

Dims 1–4 completed before the stop:
- Harness self-test and schema tests pass.
- All five telemetry diffs PASS. The FO4/Skyrim entity and skin moves are attributed to `8c925ec54` (#5095 head parts). The FNV/FO3 `gpu_calls` moves are attributed to `9a179472f` (#2764).
- The baseline-integrity checks pass.

A `combustion_lab_golden_frame` bisect then found the golden diverging at **`186234944`** (2026-09-26, "Implement ReSTIR light identity…"), 97.45% of pixels against 1.66% at its parent. That commit is where the Cornell penumbra grain first appears; the evidence is on #5369. The golden itself was never re-blessed after that commit.

## Harness notes for the next suite run

- Never run `find /`. Two audits left whole-filesystem scans running, one for 17 minutes. The rule was added to the run's shared rules file mid-suite.
- The runtime audit left seven `Xvfb` servers orphaned after it was stopped; they were killed by hand.
- `_audit-validate.sh` cannot see a stale path that is a later token inside one backtick span (TD4-01). Three skills' first-step pathspecs named files deleted by the streaming and physics splits.

Publish: done (`/audit-publish`, 124 issues titled `*-2026-10-08-*`).
