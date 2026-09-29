# Audit Suite Summary — comprehensive — 2026-09-29

**HEAD**: `9fcfdc3fc` · 30/30 audits complete (29 owner audits + `runtime --game all`, run last and alone).
**0 CRITICAL · 14 HIGH · 46 MEDIUM · 100 LOW** (160 findings: NEW + regressions; "Existing: #N" re-confirmations are not counted).

Counts come from each agent's final reply, then checked against the `**Severity**` lines in each report. They differ in three reports, and each difference is explained: re-confirmed existing issues also carry severity lines (Physics #4134, Skyrim #4256/#4628, FO3's cross-reference to FNV-D2-01), and ECS-SK-01 has no severity line.

| Audit | C | H | M | L | Report |
|---|---|---|---|---|---|
| Save | 0 | 3 | 2 | 1 | `AUDIT_SAVE_2026-09-29.md` |
| Concurrency | 0 | 2 | 0 | 3 | `AUDIT_CONCURRENCY_2026-09-29.md` (+ #4987 M, #4780 M, #4989 L existing) |
| Runtime | 0 | 1 | 5 | 1 | `AUDIT_RUNTIME_2026-09-29.md` |
| Gameplay | 0 | 1 | 3 | 4 | `AUDIT_GAMEPLAY_2026-09-29.md` |
| ECS | 0 | 1 | 2 | 3 | `AUDIT_ECS_2026-09-29.md` |
| Scripting | 0 | 1 | 1 | 3 | `AUDIT_SCRIPTING_2026-09-29.md` |
| Exterior | 0 | 1 | 1 | 2 | `AUDIT_EXTERIOR_2026-09-29.md` |
| Starfield | 0 | 1 | 1 | 3 | `AUDIT_STARFIELD_2026-09-29.md` |
| FNV | 0 | 1 | 1 | 0 | `AUDIT_FNV_2026-09-29.md` |
| Renderer | 0 | 1 | 0 | 6 | `AUDIT_RENDERER_2026-09-29.md` |
| Oblivion | 0 | 1 | 0 | 3 | `AUDIT_OBLIVION_2026-09-29.md` |
| Tooling | 0 | 0 | 6 | 4 | `AUDIT_TOOLING_2026-09-29.md` |
| Parsers | 0 | 0 | 3 | 6 | `AUDIT_PARSERS_2026-09-29.md` |
| Tech-debt | 0 | 0 | 2 | 20 | `AUDIT_TECH_DEBT_2026-09-29.md` |
| Performance | 0 | 0 | 2 | 5 | `AUDIT_PERFORMANCE_2026-09-29.md` |
| Character | 0 | 0 | 2 | 3 | `AUDIT_CHARACTER_2026-09-29.md` |
| UI | 0 | 0 | 2 | 3 | `AUDIT_UI_2026-09-29.md` (M includes #4757 partial) |
| Skyrim | 0 | 0 | 2 | 1 | `AUDIT_SKYRIM_2026-09-29.md` (+ #4256 M, #4628 L existing) |
| Safety | 0 | 0 | 2 | 1 | `AUDIT_SAFETY_2026-09-29.md` |
| Regression | 0 | 0 | 2 | 1 | `AUDIT_REGRESSION_2026-09-29.md` |
| Physics | 0 | 0 | 1 | 4 | `AUDIT_PHYSICS_2026-09-29.md` (+ #4134 L existing) |
| ESM | 0 | 0 | 1 | 3 | `AUDIT_ESM_2026-09-29.md` |
| Audio | 0 | 0 | 1 | 3 | `AUDIT_AUDIO_2026-09-29.md` (2 L are #4743/#4747 residuals) |
| SpeedTree | 0 | 0 | 1 | 3 | `AUDIT_SPEEDTREE_2026-09-29.md` |
| FO4 | 0 | 0 | 1 | 2 | `AUDIT_FO4_2026-09-29.md` |
| Papyrus | 0 | 0 | 1 | 1 | `AUDIT_PAPYRUS_2026-09-29.md` |
| Legacy-compat | 0 | 0 | 1 | 1 | `AUDIT_LEGACY_COMPAT_2026-09-29.md` |
| NIF | 0 | 0 | 0 | 4 | `AUDIT_NIF_2026-09-29.md` |
| NIFAL | 0 | 0 | 0 | 4 | `AUDIT_NIFAL_2026-09-29.md` |
| FO3 | 0 | 0 | 0 | 2 | `AUDIT_FO3_2026-09-29.md` (HIGH FO3-D2-01 = cross-ref of FNV-D2-01) |

Unchanged-since-baseline dimensions skimmed: ESM Dim 6 · NIFAL Dims 4, 6 · Character Dim 2 · Gameplay Dim 3 · Audio Dim 2 · Scripting Dim 7 · Save Dim 3 · Legacy-compat Dim 2 · Papyrus Dims 1–3 · UI Dims 2–3 · Exterior Dims 3–4 · ECS Dims 1 (machinery), 2, 3, 4, 8, 9. Every other dimension had commits in the delta and was audited.

## HIGH findings

| ID | Title |
|---|---|
| SAVE-D1-01 | `ActorControlState` (script restraint) survives a load. The player can't move |
| SAVE-D5-01 | An exterior load reloads only `radius_load`. Saved rows in the outer `radius_unload` ring are dropped: killed NPCs revive and containers restock (item duplication) |
| SAVE-D5-02 | Cinematic state survives the load teardown. The player stays glued to the Helgen cart, and the cart, horse and riders get duplicate FormIDs |
| GAME-D7-01 | A dead player stays `Dead` after loading a save (additive registration at `save_io.rs:418`) |
| CONC-D2-01 | The ReSTIR reservoir clear is published only to fragment writes. Sync validation reports a RAW hazard at every draw (lavapipe CI) |
| CONC-D2-02 | `VUID-vkCmdDispatch-None-08114`: compute bindings 9/10 are unwritten when there is no global geometry buffer (likely the caustic pass). Not reproduced on the 4070 Ti with a real cell (RT validation capture: 0 errors) |
| ECS-D1-01 | `ab31cfefe` (dialogue) nests locks in `populate_candidates` / `running_quests_binding_entity`. The ABBA lane is red for the 5th time |
| SCR-D3-01 | `GetIsID` compares the placed-reference FormID, not the base FormID, so it is false on the actor it names (19,344 of 19,345 Oblivion params are `NPC_`). Breaks dialogue, AI packages, perks and alias fills |
| EXT-D1-01 | Starfield WTHR fog distances stay in metres, so exteriors fog out at about 43 m |
| SF-D4-01 | Starfield WATR stays in metres: water is opaque within about 30 cm, and underwater visibility is about 1 m |
| FNV-D2-01 | FO3/FNV placed corpses (XRGD, 387 FNV + 498 FO3) spawn alive. #4814 reads only the header bit `0x200`, which these games never set |
| OBL-D2-01 | Oblivion corpses are base Health 0 (CS wiki). All 787 spawn alive |
| REN-D10-01 | The translucency lobe (#4946 fix `efc059f3a`) and the Skyrim/FO4 back-light lobe are self-shadowed to about 0. Needs RenderDoc |
| RT-2 | Oblivion directional lights 2 → 6: the player body re-spawns the `__max_default_light` pair (related to closed #3557) |

## Cross-cutting themes

1. **Starts-dead / starts-unconscious placement is a per-game decode gap.** #4814 only knows FO4/Skyrim's header bit. FNV-D2-01 (XRGD on FO3/FNV) and OBL-D2-01 (Health 0) mean corpses spawn alive. SKY-D4-01 (the XRGD pose is never decoded, 1,445 Skyrim actors plus FO4) means corpses collapse from their spawn pose. FO4-D4-01: the 0x2000 "Starts Unconscious" flag is undecoded (191 dormant robots and turrets start awake). The XRGD = dead rule for FO3/FNV and FO4's dormant semantics are inferred from data and need a source before fixing.
2. **Save/load leaves state behind across the teardown.** GAME-D7-01 (`Dead`), SAVE-D1-01 (`ActorControlState`), SAVE-D5-02 (cinematic), SAVE-D1-02 (alias factions), GAME-D1-02 (worn meshes vs loaded `EquipmentSlots`), SAVE-D5-01 (outer ring). One sweep over the additive-registration components is warranted.
3. **Starfield metre units don't reach every record.** `spatial_units::normalize` skips WTHR (EXT-D1-01) and WATR (SF-D4-01). WTHR fog power/opacity/height are undecoded (SF-D4-02), and LGTM uses Skyrim offsets (SF-D4-03, dormant).
4. **Dialogue landing (`ab31cfefe`, `766e1746e`) produced a cluster.** Lock cycles (ECS-D1-01/02), stale access declarations (ECS-D5-01), a per-frame scan (ECS-D6-01 = PERF-D1-01, Regression #3475), the wrong NPC shown (ECS-D7-01), unfiltered topics (GAME-D2-01), no `player_can_act` gate (GAME-D2-02), DIAL `DATA` byte 0 read as the category (LC-D3-01), `GetIsID` (SCR-D3-01), and QNAM doc drift (ESM-D2-03).
5. **Mid-life gear import (`0182fc5e8`).** One never-worn equip per wearer per batch (GAME-D1-01), never released (REN-D5-02), a third archive set on the main thread (PERF-D7-02), and missing access declarations (CONC-D4-01).
6. **Third-person camera boom (`a070baaad`).** Gameplay rays start behind the head (PHYS-D4-01), footsteps follow the camera and play while swimming (AUD-D5-01), there is no head on Skyrim (SKY-D3-01), and it adds duplicate Oblivion lights (RT-2).
7. **`63c0aee3b` broke the harnesses.** The release debug server is now opt-in (`BYRO_DEBUG_SERVER=1`) and `screenshot` accepts only a bare filename. `capture.sh` (RT-1), `m-exteriors` / `m34` (EXT-D7-01), `m-trees` (SPT-D3-01) and 8 more harnesses plus 3 docs (TOOL-D1-03) have failed ever since. It also introduced byro-dbg 10 s < server 30 s (TOOL-D1-01), a config-writer fallback that overwrites good files on a full disk (TOOL-D4-01), and `esm_opens` reading 1.46 GB on the UI thread (TOOL-D5-01).
8. **CI is red on 5 of 10 jobs.** Clippy on Rust 1.98.1 (SAFE-D4-02, Regression #4595), ABBA (ECS-D1-01), Vulkan validation (CONC-D2-01/02; the lane now reaches a device, so #4987 can close once they're fixed), issue-traceability (`rg` missing, TD9-01, Regression #3504), and the FSR bench self-test (no python3, TD9-02).
9. **Fixes landed under commit titles that don't name the issue.** About 31 issues are fixed but still OPEN (TD4-01, consolidated with evidence in AUDIT_REGRESSION REG close-candidates). The main carriers are `2f8538334` ("audio + combat anim": #4739, #4749, #4750, #4751), `63c0aee3b` (#4753–#4756, #4758–#4760), `b7491072f` ("Refactor code structure": #4620–#4625, already hand-closed today) and `b9e961eeb` (lighting-doc title: SF units, BSGeometry basis, bench camera move). #4864 should stay open (no A/B recorded). #4606 should be reopened (REG-01: only the doc was fixed).
10. **Runtime baselines are stale, not regressed.** `cb44d99f6` reproduces them exactly. Rows moved because of `b9e961eeb` (bench camera → whole draw split, which fully explains R6a-regress-22), `5570c221c` (FNV −92 entities, the likely cause of FNV-D6-01's Prospector drift), `a070baaad` (player body) and `f87490826` (Initially Disabled refs). A deliberate `--regen` after the RT-2 fix is the follow-up.

## Process notes

- **Unplanned engine launch.** The regression agent ran `scripts/check-playable-smoke-contracts.sh`, which the runtime skill describes as data-neutral. It does not mask `BYROREDUX_OBLIVION_DATA`, so it launched one real Oblivion door-gate session at 17:39 that exited on its own (filed as REG-02).
- **Bisect side effect.** During the runtime bisect a shared `target/` regenerated `shader_constants.glsl` with wrong content. The agent restored it (`git checkout --`), cleaned and rebuilt; the recheck `state_hash` matches HEAD.
- **Final state.** The working tree is clean apart from the 30 reports. No `byroredux`/`byro-dbg` process is running and port 9876 is free.
- **Starfield runtime capture** was skipped before launch because of memory pressure (10–12 GB free, 28–29 of 31 GB swap in use).
- **Gates not run:** golden-frame device captures, playable-slice gates, milestone smokes and renderer-correctness gates are recorded as NOT RUN, not SKIP and not pass.
- **Skill drift.** Several reports list skill-drift notes (Starfield META-01, ECS SK-01, NIFAL `texture_clamp_mode`, performance `merge_precombine_materials` thread, TD4-03 `triangle_early.frag`). These are for the next skill sync.

## Publish

Each report has findings:

```
/audit-publish docs/audits/AUDIT_SAVE_2026-09-29.md
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-09-29.md
/audit-publish docs/audits/AUDIT_RUNTIME_2026-09-29.md
/audit-publish docs/audits/AUDIT_GAMEPLAY_2026-09-29.md
/audit-publish docs/audits/AUDIT_ECS_2026-09-29.md
/audit-publish docs/audits/AUDIT_SCRIPTING_2026-09-29.md
/audit-publish docs/audits/AUDIT_EXTERIOR_2026-09-29.md
/audit-publish docs/audits/AUDIT_STARFIELD_2026-09-29.md
/audit-publish docs/audits/AUDIT_FNV_2026-09-29.md
/audit-publish docs/audits/AUDIT_RENDERER_2026-09-29.md
/audit-publish docs/audits/AUDIT_OBLIVION_2026-09-29.md
/audit-publish docs/audits/AUDIT_TOOLING_2026-09-29.md
/audit-publish docs/audits/AUDIT_PARSERS_2026-09-29.md
/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-09-29.md
/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-09-29.md
/audit-publish docs/audits/AUDIT_CHARACTER_2026-09-29.md
/audit-publish docs/audits/AUDIT_UI_2026-09-29.md
/audit-publish docs/audits/AUDIT_SKYRIM_2026-09-29.md
/audit-publish docs/audits/AUDIT_SAFETY_2026-09-29.md
/audit-publish docs/audits/AUDIT_REGRESSION_2026-09-29.md
/audit-publish docs/audits/AUDIT_PHYSICS_2026-09-29.md
/audit-publish docs/audits/AUDIT_ESM_2026-09-29.md
/audit-publish docs/audits/AUDIT_AUDIO_2026-09-29.md
/audit-publish docs/audits/AUDIT_SPEEDTREE_2026-09-29.md
/audit-publish docs/audits/AUDIT_FO4_2026-09-29.md
/audit-publish docs/audits/AUDIT_PAPYRUS_2026-09-29.md
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-09-29.md
/audit-publish docs/audits/AUDIT_NIF_2026-09-29.md
/audit-publish docs/audits/AUDIT_NIFAL_2026-09-29.md
/audit-publish docs/audits/AUDIT_FO3_2026-09-29.md
```

Dedupe while publishing:
- PERF-D1-01 and ECS-D6-01 are one issue.
- Smoke-harness breakage is one root cause across RT-1, EXT-D7-01, SPT-D3-01 and TOOL-D1-03.
- UI-D1-01 should be a comment on #4757, not a new issue.
