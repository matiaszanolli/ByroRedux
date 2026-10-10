# Audit Suite Summary — `--preset streaming-deep` — 2026-10-09

**HEAD**: 3bcf6c8e8 · **Baseline**: every audit's `AUDIT_<TYPE>_2026-10-08.md` (comprehensive suite, HEAD `00f580e09`, 81 commits earlier; 12 of them in the area).

**Area**: `byroredux/src/streaming/`, `byroredux/src/npc_spawn/`, `byroredux/src/cell_loader/`. Routed through `_audit-route.sh`:

- **Direct owners:** performance · concurrency · exterior · gameplay · save · nifal · esm · per-game.
- **Neighbors added:** renderer · physics · ecs · nif.
- **Per-game audits:** fnv · fo3 · fo4 · oblivion · skyrim · starfield.

**How the audits ran:** 17 audits, all completed. Each one:

- ran solo (no sub-agents), at most 3 at a time;
- followed the shared rules file `/tmp/audit/SUITE_RULES.md`;
- used a shared open-issue snapshot (147 open);
- received a forward-dedup list of findings already reported by earlier audits. Two running audits got mid-run dedup updates.

No engine launches.

**Reconciliation:** all 53 reported finding IDs are present in their reports. No scratch `dim_N.md` holds an unreported HIGH or CRITICAL candidate.

**No CRITICAL findings. 7 HIGH.**

| Audit | Findings | CRITICAL | HIGH | MEDIUM | LOW | Report |
|---|---:|---:|---:|---:|---:|---|
| performance | 3 | 0 | 0 | 0 | 3 | [AUDIT_PERFORMANCE_2026-10-09.md](AUDIT_PERFORMANCE_2026-10-09.md) |
| concurrency | 3 | 0 | 0 | 2 | 1 | [AUDIT_CONCURRENCY_2026-10-09.md](AUDIT_CONCURRENCY_2026-10-09.md) |
| exterior | 5 | 0 | 0 | 0 | 5 | [AUDIT_EXTERIOR_2026-10-09.md](AUDIT_EXTERIOR_2026-10-09.md) |
| gameplay | 3 | 0 | 0 | 3 | 0 | [AUDIT_GAMEPLAY_2026-10-09.md](AUDIT_GAMEPLAY_2026-10-09.md) |
| save | 2 | 0 | 0 | 2 | 0 | [AUDIT_SAVE_2026-10-09.md](AUDIT_SAVE_2026-10-09.md) |
| esm | 5 | 0 | 0 | 2 | 3 | [AUDIT_ESM_2026-10-09.md](AUDIT_ESM_2026-10-09.md) |
| nifal | 3 | 0 | 0 | 2 | 1 | [AUDIT_NIFAL_2026-10-09.md](AUDIT_NIFAL_2026-10-09.md) |
| renderer | 3 | 0 | 0 | 2 | 1 | [AUDIT_RENDERER_2026-10-09.md](AUDIT_RENDERER_2026-10-09.md) |
| physics | 3 | 0 | 1 | 0 | 2 | [AUDIT_PHYSICS_2026-10-09.md](AUDIT_PHYSICS_2026-10-09.md) |
| ecs | 2 | 0 | 1 | 0 | 1 | [AUDIT_ECS_2026-10-09.md](AUDIT_ECS_2026-10-09.md) |
| nif | 3 | 0 | 2 | 0 | 1 | [AUDIT_NIF_2026-10-09.md](AUDIT_NIF_2026-10-09.md) |
| fnv | 1 | 0 | 0 | 1 | 0 | [AUDIT_FNV_2026-10-09.md](AUDIT_FNV_2026-10-09.md) |
| fo3 | 2 | 0 | 0 | 1 | 1 | [AUDIT_FO3_2026-10-09.md](AUDIT_FO3_2026-10-09.md) |
| fo4 | 5 | 0 | 1 | 2 | 2 | [AUDIT_FO4_2026-10-09.md](AUDIT_FO4_2026-10-09.md) |
| oblivion | 4 | 0 | 1 | 2 | 1 | [AUDIT_OBLIVION_2026-10-09.md](AUDIT_OBLIVION_2026-10-09.md) |
| skyrim | 2 | 0 | 0 | 2 | 0 | [AUDIT_SKYRIM_2026-10-09.md](AUDIT_SKYRIM_2026-10-09.md) |
| starfield | 4 | 0 | 1 | 1 | 2 | [AUDIT_STARFIELD_2026-10-09.md](AUDIT_STARFIELD_2026-10-09.md) |

Total: 53 raw findings (0 critical, 7 high, 22 medium, 24 low). There are 52 distinct root causes: CONC-D5-01 and SAVE-D4-01 share one (see Cross-audit merges).

**Dimensions skimmed as unchanged since the baseline:**

