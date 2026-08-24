# Runtime Telemetry Audit — 2026-08-24

## Scope and execution mode

**Live headless-engine comparison pass: SKIPPED.**

Per this project's hard operating rule ("no parallel engine launch" —
`feedback_no_parallel_engine_launch`), Phase 2–4 of `audit-runtime/SKILL.md`
(drive the engine headless per game, capture `byro-dbg` telemetry, diff
against the committed baselines) were **not executed**. Before any work
began, `pgrep -fa byroredux` and `pgrep -fa byro-dbg` both returned live
processes:

```
byroredux …--esm .../Starfield.esm --sf-smoke CydoniaBase01   (pid 2517047)
byro-dbg                                                       (pid 3392149)
```

`byro-dbg` was re-checked immediately before writing this report (18:34) and
is still alive (pid 3392149), confirming the skip decision held for the
entire audit, not just at dispatch time. No `cargo run -- --bench-hold` or
equivalent engine-launching command was run for any game, and no attempt was
made to route around the collision (e.g. a different `BYRO_DEBUG_PORT`).

The playable-slice smoke gates (`docs/smoke-tests/{p0-door-interaction,
p1-character-traversal,p2-melee-core}.sh`) each launch their own
`cargo run --release -- … --bench-hold` engine internally, so they were
**skipped for the same reason** — running them would violate the same rule
via a different entry point.

What follows is everything the skill's Phase 1 setup and a static read of
the telemetry/baseline/history surface can establish without a live engine.

## Static work performed

1. **Dedup baseline** — `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels` fetched successfully to `/tmp/audit/issues.json` (200 issues).
2. **Build check** — `cargo build --release -p byroredux -p byro-dbg` (Phase 1 step 4) completed clean, no errors/warnings surfaced in the tail. This is a compile-only step; it does not launch a process and is compatible with the safety rule.
3. **Telemetry contract check** — verified the four scalar sources the skill's Phase 3 table depends on still exist at their documented locations, format unchanged:
   - `bench: mode=… entities=… draws={}/{}b/{}c …` — `byroredux/src/app_events.rs:858-928`, format string byte-identical to the skill's citation (`entities=`, `draws=N/Mb/Kc`, `wall_fps`, `frame_p50_ms`/`frame_p95_ms`/`frame_max_ms`).
   - `skin={}/{}+{}` — `byroredux/src/systems/debug.rs:179`, unchanged.
   - `tex.missing`, `mesh.cache failed`, `light.dump` console commands — present and unchanged in `byroredux/src/commands/assets.rs` and `byroredux/src/commands/scene.rs`.
   No drift here — a future live run can trust the skill's Phase 3 parsing recipe as written.
