---
description: "Audit accumulated technical debt — stale markers, dead code, duplication, magic numbers, stub impls, doc rot, oversized files"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Tech-Debt Audit

Audit ByroRedux for accumulated technical debt: code that compiles, passes tests and ships, but raises the
cost of every future change. Correctness bugs belong to other audits; this one hunts decay since the last
cleanup pass.

**Every dimension below is a DISCOVERY RECIPE, not a finding list.** Instances churn between audits, so each
dimension hands you a command that enumerates *current* instances plus a triage rule. There is no hardcoded
instance list here on purpose — re-run the recipe and report what it surfaces today. Where a `cargo test` /
CI gate already enforces an invariant, the dimension names the guard and aims at what it cannot see.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for layout, crate roster, methodology, dedup, severity and finding
format. Young code has had the fewest sweeps — find it with
`git log --diff-filter=A --since=<last-report-date> --name-only --format= -- 'crates/*/Cargo.toml' 'tools/*/Cargo.toml'`
plus the crates the last report never names (`for c in crates/*/; do grep -q "$c" <last-report> || echo "$c"; done`;
the 2026-10-25 report still left 18 crate dirs and all four non-vendored `tools/` unnamed — among them `bgsm`, `boot-request`,
`debug-protocol`, `debug-ui`, `facegen`, `mod-runtime`, `papyrus`, `terrain-height` and the two placeholders; `video`, `bink2` and
`binkaudio` landed 2026-11-11..13, 6e64c453b / 8e11d49a4 / 3ebecb7ec, and no report has swept them). File real findings there; do not
just note the crate is young. `crates/cxx-bridge` and `crates/platform` are deliberate placeholders owned here:
check they have not grown a second job or silent consumers, not that they are small.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: comma-separated dimension numbers (e.g. `1,3,5`). Default: all 9.
- `--depth shallow|deep`: `shallow` = surface counts + worst offenders; `deep` = per-instance triage with a concrete fix proposal. Default `deep`.

## Extra Per-Finding Fields

- **Dimension**: one of the 9 below.
- **Age** (when relevant): commit hash + date the debt landed (`git log -L` / `git blame`).
- **Effort**: trivial (≤30 min) | small (≤2 h) | medium (≤1 day) | large (>1 day, decompose first).
- **ID convention**: `TD<dim>-<date>-NN` (e.g. `TD3-2026-09-05-02` = Dim 3 Stale Documentation, finding 2 of that report).

## Severity for Tech Debt

Tech-debt findings default to **LOW** (see `_audit-severity.md`). Promote only on amplification:

| Promotion Trigger | Floor |
|-------------------|-------|
| Duplicated logic with divergent bug-fix history (one branch fixed, the other regressed) | MEDIUM |
| `unimplemented!()` / `todo!()` / `panic!("not …")` reachable from a shipped CLI flag or smoke test | MEDIUM |
| `#[ignore]`d test that guards a fix from a closed CRITICAL/HIGH issue | MEDIUM |
| Stale doc/audit baseline that misled an audit in the last 90 days | MEDIUM |
| Magic number that would silently over/underflow under documented use | HIGH |
| Stale `GpuCamera`/`GpuInstance`/`GpuMaterial` size in a doc comment (lockstep-drift bait) | MEDIUM |

## Phase 1: Setup

1. Parse `$ARGUMENTS`. 2. `mkdir -p /tmp/audit/tech-debt`.
3. Dedup baseline:
   `gh issue list --repo matiaszanolli/ByroRedux --limit 500 --state all --label tech-debt --json number,title,state > /tmp/audit/tech-debt/issues_all.json`