| Audit | Dimensions |
|---|---|
| performance | 6, 8 |
| concurrency | 1, 2, 6 |
| exterior | 2, 3, 4, 7 |
| gameplay | 1, 3, 4, 6 |
| save | 3 |
| esm | 1, 6 |
| nifal | 4, 5, 6, 7, 9 |
| renderer | 1, 4, 8, 9, 12 |
| physics | 1, 3 |
| ecs | 2, 6, 9 (plus the Dim 1 and Dim 3 machinery) |
| nif | 1, 2, 3, 5 |
| fnv | 4 |
| fo3 | 4 |
| fo4 | 2, 3, 5 |
| oblivion | 1, 3 |
| skyrim | 2, 5 |
| starfield | 1, 5 |

## HIGH

| ID | Location | What breaks |
|---|---|---|
| ECS-2026-10-09-D7-01 | `byroredux/src/cell_loader/exterior.rs:1170-1178` | The persistent-CELL apply job starts its stamp range at job creation. On an interactive `--grid` boot (any launch without `--bench-frames`), the camera, player capsule, player body and arrival cell get `CellRoot(persistent_root)`, so the first worldspace teardown (door, save load, `dbgload`) **despawns the player and camera**. Interior→exterior arrivals also get a wrong `GetInCell` and ghost gear. No smoke gate catches it, because every script passes `--bench-frames`. |
| PHYS-D2-2026-10-09-01 | `crates/physics/src/world/mod.rs:942-998` | A ragdoll explosion is integrated *and* broad-phased inside one rapier `step` (16 internal substeps), so no #5161/#5246 guard runs before rapier panics at `sap_axis.rs:61`. This is the live Skyrim SE crash seen this session, and it is **distinct from #5352**: the restore claimed nothing before the panic. |
| NIF-D4-2026-10-09-01 | `crates/nif/src/import/walk/lights.rs:18` (+5 walkers) | The scene-graph walkers have no visited set. A self-child NiNode causes an uncatchable stack overflow on the streaming pre-parse worker; a doubly-listed child walks 2^128 paths and runs out of memory or hangs. Reproduced with a 4-byte patch to a real FNV NIF. Regression of #1269. Vanilla content is always a strict tree (128k NIFs checked), so the fix is a no-op for vanilla. |
| NIF-D6-2026-10-09-01 | `crates/nif/src/anim/bspline.rs:283-318` | B-spline resampling is capped per channel but has no total cap: about 118 MiB per controlled block that shares one interpolator. A ~4 KB NIF can exhaust memory, and the abort cannot be caught. |
| FO4-2026-10-09-D1-01 | `crates/plugin/src/esm/cell/helpers.rs:119-180` + `cell_loader/references/mod.rs:536-553` | XCRI and XPRI are swapped at the precombine de-dup gate. 959,238 baked XCRI refs in Fallout4.esm are drawn twice, and 9,886 unbaked XPRI refs are deleted. Position-proven: Switchboard 2,343/2,344 vs 0/33. Settles #2699. Recapture the FO4 runtime baselines in the fix commit. |
| OBL-2026-10-09-D3-01 | `byroredux/src/npc_spawn.rs:640-673` | Oblivion bodies are built from FO3/FNV paths. `lefthand`/`righthand.nif` don't exist, and `lowerbody`/`hand`/`foot.nif` are never requested, so every non-gauntleted humanoid spawns handless and legs and feet go missing wherever nothing covers them. The RACE body-section skins are dropped (seam tone falls back to the Imperial skin). Two tests pin the nonexistent paths. |
| SF-2026-10-09-D4-01 | `byroredux/src/cell_loader/refr.rs:586-600` | Starfield PKIN REFRs spawn nothing: CNAM is a template CELL, and instancing it is unimplemented. That is 97,981 REFRs and 1.21 M template children; Cydonia loses 12,218 placements. `sf_smoke` counts these as resolved (SF-D4-02), which hid the gap behind a 91.2% figure. |

## Cross-audit merges and severity notes