4. **Baseline inventory** — read all five committed TSVs under `.claude/audit-baselines/runtime/` in full, including their header commentary (which carries prior audits' root-cause attributions).
5. **Code-churn analysis** — `git log` against each baseline's own `# regenerated:` date, scoped to the files that feed the gated metrics (`byroredux/src/render/`, `crates/renderer/src/vulkan/scene_buffer/`, `crates/core/src/ecs/resources/skin_slot_pool.rs`, `byroredux/src/asset_provider/`, `byroredux/src/material_translate.rs`, `byroredux/src/cell_loader.rs` + `cell_loader/`, `byroredux/src/npc_spawn.rs`).
6. **Prior-audit cross-check** — read `docs/audits/AUDIT_RUNTIME_2026-08-16.md` (the last live runtime sweep, 8 days before this one) in full and checked the disposition of every finding it raised via `gh issue list --search`.
7. **Smoke-gate fix verification (static)** — since the 08-16 audit found two gates deterministically RED from an assertion-string drift, checked whether the fix that closed those issues actually landed in code (not just in the tracker).

## Baseline inventory and staleness

| Baseline | Cell | Last regenerated | Age (vs 2026-08-24) |
|---|---|---|---|
| `fnv-FreesideAtomicWrangler.tsv` | FNV `FreesideAtomicWrangler` | 2026-08-06 | 18 days |
| `fo3-MegatonPlayerHouse.tsv` | FO3 `MegatonPlayerHouse` | 2026-06-14 | **71 days** |
| `oblivion-ICMarketDistrictTheGildedCarafe.tsv` | Oblivion `ICMarketDistrictTheGildedCarafe` | 2026-08-06 | 18 days |
| `skyrim_se-WhiterunDragonsreach.tsv` | Skyrim SE `WhiterunDragonsreach` | 2026-08-06 | 18 days |
| `fo4-InstituteBioScience.tsv` | FO4 `InstituteBioScience` | 2026-08-22 | 2 days |

No `starfield` cell baseline exists — matches the skill's own documentation
(`starfield` profile ships empty archives, no `sample_cells`; use
`--sf-smoke` instead). `.claude/audit-baselines/sf-esm/` (5 TSVs, all dated
2026-05-28) is the `--sf-smoke` ESM resolve-rate baseline, out of this
skill's scope per its own instructions — not diffed here.

`git log` between 2026-08-06 and 2026-08-24 shows **682 commits** touching
the tree; between 2026-06-14 and 2026-08-24 (the fo3 baseline's actual age)
the count is substantially higher. This volume, by itself, is the reason a
static audit cannot respond with confidence to "is the current baseline
still accurate" — only a live capture can answer that, and this run could
not perform one.

## Cross-check against the last live sweep (2026-08-16)

The most recent actual runtime measurement is 8 days old. Re-reading it in
full and checking every one of its 10 findings against the issue tracker:

| Finding | Status 2026-08-24 | Note |
|---|---|---|
| RT-2026-08-16-01 (p2 gate certifies an ungrounded fixture) | **CLOSED** (#3000) | `23068af0` "fix(smoke): make playable gates truthful" (2026-08-17) rewrote the gate; current `p2-melee-core.sh:232` now asserts `grounded=true` where the old version didn't (statically confirmed). |
| RT-2026-08-16-02 (p0/p1 red from log-string drift) | **CLOSED** (#3001) | Statically confirmed: `p0-door-interaction.sh:114-115` / `p1-character-traversal.sh:251,285` now grep `"input.press: queued action=Activate binding=E"`, which matches the live format string at `byroredux/src/interaction.rs:503` exactly. |
| RT-2026-08-16-03 (spawn-probe/controller column mismatch) | **CLOSED** (#3002) | Same `23068af0` commit rewrote `byroredux/src/scene.rs` spawn logic (535 lines changed). Not independently re-verified live (would require the engine launch this audit skipped). |
| RT-2026-08-16-04 (no CI wiring, silent zero-exit on missing data) | **CLOSED** (#3003) | `.github/workflows/playable-smoke.yml` now exists (`workflow_dispatch`, self-hosted `byroredux-game-data` runner, all three gates); `scripts/check-playable-smoke-contracts.sh` added alongside. Statically confirmed present. |
| RT-2026-08-16-05 (FO3/FNV Health never derived) | **CLOSED** (#3004) | Not independently re-verified (character/CHARAL domain, out of this skill's scope). |
| **RT-2026-08-16-06** (fnv/fo3 draw-batch/gpu_calls regression, `draw_sort_key`) | **STILL OPEN — #3005** | **Not fixed, not re-baselined.** The `fnv` and `fo3` TSVs in `.claude/audit-baselines/runtime/` are unchanged since this finding was filed — `fnv` still carries its 2026-08-06 `bench_draws_batches=89`/`bench_draws_gpu_calls=25` values (the pre-regression numbers the 08-16 sweep measured 164/35 against), and `fo3` is still the 2026-06-14 baseline entirely. This audit cannot confirm whether the regression has grown, shrunk, or been fixed since 08-16 — that requires a live run. |
| RT-2026-08-16-07 (fo4 entity/draw rise) | **CLOSED** (#3006) | Confirmed via the `fo4-InstituteBioScience.tsv` header itself: bisected to `322f33a8`, re-baselined 2026-08-22 with a detailed root-cause note (effect-shader TLAS eligibility fix, verified-correct). |
| RT-2026-08-16-08 (debug-server logs "listening" before bind succeeds) | **CLOSED** (#3007) | Not independently re-verified (would need a live bind-collision reproduction, which this audit could not attempt). |
| RT-2026-08-16-09 (p2 gate doesn't assert fixture identity, pins unarmed fallback) | **CLOSED** (#3008) | Statically confirmed: `p2-melee-core.sh:106-111` now asserts the base NPC (`000E9895`) and both weapon leaves (`0001CB64`, `000236A5`) that the 08-16 finding said were missing. |
| RT-2026-08-16-10 (inventory/settings have no runtime gate) | **STILL OPEN — #3009** | No `inventory.status`/settings console command found in `byroredux/src/commands/`; unchanged. |

**Net:** 8 of 10 findings from the last live sweep are closed, and static
inspection corroborates that the closures for RT-01/02/04/09 (the gate
*mechanics*) reflect real code changes, not just tracker bookkeeping. RT-06
(fnv/fo3 draw-batch regression) and RT-10 (no inventory/settings gate)
remain genuinely open and this audit adds no new information about either —
both need the live pass this run could not perform.

## Code-churn since each baseline, on metric-feeding paths

Restricting `git log` to files that directly produce the gated scalars
(entity spawn, draw batching, texture/mesh resolution, skin pool), **since
the freshest baseline's own capture time** (`fo4`, 2026-08-22 12:36):

| Commit | File(s) touched | Why it could move a gated metric |
|---|---|---|
| `7fbc5baf` Fix #2221 (2026-08-23) | `byroredux/src/render/static_meshes.rs`, `byroredux/src/render/mod.rs` | Grew `GpuMaterial` 348→364 B to carry `AnimatedShaderColor`/`AnimatedShaderFloat`, and merges `AnimatedAlpha`/`AnimatedDiffuse`/`Ambient`/`Specular`/`EmissiveColor` sinks into the material hash before interning. A material-hash change can split or merge draw batches even with identical geometry, which is exactly the `bench_draws_batches`/`bench_draws_gpu_calls` axis the open #3005 regression lives on. |
| `d0322785` Fix #3231 (2026-08-23) | `byroredux/src/render/mod.rs`, `cell_loader/spawn/mesh_instance.rs`, `unload.rs` | New per-entity `MorphSlot` GPU resource wired into spawn/unload/draw. Per the commit message this is deformation-only (no draw-count change intended), but it is a new resource-lifecycle path through the same spawn/unload sites `skin_pool_live`/`skin_pool_max` already gate. |
| `900aa081` Fix #973 (2026-08-23) | `byroredux/src/cell_loader/spawn.rs`, `cell_loader/spawn/mesh_instance.rs` | Per-shape MSWP material-swap resolution (previously only one shape per REFR got its swap applied). Any FNV/FO3/FO4 cell with MSWP-equipped NPCs (armour with per-piece material swaps) now resolves different material/texture paths per shape than it did when any of the five baselines were captured — this is the single most plausible unmeasured mover of `tex_missing_unique_paths` on the committed cells, none of which have been re-run since. |
| `4e1afcbe`, `eb2e2445`, `06f86742`, `bfdc3d3f` (2026-08-23/24) | `byroredux/src/streaming.rs`, `byroredux/src/asset_provider/texture.rs`, `cell_loader/spawn.rs`, `npc_spawn.rs` | Refactors touching the streaming/texture-resolution/spawn call paths that back `entities_total` and `tex_missing_unique_paths`. Read as refactor-labelled, not behavior-labelled, but none carry their own runtime-telemetry re-verification. |

None of this is a measured regression — it is exactly the set of code paths
that, if this audit could have run Phase 2–4, would have been the first
place to look for drift. Recorded here as the static substitute for that
verification, per the assignment's explicit request to check "code changes
since baselines were captured."

## Findings

### RT-2026-08-24-01: fnv/oblivion/skyrim_se baselines are 18 days and fo3's is 71 days stale against 682+ intervening commits, several touching draw-batching/material/spawn code directly
- **Severity**: LOW
- **Dimension**: Runtime baseline currency (audit-infrastructure gap, not a code defect)
- **Location**: `.claude/audit-baselines/runtime/*.tsv`
- **Status**: NEW
- **Description**: This is a coverage-freshness observation, not a demonstrated regression — no live measurement was taken this run (see Scope above). The fo3 baseline in particular predates the entire `#2371`/`#2372` exterior-streaming tranche, the `#973`/`#2221`/`#3231` render/material commits above, and roughly two and a half months of history. Combined with the still-open #3005 (fnv/fo3 draw-batch regression, not re-baselined since 08-16) this leaves the fo3 arm in the worst state of the five: an already-known-regressed baseline that is also the stalest one, with no live re-check since 2026-08-16.
- **Impact**: The next audit able to launch the engine should treat `fo3` as the highest-priority arm to re-run, and should not be surprised if `fnv`/`skyrim_se`/`oblivion` also show drift on `tex_missing`/`draws=` given the material/spawn commits identified above.
- **Related**: Existing #3005 (OPEN — fnv/fo3 draw-batch regression), Existing #2521 (OPEN — fo3 entities_total marginal drift).
- **Suggested Fix**: No code fix — schedule a live `/audit-runtime --game all` the next time no engine/`byro-dbg` instance is running, prioritizing `fo3` first.

### RT-2026-08-24-02: this run could not verify whether #3005 (fnv/fo3 draw-batch regression) has grown, shrunk, or resolved
- **Severity**: N/A (informational — not a new code finding, restating an existing OPEN issue's unresolved status)
- **Dimension**: Baseline diff — render load
- **Location**: `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv`, `.claude/audit-baselines/runtime/fo3-MegatonPlayerHouse.tsv`
- **Status**: Existing: #3005 (OPEN, filed 2026-08-16, unchanged)
- **Description**: See Cross-check table above. `bench_draws_batches`/`bench_draws_gpu_calls` moved past their ×1.1 gate on both `fnv` (89→164, 25→35) and `fo3` (96→114, 9→12) as of 2026-08-16, root-caused to a `draw_sort_key` batching-policy interaction, and the baselines were deliberately left un-regenerated pending attribution. Nothing in this audit's static pass can confirm or deny whether that regression is still present at HEAD — `byroredux/src/render/mod.rs` (the file `draw_sort_key` lives in) has one commit since (`d0322785`, MorphSlot wiring, unrelated to batching) so the mechanism is plausibly unchanged, but "plausibly unchanged" is not a measurement.
- **Suggested Fix**: Unchanged from #3005 — bisect the merge predicate over the 2026-08-06→08-16 window; live-verify at HEAD once an engine slot is free.

## Clean dimensions (statically verified)

- **Telemetry wire format** — `bench:`/`skin=`/`tex.missing`/`mesh.cache failed`/`light.dump` all match the skill's documented contract exactly; a future live run can trust `audit-runtime/SKILL.md` Phase 3 as written.
- **Release build** — `cargo build --release -p byroredux -p byro-dbg` succeeds cleanly at HEAD.
- **Playable-slice gate mechanics (RT-01/02/09)** — the assertion-string drift and fixture-identity gaps the 2026-08-16 audit found are fixed in code (`23068af0`), statically confirmed by diffing the current gate scripts against the current `interaction.rs`/`p2-melee-core.sh` content. Not re-run live.
- **CI wiring (RT-04)** — `.github/workflows/playable-smoke.yml` exists and dispatches all three gates on a game-data-bearing self-hosted runner. Not itself exercised by this audit.
- **fo4 baseline** — freshly regenerated 2026-08-22 with a detailed, verified root-cause header (`#3006`); the most trustworthy of the five right now.

## Reproduction (deferred — do not run while an engine/byro-dbg instance may be live)

```bash
pgrep -fa byroredux || pgrep -fa byro-dbg   # must both be empty first
cargo build --release -p byroredux -p byro-dbg
# then Phase 2 of audit-runtime/SKILL.md, serial, one game at a time,
# prioritizing fo3 (staleset + only OPEN regression) and fnv (OPEN
# regression, needs re-verification against d0322785).
```

---

Report ready. This report was **not** created via `gh issue create` (per
instructions). Suggest:

```
/audit-publish docs/audits/AUDIT_RUNTIME_2026-08-24.md
```