4. Scan `docs/audits/` for prior `AUDIT_TECH_DEBT_*.md` (diff direction, not re-litigation).
5. **Production-LOC helper** (Dim 1's subject is production complexity, not file length — a file long from bulk
   inline tests is not the debt this dimension hunts). Source the checked-in script and run its self-test first;
   a failure means no figure below it can be trusted:
   ```bash
   source .claude/commands/audit-tech-debt/prod_loc.sh
   prod_loc_self_test        # must print "prod_loc self-test: ok"
   ```
   *prod_loc* `<file>` reports 0 for pure-test files (`tests.rs`, `*_tests.rs`, anything under `tests/` — their
   `#[cfg(test)] mod` gate lives in the parent) and otherwise total LOC minus every `#[cfg(test)]`-gated braced
   item, counting braces on code only (comments, strings, raw strings and char literals stripped, #4336).
6. Snapshot totals so the next audit can diff. `tools/` is in scope (four workspace binaries); `tools/nifskope` is
   vendored and pruned:
   ```bash
   {
     echo "markers (TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE): $(grep -RInE '(TODO|FIXME|HACK|XXX|TBD|WIP|KLUDGE)\b' crates byroredux tools --exclude-dir=nifskope | wc -l)"
     echo "allow(dead_code):      $(grep -RInE 'allow\(dead_code\)' crates byroredux tools --exclude-dir=nifskope | wc -l)"
     echo "unimplemented!/todo!(): $(grep -RInE 'unimplemented!|todo!\(\)' crates byroredux tools --exclude-dir=nifskope | wc -l)"
     echo "#[ignore] tests:        $(grep -RInE '^[[:space:]]*#\[ignore' --include='*.rs' crates byroredux tools --exclude-dir=nifskope | wc -l)"
     echo "files >2000 production LOC: $(for f in $(find crates byroredux tools -name '*.rs' -not -path 'tools/nifskope/*'); do echo "$(prod_loc "$f")"; done | awk '$1>2000' | wc -l)"
     echo "test files >2000 total LOC (lower priority, separate bucket): $(find crates byroredux tools -name '*.rs' -not -path 'tools/nifskope/*' -exec wc -l {} + | awk '$1>2000 && $2!="total"' | wc -l)"
   } > /tmp/audit/tech-debt/baseline.txt
   ```
   Measured 2026-11-22 at bf977bfb3 (diff direction only, re-run, never quote; the figures at 91e91aeac earlier the same day are in parentheses, `allow(dead_code)`/`unimplemented!` unchanged): markers 41 (41) (19 (19) are `XXXX` false positives — one a comment in `crates/plugin/src/esm/cell/helpers.rs` — three
   are the `--type XXXX` usage placeholder in `crates/plugin/examples/dump_wths_refl.rs` — 18 (18) the FO4 `HACK` terminal
   FourCC (M43's `byroredux/src/lockpick.rs`, `byroredux/src/interaction.rs`, `byroredux/src/commands/story_actions.rs`, two comment mentions under `byroredux/src/boot/schedule/`, and the `crates/plugin/tests/parse_real_esm.rs` pair), 3 quoted upstream FIXMEs, 1 documented `TBD` — Dim 5;
   **zero** real TODOs: the two in `byroredux/src/loading_screen.rs` (tracked by #6194 since #6181, 51f97f708) were rewritten as
   a stated "what the P6 cover does not model" doc by 729fbab1f, closing #6194 — a fresh real TODO is notable),
   `allow(dead_code)` 45 (45; 46 on 11-19; 38 on 11-06, 41 on 2026-10-25, #6397/#6398 retired two; 12 of them are `cfg(test)` TypeId-only markers in `crates/core/src/ecs/lock_tracker.rs`, c5113f0d9 —
   not debt; the four `#[cfg_attr(not(test), allow(dead_code))]` items in `byroredux/src/cell_loader/{lod_bands,lod_coverage,tree_lod}.rs` that production no longer calls became plain `#[cfg(test)]` by #6987 (33b5e554c, closed; a fresh `cfg_attr(not(test), allow(dead_code))` is the same finding again), the three `allow(dead_code)` in `crates/menuxml/tests/common/mod.rs` carry a reason (each corpus file is its own test crate; not debt), one is a `block` field in `crates/ui/examples/hud_feed_paths.rs`, and the three added 2026-11-15 are `cfg(test)` type-ascription guards in `crates/renderer/src/vulkan/context/mod.rs`, `crates/renderer/src/vulkan/shadow_filter.rs` and `byroredux/src/render/skin_offsets_hasher_tests.rs` (#6687, #6686) — not debt), `unimplemented!/todo!()` **0** (a fresh hit is notable), `#[ignore]` 479 (475; 465 on 11-19, 436 on 2026-11-15 — the rise is real-data gates in `hud.rs`, `avm2_host.rs`, the menuxml corpus and `parse_real_esm.rs`; 435 on 2026-11-14, 429 on 2026-11-09, 405 on 2026-11-06, 382 on 2026-10-31, 373 on 2026-10-27, 368 on 2026-10-24, 363 on 2026-10-21, 352 on 2026-10-20, 322 on 2026-10-15; tools-inclusive —
   earlier reports scoped to `crates`+`byroredux` read lower; none runs on a hosted runner),
   production >2000 LOC: **2** (2, the same pair) — `crates/renderer/src/shader_constants_data.rs` (3595, 3589 at 91e91aeac, 3520 on 11-20 and 3510 on 11-19 — the #7152 terrain-patch constants; a data table with the generator split out, #5808 — 1579 of its lines are code, the rest comments; Dim 1, not a split candidate) and `byroredux/src/app_events.rs` (2014, 2014 on 11-20, 2003 on 11-19, 1994 on 11-15 and 1979 on 11-09 — it crossed the line after 11-15 (touched by bb986991e, 153f611a7 and ac558feb7) and kept growing despite dc4754847 (#7065) moving the paused-frame arm into `App::run_paused_frame` to hold `about_to_wait`'s function pin, now 204; a live finding, the watch-list's "most urgent row" realised: read its structure and propose an axis, not a name), where *crates/plugin/src/esm/records/misc/world.rs* (2052 on 11-15, 2054 on 11-14) was split by #6947 (11a20095b, closed) into `crates/plugin/src/esm/records/misc/world/` by record family (largest child `region.rs` 694 prod LOC, then `navigation.rs` 649); it had crossed from 1914 on the #6725 region / fog / god-ray decoders and #6781..#6799 fixes, ea5417dc7..8774d1adb — a real finding: a single file, so propose an axis from its record families, not its name). The other 2026-10-31 entry,
   *crates/debug-ui/src/panels.rs* (2086, crossed from 1938 on the M43 dialogue-page / crafting / drop-and-cast panels, 041d57333..c2742a901), was split by #6578 (37c845ee6, closed) into `panels/{snapshots,studio}.rs` plus the HUD and game-menu panels (largest child `studio.rs`, 883 prod LOC; the HUD and game-menu panels have since moved to `crates/native-ui/src/{hud,game_menu}.rs`, GUI-PAGE-006, f12ac1b82). The
   other 2026-10-25 entry, `crates/scripting/examples/mq101_conformance/`, was split by #6360 (01f48c929, largest child ~530). Three left the list through real splits since 2026-10-25:
   `crates/audio/src/` (#6140, b2e542eee — `lib.rs` is a ~200-LOC hub over `dsp`/`world`/`components`/`system`/`cache`, largest child `world.rs` ~740),
   `crates/nif/src/import/types/` (#6141, 5559074be — eight files, largest ~575) and `crates/scripting/src/obscript_effects/`
   (2034 at d5d13edcc → 1842 after #6365/#6373, then split into per-domain handler modules by #6355, aa29774ec, largest child ~550). The five scripting/save/cell monoliths of 2026-10-15 left the list through real splits —
   `crates/scripting/src/fragment/vm.rs`, `crates/scripting/src/translate/effects.rs`, `crates/scripting/src/condition.rs`,
   `byroredux/src/save_io.rs` and `crates/plugin/src/esm/cell/mod.rs` are now ~70–150-LOC hubs over `vm/`, `effects/`,
   `condition/`, `save_io/` and `cell/` directories, the largest child ~1400 — as did `byroredux/src/components.rs` (#5467) and
   `crates/scripting/src/fragment/effects.rs` (#5468) before them. **Split-hub hazard:** a path-scoped `git log` / `grep` on a hub
   file silently sees only the hub (`git log -- byroredux/src/save_io.rs` returned 0 commits where the directory returned 24);
   scope Dim 1 deltas to the `<stem>/` directory and measure a split by its children, not by the parent shrinking. Watch-list
   within 5% (1900+): `byroredux/src/app_step.rs` (1981 unchanged, 1923 on 11-20 — now the closest to the line; 92d72f117, #6106's geometry-pool retry), `byroredux/src/app_frame.rs` (1952, 1933 on 11-20, 1835 on 11-15 — the fastest grower; 84f04205f, #7152 terrain patches), `byroredux/src/asset_provider/material/merge.rs` (1941, 1904 — the #7140 BGSM ledger), `byroredux/src/interaction.rs` (1929, 1923 on 11-20, 1862 on 11-19 — the player-spellcasting / door-lock work, e.g. 4288659fa, 180080de7), `crates/renderer/src/texture_registry/mod.rs` (1926, 1896 — the #7144/#7167/#7169 archive-generation work); just under the band:
   `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (1902, 1891 at 91e91aeac), `crates/renderer/src/vulkan/volumetrics.rs` (1883), `crates/core/src/ecs/resources/mod.rs` (1880), `crates/renderer/src/texture_registry/upload.rs` (1876), `byroredux/src/systems/character.rs` (1879), `crates/nif/src/import/material/mod.rs` (1859), `crates/physics/src/world/mod.rs` (1831), `byroredux/src/combat.rs` (1845; #7236's doc-pin test),  `crates/hkx/src/behavior.rs` (1823), `crates/renderer/src/vulkan/composite.rs` (1822). `crates/renderer/src/mesh.rs` (1864) left the list when #6520 (cd78f9efe) split the pool-limit policy and the primitive generators out (1412). `byroredux/src/inventory/` (#6356, 258c6d9b2, largest child `catalog.rs` ~850)
   and `byroredux/src/npc_spawn/` (#6357, 613c7f46e, largest child `ai_package.rs` ~1540) were split on 2026-10-28; measure them by children.
   `crates/renderer/src/vulkan/gpu_timers.rs` (1929 on 2026-10-25) fell when #6358 (0288f468d) made the per-pass brackets
   `TimerPass` table rows (1196 prod LOC on 2026-11-09).
   `crates/renderer/src/vulkan/context/init.rs` (1950 on 2026-10-25, 1131 on 2026-11-09) left the band — 3e6f6b66a (#6143) split the frame and
   Vulkan drivers into `init_stages.rs` and the per-stage `context/` files, each pinned by `function_size_pin_tests.rs` (a31697a6f moved the `PostPassState` literal into `PostPassState::new`, `build_pipelines_and_finish` pin 395 -> 380), and #6143 closed once #6708/#6709/#6710 (4f6d738d7, 1a0302c8e, d66db4358) moved `record_skinned_blas_refit`, `build_blas_batched` and `record_geometry_pass` onto phase methods inside their pins;
   the instance list is regenerated every audit from `dim1_big.txt` — trust the recipe and the count delta over the names;
   test-heavy >2000: 98 (97; 95 on 11-15).
   A raw whole-repo grep for `#[ignore]` also matches markdown prose — keep `--include='*.rs'`.

## Phase 2: Dimension Agents

Ordered by debt impact: complexity and duplication compound across every edit; doc/audit rot misdirects the *next*
audit; markers and dead code are cheap. Recent yield (`grep -h '\*\*Dimension\*\*' docs/audits/AUDIT_TECH_DEBT_*.md`):
Dims 2, 1, 3 dominate (then 8, 4; 7, 9) — counted over all 43 reports on 2026-11-06; 5 and 6 rarely surface real debt. Each agent writes `/tmp/audit/tech-debt/dim_<N>.md`.

### Dimension 1: File / Function / Module Complexity
Paths: all `.rs` under `crates/`, `byroredux/`, `tools/`
First step: the two-bucket command below

An oversized file taxes every edit, review and merge. Threshold is **2000 production LOC**. Two buckets:
```bash
# Primary — the dimension's actual subject. File real findings from this.
for f in $(find crates byroredux tools -name '*.rs' -not -path 'tools/nifskope/*'); do
    p=$(prod_loc "$f"); [ "$p" -gt 2000 ] && echo -e "$p\t$f"
done | sort -rn
# Secondary — test-heavy files, report but do not auto-file: escalate one only if its OWN prod_loc also crosses 2000.
find crates byroredux tools -name '*.rs' -not -path 'tools/nifskope/*' -exec wc -l {} + | awk '$1>2000 && $2!="total"' | sort -rn
```
The recipe output is authoritative. A file that once split can re-cross (a previously-split file that grew back is a
live finding); a file that dips under by incidental shrinkage is still a candidate — only an actual split removes one;
and files within ~5% of the line (check the snapshot) are worth naming as watch-list rows. Function-level size budgets (`function_line_count`, `crates/core/src/source_scan.rs`) count CODE lines only — blank and
comment-only lines are free since #6720 (d7320311b) — so a documented function never breaks its pin and a pin figure
quoted from before that commit is not comparable; the pins sit in `scheduler_access_tests.rs`,
`crates/renderer/src/vulkan/context/function_size_pin_tests.rs`, `crates/nif/src/anim/function_size_pin_tests.rs` and
`crates/physics/src/function_size_pin_tests.rs`. Function-level splits inside
a file and file-level splits are independent signals — a `register_*_systems` / phase-helper extraction does not move
its host file across the line.

**Per file, propose a split AXIS by responsibility, chosen after reading the file's internal structure** — never from a
name or a tag. Precedent: a metadata enum tag (`ExtenderFamily`) touching 30 of 3759 lines was *not* the seam; the real
axis was the four-layer service stack (routes → declarations → source aliases → runtime adapters) with one service
holding over half the lines (#3851). Splits that worked: Vulkan `context/` and `volumetrics` by
construct-vs-record-vs-teardown; *boot.rs* into one file per registration stage; *runtime.rs* into one file per
`impl <wit>::Host` block. Data tables (FourCC → id maps, `shader_constants_data.rs`) want a static table or
generation, not a split. Vulkan-recording splits are render-pass-adjacent — read
*feedback_speculative_vulkan_fixes* before proposing barrier/order changes.

**Also flag**: functions >200 LOC (propose extraction); match arms >50 cases (want a lookup table); nesting depth >5;
a `mod.rs` / `lib.rs` with >20 `pub use` (two jobs). `cargo +nightly clippy --all-targets -- -W clippy::cognitive_complexity`
if available, else inspect the worst offenders.

### Dimension 2: Logic Duplication
Paths: sibling-file families under `crates/nif/src/blocks/`, `crates/plugin/src/esm/records/`, `crates/renderer/src/vulkan/`, `byroredux/src/cell_loader/`, `byroredux/src/systems/`
First step: `ls crates/nif/src/blocks/*.rs crates/plugin/src/esm/records/*.rs crates/plugin/src/esm/records/*/*.rs crates/renderer/src/vulkan/*.rs byroredux/src/cell_loader/*.rs`, then read two siblings side by side

CLAUDE.md global policy: *improve existing code, never duplicate logic.* Every finding names a concrete
consolidation site (an existing helper to extend, or the new one and its callers).
**Look for**: block-parser scaffolding (header → field → fixup) repeated across `crates/nif/src/blocks/`; texture-upload
chains (BC1/BC3/BC5/RGBA) and image-layout barrier sequences repeated per pass in `crates/renderer/src/vulkan/`
(`vulkan/image.rs` `GpuImage` and the shared barrier helpers are the consolidation targets — a pass that still
hand-rolls create/bind/destroy is a candidate); `vk::WriteDescriptorSet` / `DescriptorSetLayoutBinding` builder boilerplate (one home since #5593, c6d36b4e7: `descriptors::{layout_binding,compute_binding,write_*}`, `vulkan/mod.rs`'s shared `try_or_cleanup!` and `GpuBuffer::create_per_frame_uniforms`, pinned by `passes_build_layouts_writes_and_teardown_through_the_shared_helpers` — a pass that re-spells the five-line binding chain or a local cleanup macro is a regression; the bindless texture layout is the one deliberate exception; since #5809, 31f4919d5, the same guard requires `descriptors::clamp_sampler` for clamp-to-edge samplers, with the genuinely different ones allowlisted); other single homes landed by the 2026-10-15 wave — the `upload_ctx!` macro for the `GpuUploadCtx` literal (#5823), `with_extension_host` for the extension dispatch tail (#5810), `World::init_resource` for insert-if-absent (#5806; #6136, 10360816e pinned its contract and put `insert_resource`'s doc back), `byroredux_core::bytes` for little-endian reads in the bin (#5818, `no_production_code_reads_le_ints_by_hand`) and the `asset_provider` layer for archive access (#5009, `no_production_code_opens_archives_outside_the_provider_layer`); since 2026-10-22 also `byroredux_core::paths` for exe-relative asset and cache resolution (#6274, b42f802fa — presets, profiles, the Vulkan pipeline cache, height maps and the launcher each used to re-derive it), `menuxml`'s DDS/BC4 codec for the terrain-height red-channel decode (#6272, ce5796431) and `texture::submit_initial_layouts` for the per-pass one-time UNDEFINED → GENERAL barrier submit (#6146, d87246b57; a pass that re-spells it is a regression); and the 2026-10-27 wave — `descriptors::{depth_read_only_image_info,image_info}` for the remaining hand-built `DescriptorImageInfo` literals (#6369, 6c0bebb84), `PlayerEntity::get` for hand-spelled player reads (#6362, 67722b6e4), `Inventory::count_of` for held-count sums (#6363, 5b43fe347), `apply_walk_step` for the cacheless navmesh walk movers (#6364, 98b5d060e), `vertex_bound` shared by terrain LOD and BTR (#6368, a1327ad3a), `SubReader` for VMAD / weather-settings / NVNM decode (#6367, e04caa7e8), `ItemKind::weapon_stub` for test fixtures (#6372, a65e11ef9), `live_caller` / `open_actor_effects` for ObScript effect handlers (#6365, bf27df84c) and `World::init_resource` for insert-default-if-absent (#6366, f6c7c534e — now policed by `no_source_spells_insert_default_if_absent_by_hand`; a configured-value insert such as `TerrainHeightService` is legitimately different); ESM sub-record parse loops
across `crates/plugin/src/esm/records/`; AI-procedure systems sharing scaffolding; near-identical per-game HUD/profile
tables; source-scan helpers (`production_text`, `production_text_or_whole`, `strip_test_modules`, `strip_comments` — #7178, 2e9f31bef, replaced the BGSM ledger test's per-line stripper that tracked `"` only — and `count_identifier`) have ONE
home, `crates/core/src/source_scan.rs` (#5100, 655b317c9; #5469, 334458b3f routed the ~40 surviving inline cuts through it) — a
re-copied cut in a test module is a regression; `no_source_scan_cuts_with_the_weak_cfg_test_needle`
(`byroredux/src/workspace_hygiene_tests.rs`) fails a new weak `#[cfg(test)]` find/split outside its allowlist — and, since #6370 (594c843e2), a hand-written `mod tests {` find/split too. A whole-`Children` worklist walk has one home, `byroredux_core::ecs::collect_subtree` (visited set + `HierarchyTraversalGuard` budget, #5513);
`no_production_code_hand_rolls_a_children_walk` (`byroredux/src/workspace_hygiene_tests.rs`, #6577 fb6ec8afe) counts `.extend(children.0`
pushes outside the collector and ragdoll's two #4572-guarded walks, and the workspace-wide hygiene scans share one root walk
(`production_scan_roots` / `is_test_source`, bin + `crates/*` + `tools/`, #6522 3ccc57db6) — a scan that re-spells its own root list is a regression. The "prepend `meshes\` unless present" rule has one home, `byroredux_core::paths::{normalize_mesh_path, archive_mesh_path, canonical_mesh_key}` (#6956; moved from the bin's `asset_provider` — which re-exports them — so the renderer's mesh cache and nif's `geometries\` composer share it, #7145, 3ede92180); `no_production_code_prefixes_meshes_by_hand` (`byroredux/src/workspace_hygiene_tests.rs`) scans the bin and `crates/*/src` for every spelling that prepends the root (#7166, #7179: plain/raw literals, `{}` and `{name}` placeholders, `/` and `\\` separators, `Path::new("meshes").join`), its matcher pinned by `the_meshes_prefix_matcher_tells_prefixing_from_naming`. A binary-side placement resolves through the cached `FormIdPool` index, not a `World::find_by_form_id` scan: `no_binary_production_source_scans_for_a_form_id` (#7113, 209a11e74). The Z-up→Y-up conversion has one home, `crates/core/src/math/coord.rs` (`zup_to_yup_pos`,
`zup_to_yup_quat_wxyz`); re-exports elsewhere are not leaks — a genuine reimplementation would live outside that file.

### Dimension 3: Stale Documentation & Comments
Paths: `docs/`, `ROADMAP.md`, `HISTORY.md`, `README.md`, `CLAUDE.md`, doc comments across the tree
First step: `.claude/commands/_audit-validate.sh`

Doc rot misleads the next reader and the next audit. **Run the gate first**: `_audit-validate.sh` resolves every
backticked path in the audit skills AND `docs/engine/*.md` against the tree (fatal on STALE) and lists backticked
symbols found in no tracked `.rs` file (advisory — clear it, do not learn to scroll past it). STALE refs are
auto-eligible findings (trivial). Guards that already police a class — aim at what they cannot see:
- **GPU-struct sizes**: `gpu_material_size_claims` (`crates/renderer/src/vulkan/material_tests.rs`) scans `crates`,
  `byroredux`, `tools`, `docs/engine`, `.claude/commands` and the top-level status docs for a stale byte count of every
  size-pinned struct — `GpuMaterial`, `GpuLight`, `GpuTerrainTile`, `GpuInstance`, `GpuCamera` since #5203
  (`pinned_sizes()`; plus `bindings_glsl_states_the_real_struct_size`). It CANNOT see `Vertex::SIZE` (a size written
  without the type name) or `GpuWaterParams`-class structs outside `pinned_sizes()` — cross-check those against the
  layout tests, never the prose:
  `grep -rn "fn gpu_.*_is_[0-9]\+_bytes\|size_of::<Gpu" crates/renderer/src/vulkan/`. A GPU-size claim found stale in
  prose is MEDIUM (severity table).
- **Doc blocks spliced onto the wrong item**: a function inserted between another item's `///` block and its signature
  (#5809 → #6064, `init_resource` → #6136, `raised` → #6268, `install_block_model` → #7236, 09de853c5) leaves the contract on the wrong fn; the renderer has a
  source-scan guard (`no_item_is_spliced_into_another_items_doc_block`) and, since #6373 (546f5e2d4, which also moved
  eight more splices back), the bin and every crate share `no_doc_block_is_spliced_onto_the_wrong_item`
  (`byroredux/src/workspace_hygiene_tests.rs`, over `byroredux_core::source_scan::spliced_doc_blocks`). It is deliberately narrow —
  it only sees a doc whose FIRST backticked name is a sibling declared just below (a looser rule flagged 126 sites of ordinary
  prose), so a splice that does not open with a name still needs a reviewer (#7236's opened with "Install …", so it needed its own `every_rule_row_installer_documents_its_own_model` in `byroredux/src/combat.rs`): sweep the commits' hunks that add an item
  directly below a doc block.
- **Status-doc line ceilings**: `readme_and_roadmap_hold_their_line_ceilings` (`byroredux/src/workspace_hygiene_tests.rs`,
  #5332) fails README.md > 600 or ROADMAP.md > 800 lines (`wc -l` agrees with it). README.md reads 600 (exactly at the cap, so the next added line trips it; c9847c0da moved the clean-room section to `docs/clean-room-decoders.md` to get back there) and ROADMAP.md 800 on 2026-11-19 (also exactly at the cap; 795 on 11-15, 797 on 11-14) — a red hygiene test there is a finding (trim, do not raise the cap). The `docs/engine/game-loop.md`
  live-schedule table must name EVERY registered system: `every_registered_system_is_named_in_the_live_schedule_table`
  (`byroredux/src/workspace_hygiene_tests.rs`, #6583 4b13fd4d6, superseding #6378's by-name list) reads each `scheduler.add*`
  call under `boot/schedule/` from disk, so a new per-subsystem file is covered automatically; it matches whole identifiers.
- **Comment rule** (#6719, 27dcf26db; `docs/contributing.md` § Comments): a comment states the rule the code follows today and
  why, with an issue number if useful — no "used to", "pre-#NNNN this did", "the first fix added" or before/after narration, and
  no restating the next line. Comment-only rewrites go one file or function per commit with a diff that shows only comment
  lines changing, and only after searching for tests that pin comment text (`get_distance_variant_keeps_its_doc_comment`,
  `module_doc_covers_the_buoyancy_phase_and_its_ordering`, the scans in the source-scan triage records). Report history-narrating comments as
  LOW doc-rot with the file as the unit; the first pass was `crates/renderer/src/vulkan/context/skinned_blas_refit.rs`
  (`record_skinned_blas_refit`, ~160 comment lines → 80). Not a sweep target for a bulk rewrite.
- **Out-of-range line references**: `crates/core/tests/stale_line_references.rs` (#6167, #6514) fails a `file.rs:LINE` /
  `file.rs#LLINE` in a source comment or engine doc that points past the end of every file that name resolves to; it cannot see an
  in-range line that now means something else, and `docs/audits/`, `docs/archive/` and `HISTORY.md` are exempt. The remedy is to
  name the symbol and its module, not renumber.
- **Closed blocker named as live**: `no_production_source_names_the_closed_pre_avif_resolver_as_a_blocker`
  (`byroredux/src/workspace_hygiene_tests.rs`, #7218, 9765d4b1f) fails a production comment that still says Oblivion's actor values /
  ruleset / regen wait on the closed #3768 "pre-AVIF resolver" (they are wired, #7010 / #7197; a dozen comments had rotted that way). Its
  needles are built at run time so its own text cannot match — a *new* closed-issue framing is a Dim 3 / Dim 4 finding the guard cannot see.
- **Deleted `Material::classify_pbr`**: `no_source_file_frames_the_deleted_classify_pbr_as_live`
  (`byroredux/src/workspace_hygiene_tests.rs`) fails a doc that names it as live. The survivors are the free fn
  `classify_pbr_keyword` and `Material::resolve_pbr`.
- Beyond the guards, sweep: doc comments naming renamed/deleted symbols (`git log --diff-filter=D --since=<last>` then
  grep each deleted name); ROADMAP.md milestones "in progress" whose issues are closed (or vice versa) — cross-check
  `git log` / `gh issue`; **`docs/feature-matrix.md`** rows against the crate that implements them (`git log --grep M45`,
  …) — it is a status floor, so flag any row contradicting shipped code; HISTORY.md entries referencing reverted work;
  README.md / `CLAUDE.md` command examples whose flags or paths changed; per-game compat numbers in `docs/engine/game-compatibility.md`
  vs ROADMAP.md's matrix (ROADMAP is the single home); `docs/legacy/` Gamebryo source paths that moved;
  `crates/renderer/shaders/triangle.frag` comments quoting outdated struct sizes.
- **`docs/engine/*.md` are the "authoritative" references audits are told to believe** (`_audit-common.md`): beyond the gate's
  path/symbol check, sample their numbers and lists against code — stage order and lock order (`ecs.md`), VRAM/pool figures
  (`memory-budget.md`), shader binding tables (`shader-pipeline.md`), dependency and crate lists — a doc that misled an audit
  in the last 90 days is MEDIUM.
- **Path convention**: a backticked path or snake_case symbol in an audit skill asserts "exists right now"; deleted or
  forward-looking names go in italics or plain text (`_audit-common.md` Path-Reference Convention).

### Dimension 4: Audit-Finding Rot
Paths: `.claude/commands/`, `docs/audits/`
First step: `.claude/commands/_audit-validate.sh` then `ls .claude/commands/ docs/audits/ | tail -40`

Stale baselines actively misdirect future audits.
- STALE refs the gate prints in *other* audit skills → Dim 4 findings (trivial); advisory symbols in skills likewise.
- Numeric claims in skills and `_audit-common.md` (LOC figures, struct sizes, counts, "all N dimensions") — the gate
  cannot see numbers: re-measure a sample (`wc -l`, *prod_loc*, `grep -c`) and flag any off by >20%. Layout-row LOC
  drift has been the single most repeated finding here.
- "Existing: #NNN" / "open issue" callouts in skills where the issue is now CLOSED: `gh issue view N --json state`.
- Dimension cross-references between skills ("`/audit-x` Dim N") that no longer name that dimension.
- `docs/audits/` reports older than 90 days whose CRITICAL/HIGH findings have no GitHub trace. Caveat: reports
  dated before 2026-06-07 predate `/audit-publish`, so a missing issue is *not* evidence a finding is open (#3875,
  `_audit-common.md` Deduplication) — verify against code and say which you checked.
- Do NOT flag `.claude/issues/<N>/ISSUE.md` "Status: Open" drift: local issue files are immutable snapshots (TD10-001 / #1156);
  `gh issue view <N> --json state` is authoritative.

### Dimension 5: Stale Markers (TODO / FIXME / HACK / XXX / TBD)
Paths: `crates/`, `byroredux/`, `tools/` (not `tools/nifskope`), `crates/renderer/shaders/`
First step: the two greps below
```bash
grep -RInE '(TODO|FIXME|HACK|XXX|TBD|WIP|KLUDGE)\b' crates byroredux tools --exclude-dir=nifskope
grep -RInE '(TODO|FIXME|HACK|XXX|TBD|WIP|KLUDGE)\b' crates/renderer/shaders/
```
Keep the token list identical in both commands (an asymmetry once hid every shader `FIXME`, #3877); `TBD` is in it because
the codebase uses it. `tools/` (`byro-dbg`, `byro-launcher`, `byro-detect`, `texture-upscale`) is first-party and in scope.
**Triage each** (skip markers <30 days old unless they name a closed issue): `git blame` for age — over 6 months gets
reported; does it name an issue, and is that issue still open (closed issue + live marker = "marker outlived its driver");
does it name a milestone now complete per ROADMAP.md; a `// TODO: implement` on a path reachable from a shipped CLI flag
promotes (severity table).
**False positives**: `XXXX`, the ESM extended-size sub-record tag (key on comment content — "references the ESM `XXXX`
escape" — not a file list; `reader.rs`, `cell/wrld.rs`, the `b"XXXX"` test sentinels in `records/misc/magic.rs` and the
FourCC table doc in `sdk/src/compatibility/storage_util/mod.rs` all qualify); `HACK`, the FO4 terminal-record FourCC
(`crates/plugin/tests/parse_real_esm.rs`; since M43 also the `HACK` constant, docs and tests in `byroredux/src/lockpick.rs`,
`byroredux/src/interaction.rs` and `byroredux/src/commands/story_actions.rs`); a `// FIXME` quoting a reference implementation's
own FIXME (`crates/bgsm/src/bgem.rs`, `bs_geometry.rs`) documents upstream; a `TBD` that records an unresolved format
semantic together with its own resolution (the FNV `WEAP` `DNAM` arm in `crates/plugin/src/esm/records/items.rs`) is a
documented unknown, not a stale marker.
**Must-not-delete**: the third-party attribution block atop `crates/renderer/shaders/triangle.frag` (GLSL-PathTracer MIT
notice + Burley citation, ~first 30 lines). Flag any edit that strips it — MIT requires the notice to travel with the code.

### Dimension 6: Stub & Placeholder Implementations
Paths: `crates/`, `byroredux/`, `tools/`
First step: the two greps below
```bash
grep -RInE 'unimplemented!|todo!\(\)|panic!\("not ' crates byroredux tools --exclude-dir=nifskope
grep -RInE '// *(stub|TODO: real|placeholder|not yet)' crates byroredux tools --exclude-dir=nifskope
```
The first command should return nothing — any hit is notable. For each: reachable from a shipped CLI flag or smoke test →
MEDIUM; functions returning `None` / `Vec::new()` / `Default::default()` under a stub comment; trait impls with empty bodies
the trait docs say should do work; console commands in `byroredux/src/commands/` that no-op or print "TODO".
**Production-unreachable features** are the yield here: a public builder/system with zero non-test callers whose docs or
ROADMAP say it is wired (a ruleset builder that `main` never calls; a component nothing inserts). For a candidate
`name`: `grep -rnw name crates byroredux tools --include='*.rs'` and drop test-only hits. Cross-check per-game ESM record
coverage in `crates/plugin/src/esm/records/` against the ROADMAP.md compat matrix (wired vs stubbed).

### Dimension 7: Magic Numbers & Hardcoded Constants
Paths: `crates/nif/src/blocks/`, `crates/renderer/`, `crates/plugin/src/esm/records/`
First step: read the version-gate and budget sites; do not regex blindly (most literals are legitimate)

- Bare numeric literals compared against version codes in `crates/nif/src/blocks/` → a `NifVersion` constant.
- Vulkan `MAX_*` / `MIN_*` hardcoded inline → `vk::PhysicalDeviceLimits` or a named constant.
- **Shader `#define` provenance**: every define is generated from `crates/renderer/src/shader_constants_data.rs`
  (`include!`d by `shader_constants.rs` and `crates/renderer/build.rs`, which emits `shaders/include/shader_constants.glsl`);
  source-scan tests in `shader_constants.rs` police the generated header. The check is "every shader `#define` / loop
  bound / budget literal that bypasses that file" (lockstep risk HIGH — *feedback_shader_struct_sync*).
- GPU `#[repr(C)]` size literals: reference the layout pins (Dim 3 has the recipe and the guards); an inline size
  literal that should reference them is a code finding here, a stale prose figure is Dim 3.
- Frame/ray/cache budgets (`MAX_TOTAL_BONES`, `MAX_MATERIALS`, …) scattered vs one tunable module; ESM sub-record sizes
  hardcoded (`if data.len() == 24`) → a named constant on the record struct.
- Do NOT flag protocol-defined magic: FourCC tags, BSA/NIF/BA2 magic, Vulkan format enums.

### Dimension 8: Dead Code & Backwards-Compat Cruft
Paths: `crates/`, `byroredux/`, `tools/`, every `Cargo.toml`
First step: `cargo clippy --workspace --keep-going -- -D warnings` (the CI gate) and the greps below
```bash
grep -RInE 'allow\(dead_code\)' crates byroredux tools --exclude-dir=nifskope
grep -RInE '#\[deprecated\]|// *removed:|_unused|fn .*_unused' crates byroredux
cargo machete --with-metadata 2>/dev/null || echo "cargo machete not installed — scan Cargo.toml deps vs use stmts"
```
- **Unused-dependency gate**: CI job `Unused dependencies` runs `cargo machete --with-metadata` (#3890; `--with-metadata`
  since #5851, 251d491ef, so `[dev-dependencies]` workspace entries are seen too, plus a `Cargo.lock` drift check) — a red
  run is a finding. Its blind spots are macro-only and re-export-only use (the job comment says so); the hand scan earns its
  keep there and on deps used only under a non-default feature. Since the 1.100 pin, cargo's own `cargo::unused_dependencies`
  lint (warn by default) covers that last case on any full-target build: its first run caught `byroredux`'s non-optional
  `tracing-subscriber`, referenced only under `feature = "tracing-tracy"` (3c9949ec9 made it optional behind that feature).
- **Clippy gate** (CI job `Test + Check + Clippy` runs `cargo clippy --workspace --keep-going -- -D warnings` plus a
  renderer-only `undocumented_unsafe_blocks` re-lint, #5121; the toolchain is pinned by `rust-toolchain.toml` since #5308 (b24cb46b6) —
  1.96.0, raised to 1.100.0 by 3c9949ec9 on 2026-11-14 under the deliberate-bump rule: the full gate run locally first — after floating
  `stable` let rustc 1.99's *chunks_exact_to_as_chunks* wave turn the gate red for four days; rustc 1.96 (800802516) and 1.98
  (530c9e7aa) raised lints on untouched code too, and the distro cargo ignores the pin; wasmtime 49's MSRV is exactly 1.96.0, 15a6b1d2d,
  so the pin can rise but not fall). **State at 8ff82d514: both forms are at zero** — 3c9949ec9 cleared 140 sites the pin had kept out of
  view (*as_chunks*, a wider *needless_range_loop*, *needless_borrows_for_generic_args*, *type_complexity*), CI is green on both, and ROADMAP
  retired its "`--all-targets` carries 20 lints" known issue (8ff82d514); a hit from either form is new debt, not a known one. The blocking
  gate had gone red on untouched code before and was re-zeroed by #6222 (fde8836dd, 2026-10-27) and #6582 (72673394b, 2026-11-02, nine sites
  hidden behind the first three failing crates), was red again from Session 110 until just after the Session 112 close, and went red on feature-branch code (door / fall / ray-budget lints) until 4288659fa (#7085, 2026-11-20) cleared it, and red again on 2026-11-22 (five lints across six files, the first in `crates/core/src/source_scan.rs`) until 3e14962a7 cleared it. A red gate is a finding:
  list each failing lint and file, plus the crates it left unlinted — `--keep-going` lints every crate that still
  compiles, but a crate red under clippy leaves its dependents unchecked (a red `byroredux-nif` skips the bin).
  CI also runs `--all-targets --keep-going` as a NON-blocking step (`continue-on-error`, #5621, a3ccd68fb; a red run raises a warning
  annotation and a job-summary line, #6492 — `continue-on-error` alone reports `success`): test/example-target lints re-accumulate on
  active days (re-zeroed in 9896b1cf7 / 8b0b922f5, #5849 / #5850, #6193 — 42 sites incl. three deny-level `erasing_op`, #6394, #6492 —
  six audited sites had grown to twenty — and 3c9949ec9). A red non-blocking step is a lower-priority bucket, not a gate failure (the
  workspace example build itself went red once, the #3894 class, 530c9e7aa); with the lane at zero, a first re-accrual is the
  signal to fix before it compounds. Check every
  `#[allow(clippy::…)]` carries a reason comment (the house style for `too_many_arguments`) — enforced since #5622
  (c4975b1ab) by `every_clippy_allow_carries_a_reason` (`byroredux/src/workspace_hygiene_tests.rs`: a trailing comment, a
  comment/doc line on the contiguous block above, or one within three lines below), so a hit is a guard failure to
  route there, not a sweep to repeat. 3c9949ec9's `#[allow(clippy::duplicated_attributes)]` on papyrus's logos `Token` carries one
  (the lint reads logos's `#[regex]` helper attributes and its suggestion would delete a lexer rule) — leave it.
- Each `#[allow(dead_code)]`: called now or still dead? Delete if dead. `pub fn` in a private module nobody imports (`cargo +nightly rustc -p <crate> -- -W unused`);
  `mod.rs`/`lib.rs` re-exports with no consumer; `_`-prefixed params that survived a refactor (delete, do not rename);
  `// removed: …` breadcrumbs (delete completely); re-exports of deleted types "for compatibility" (no external consumers
  exist — pure rot); `#[deprecated]` items with no consumers; `Cargo.toml` feature flags with one branch always on/off.
- **Do NOT flag**: `cfg(test)` / `cfg(debug_assertions)`-gated code, FFI boundary functions, or the public API of a
  workspace-internal crate a future binary will consume (note it rather than deleting).

### Dimension 9: Test Hygiene
Paths: every `#[test]`/`#[ignore]` site, `.github/workflows/ci.yml`, `byroredux/tests/`
First step: the ignore-triage command below
```bash
# All ignores (matches BOTH `#[ignore]` and the reason form `#[ignore = "…"]` — do not tighten to a bare `]`)
grep -RInE '^[[:space:]]*#\[ignore' --include='*.rs' crates byroredux tools
# Ignores whose reason is NOT a data/hardware gate — the triage input
grep -RInE '^[[:space:]]*#\[ignore' --include='*.rs' crates byroredux tools | grep -viE 'game data|installed|\.(bsa|ba2|esm)|BSA|BA2|ESM|audio device|vulkan|gpu|rt-capable|display|opt-in|corpus|real (data|master)|steam|env'
```
- **Guards**: `workspace_hygiene_tests` (`byroredux/src/workspace_hygiene_tests.rs`) — `every_ignore_attribute_carries_a_reason`
  (a bare `#[ignore]` fails; the reason text is the triage input), `no_tmp_scratch_examples_are_committed` (`_tmp_*` /
  `tmp_*` probes) plus `no_committed_example_self_describes_as_disposable` (module doc opening "throwaway / one-off / TEMP",
  #5114), `no_test_attribute_is_stacked_on_another`, `example_targets_hold_their_per_crate_ceilings` (per-crate example
  file/LOC ceilings, #5856 — a raised ceiling must come with a re-measure in the same commit — 3c9949ec9 raised ui +3 / pex +1 for the new *type_complexity* aliases, 17a2e2cf4 re-measured nif and terrain-height, 022450758 plugin), and the `classify_pbr` check
  (Dim 3). Confirm they are not themselves `#[ignore]`d.
- **Closed-HIGH real-data pins**: `.github/real-data-pins.txt` lists the `#[ignore]`d real-data tests guarding closed
  CRITICAL/HIGH fixes; no hosted runner executes them (`scripts/run-real-data-pins.sh` is the local run). Since #6183
  (9ee74b6e7) `every_real_data_pin_names_an_ignored_test_in_its_lane` checks each row names a real ignored `fn` its lane
  compiles. A closed-HIGH guard test that is `#[ignore]`d but absent from the list is the MEDIUM row above (#6224, 6adf6b5d8 appended four such rows;
  `/fix-issue`'s completeness checklist now has a PINS line). The runner counts passes with `scripts/count-passed-tests.sh`
  (per-binary `test result: ok. N passed` sums, #6319, 718c7b836), since counting `test NAME ... ok` lines misreads a printing test.
- **Source-scan tests** (a `#[test]` that `include_str!`s or reads a `.rs` file and searches its text): a standing test-gap
  bucket under triage, with per-issue records at `.claude/issues/<N>/CLASSIFICATION.md` — #6686 (binary, OPEN: four passes, the fourth on 2026-11-10 classified every remaining file — mostly by doc, not by reading —
  and replaced four tests; a fifth pass (f5210e6e6) replaced `skin_offsets_hasher_tests` with a type ascription, leaving the `sandbox` / `zup_yup` rows as candidates),
  #6687 (renderer, closed 2026-11-15 by d84a67cd9: the Fx-hash cluster in `context/mod.rs` became type ascriptions in
  `hot_path_maps_are_fx_hashed` and the `shadow_filter.rs` count an exhaustive destructure; earlier, 055a5ca11 made the UI instance's two clamps pure functions with value tests), #6688 (scripting, closed, 58caf1799 ran the
  Pattern-B marker consumers instead of scanning them) and #6689 (nif/plugin/physics, closed, 8124d9594). Classes: device/data (needs Vulkan, the
  winit loop or game data — keep), runtime (replaceable by running the code), prose, syntax, input (a read that feeds a check). A scan whose
  property a behavioural test can state is the finding (precedents: #6685 turned the system-access scan into a `syn` walk,
  `byroredux/src/acquisition_scan.rs`; #6684 made seven schedule-order guards read the built schedule; 27c61842f replaced three renderer scans);
  a new scan needs a doc stating why no runtime form exists. Read the record before filing, and add to the
  record, not a new issue; the renderer's per-file self-include baseline (`a_new_self_scan_goes_through_production_text_or_is_reviewed_into_the_baseline`, `crates/renderer/src/source_scan.rs`) must move down in the same commit as a scan it counts.
- Most `#[ignore]`s gate Vulkan / game-data / audio tests — not debt. Triage the remainder: does a reason name an issue
  (`gh issue view N`), and if it guards a closed CRITICAL/HIGH fix → MEDIUM.
- Tests with only smoke assertions (`assert!(result.is_ok())`); commented-out assertions in passing tests; tests that
  `println!` without an assert.
- **Feature gates vs CI lanes**: enumerate `[features]` (`grep -A6 '^\[features\]' crates/*/Cargo.toml byroredux/Cargo.toml`)
  and compare with `.github/workflows/ci.yml` — non-default states run only where a lane names them (today: `core`
  `inspect`/`save` and `scripting` `save` via workspace unification, `byroredux-spt --features recon`, `byroredux` without `debug-server` / with
  `tracing-tracy` / with `dhat-heap`, `byroredux-nif --features dhat-heap`). A feature or `required-features` target with no
  lane can rot silently (the class #3894 / #4387 / #4390 closed).
- `byroredux/tests/golden_frames.rs` (opts into `--ignored`) — still runnable, golden images current.
- Cross-reference "must not regress" lines in other audit skills — each named regression test still exists and is not
  `#[ignore]`d (`_audit-validate.sh` advisory names symbols that are gone).

## Cross-Dimension Dedup

A TODO inside a dead function reports under Dim 8, not also Dim 5. Material doc rot reports under Dim 3, not Dim 8. A stale
GPU-size doc comment is Dim 3; a stale GPU-size *code literal* is Dim 7. NIFAL/material *translation correctness* is out of
scope (`/audit-nifal`) — this audit owns only the debt around that tier.

## Phase 3: Merge

1. Read all `/tmp/audit/tech-debt/dim_*.md`.
2. Combine into `docs/audits/AUDIT_TECH_DEBT_<TODAY>.md`: **Executive Summary** (findings by severity + delta vs
   `baseline.txt`), **Baseline Snapshot** (the Phase-1 counts), **Top 10 Quick Wins** (trivial/small effort),
   **Top 5 Medium Investments** (splits, consolidations), **Findings** (HIGH → MEDIUM → LOW, then by dimension),
   **Deferred** (gated on an in-progress milestone; name it).
3. Remove cross-dimension duplicates per the rules above.

## Phase 4: Cleanup

`rm -rf /tmp/audit/tech-debt`; tell the user the report is ready; suggest
`/audit-publish docs/audits/AUDIT_TECH_DEBT_<TODAY>.md`.

## GitHub Labels

Findings publish under `tech-debt` (plus the standard `<severity>` and `<domain>` labels; `/audit-publish` applies it for
`TECH_DEBT` reports). Two sibling kind labels split the bucket — apply the one that matches:
- **`doc-rot`** — documentation drifted from code (stale ROADMAP row, a SKILL naming a deleted symbol, a comment describing
  removed behaviour); publishes as `documentation`, not `bug`.
- **`test-gap`** — missing, vacuous or non-asserting coverage (an `#[ignore]`d test with no data gate, an entry point with
  zero tests).
Pure debt (dead code, duplication, magic numbers, oversized files, stale markers) stays `tech-debt` + `bug`.