- **CONC-D5-2026-10-09-01 + SAVE-D4-2026-10-09-01: one root cause, publish as one issue.** The `cell_loader/unload.rs:97-113` session-replace purge bare-`despawn_batch`es parentless convoy roots (since #5384). It has two consequences:
  - ghost subtrees with no mesh, texture or BLAS release, plus a phantom Rapier collider;
  - orphaned `Parent(<dead root>)` children make `validate_world` refuse every later save for the life of the process.

  ECS confirms that `despawn_batch` being flat and the Hierarchy rule are both correct, so the fix belongs in the caller. Regression of #5379.
- **The #5376 fix has three independent holes:**
  - GAME-D5-01: the declined Dialogue package stays the active winner. 56 FNV NPCs are left idle, and FO3 has 55–57 NPC_ plus 9 CREA starved (Amata, Butch, Dogmeat, Fawkes …).
  - ESM-D2-02: the PKDD Dialogue Type is read from the FOV byte, so the Say-To gate is inert.
  - SKY-D3-02: `force_greet` ignores PKCU templates, so 334/339 Skyrim ForceGreets read as `NotAForceGreet`.

  Publish them as separate issues that link each other.
- **ESM-2026-10-09-D4-01: the FO3 audit recommends LOW → MEDIUM, `game:fo3`.** The #5375 fold reversal makes The Pitt's catch-all "Go away." GREETING shadow 92 conditioned greetings, and Anchorage authors 0 PNAMs.
- **Worldspace-inheritance cluster:**
  - FO3-D3-01: the LOD rings ignore PNAM "Use LOD Data". This also hits Skyrim's city worldspaces.
  - OBL-D4-01: 30 child worldspaces draw no parent LAND or terrain LOD.
  - EXT-D1-01: inherit-all is implemented twice and only the dead copy is tested.

  These are distinct defects; publishing them together lets one fix wave cover them.
- **Starfield material cluster:**
  - NIFAL-D8-01: the CDB lookup ignores `Parent`, so #5277/#5283 fire on 0/10,158 referenced materials.
  - NIFAL-D8-02: unsourced emittance scale.
  - NIFAL-D8-03: the loose `.mat` decoder drops emissive.
  - SF-D6-01: Additive and soft-additive shapes render as opaque cards. This one is live today, independent of D8-01.
- **Seating:** FNV-D5-01 (creatures take the humanoid seat path; 59 FNV + 15 FO3 placements) and GAME-D5-03 (stream snapshot rebuilds `Seated` as a bare marker) are distinct.
- **Voice:**
  - FO3-D5-01 (LOW): the truncation rule is wrong, but the miss falls through.
  - OBL-D5-01 (LOW): Oblivion voice never resolves.
- **REN-D11-2026-10-09-01 is a residual of today's #5482 fix (3bcf6c8e8).** The shadow toe still crosses zero for authored contrast > 1 + TOE/PIVOT ≈ 1.556. 31 vanilla IMGS qualify: FNV UltraLuxe 1.9, Skyrim FrostmereCrypt 2.0 and storm weathers 1.65–1.7, FO3 Metro 1.6. Reproduced: min −0.0008 at 1.9, −0.00125 at 2.0. The tests pinned only 1.3.

## Existing-issue status changes reported by the audits

- #2699: settled by FO4-D1-01.
- #5452: now live; ECS-D6-01, which masked it, was fixed in f0c683ef1.
- #5353: corroborated by per-substep DOF-clamp floods in this session's Skyrim log.
- #5291: grew; #5277 added an owned-String-per-object `effect_blend` map.
- #5466: half fixed by faf09c6e6 (FO4). The Starfield sites remain.
- #5405: still reproduces.
- **Verified fixed:** #5355, #5356, #5357, #5272, #5378, #5381, #5385, #5386, #5420, #5421, #5422, #5373, #5384, #5418, #5419, #5371, #5372, #5414–#5417, #5375, #5393, #5395, #5426, #5427, #5397.

## Out-of-scope notes from the audits

- CI "Test + Check + Clippy" has been red since f1141aebe, on clippy only:
  - `crates/spt/src/parser.rs:87-93` (`doc_lazy_continuation`);
  - `crates/sfmaterial/src/index.rs:156`.

  It does not hide a lock lane (compare #5474).
- `AudioWorld::stop_sounds_for` (#5410) has no production caller (#5432 still open).
- 3bcf6c8e8 changes the grade for every game, so golden and reference captures from before 2026-10-09 are not A/B-comparable with new ones.

## Next steps

Publish each report with findings:

```
/audit-publish docs/audits/AUDIT_ECS_2026-10-09.md
/audit-publish docs/audits/AUDIT_PHYSICS_2026-10-09.md
/audit-publish docs/audits/AUDIT_NIF_2026-10-09.md
/audit-publish docs/audits/AUDIT_FO4_2026-10-09.md
/audit-publish docs/audits/AUDIT_OBLIVION_2026-10-09.md
/audit-publish docs/audits/AUDIT_STARFIELD_2026-10-09.md
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-10-09.md
/audit-publish docs/audits/AUDIT_SAVE_2026-10-09.md
/audit-publish docs/audits/AUDIT_GAMEPLAY_2026-10-09.md
/audit-publish docs/audits/AUDIT_ESM_2026-10-09.md
/audit-publish docs/audits/AUDIT_NIFAL_2026-10-09.md
/audit-publish docs/audits/AUDIT_RENDERER_2026-10-09.md
/audit-publish docs/audits/AUDIT_SKYRIM_2026-10-09.md
/audit-publish docs/audits/AUDIT_FO3_2026-10-09.md
/audit-publish docs/audits/AUDIT_FNV_2026-10-09.md
/audit-publish docs/audits/AUDIT_EXTERIOR_2026-10-09.md
/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-10-09.md
```

Publish CONC-D5-01 and SAVE-D4-01 as a single issue.
