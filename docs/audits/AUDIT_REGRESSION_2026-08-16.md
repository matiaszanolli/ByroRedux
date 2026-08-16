# Regression Verification Audit — 2026-08-16

Verification that CLOSED bug fixes are still in place, plus the sweep-wide hunt
for **regression guards that are green by construction** — a guard that cannot
fail is not protection, it is a fix with no guard wearing a passing badge.

**Scope**: the 120 most-recently-closed issues (window 2026-08-08 → 2026-08-16),
the six "fresh verification candidates" named in `audit-regression/SKILL.md`
(#1815 / #1816 / #1728 / #1740 / #1731 / #1718), the unconditional Step-4
fragile-area contracts (NIFAL tier + Disney BSDF + `#[repr(C)]` GPU struct pins),
and two explicit verification requests relayed from `/audit-nif` and
`/audit-oblivion`.

**Live evidence, not quoted**: three opt-in gates were actually run against
on-disk game data on this host, not read.

## Executive Summary

**No closed fix was found missing.** Every one of the 27 fixes sampled across
the discovery window is physically present in the live tree, and all three
Step-4 contract families are intact. Zero FAILs, zero regressions of code.

The findings are all of one shape, and it is this sweep's dominant theme: **five
closed fixes are guarded by something that cannot fail.** Three mechanisms show
up:

1. A gate whose comparison is *one-directional by design* — it detects new
   truncations, so the five files it lists that no longer truncate are
   permanently un-guarded, and the `parsed=` count it writes into its own
   baseline is never read back.
2. A "guard" that asserts nothing at all — `run_skinning_invariant` is 145 lines
   of `eprintln!` behind three tests whose names end in `_invariant_check` /
   `_check`, in a file whose module doc advertises "all live regressions, no
   soft flags".
3. A fixed function **duplicated into a second, unguarded call site** the day
   after its issue closed — `#2955`'s `effective_actor_level` now exists twice,
   and the regression test only reaches one copy.

Plus one documented-invariant break: `ActorValues`' key space was widened from
"AVIF FormID, one space" to "AVIF FormID *or* Skyrim's engine enum index"
without updating `#1663`'s `GetActorValue` consumer, whose doc comment still
asserts the single-space rule three lines above the lookup.

**Total findings**: 5 (0 CRITICAL, 0 HIGH, 4 MEDIUM, 1 LOW).
**Fixes verified present**: 27 / 27. **FAILs**: 0. **Regressions of code**: 0.
**Regression-guard gaps (regressions waiting to happen)**: 5.

### Live gate runs (this host, this sweep)

| Gate | Invocation | Result |
|---|---|---|
| Per-block baselines | `cargo test -p byroredux-nif --test per_block_baselines --release -- --ignored` | **7 / 7 PASS** (Oblivion 82 types matched) |
| Block-coverage baselines | `cargo test -p byroredux-nif --test block_coverage_baselines --release -- --ignored` | **7 / 7 PASS** — Oblivion parity reports `8031/8032 NIFs whole, 1 truncating` |
| GPU struct size pins | `cargo test -p byroredux-renderer --release gpu_` | **67 passed, 0 failed** |

## Dimension Roll-Up (every dimension, including clean ones)

| # | Dimension | Findings |
|---|---|---|
| 1 | Closed-issue discovery & fix presence (SKILL Steps 1–3) | **2** (2 MEDIUM) |
| 2 | Guard-test existence & liveness for closed fixes (Step 3) | **0** — folded into D1/D5 where a guard gap was the finding |
| 3 | Step 4 — NIFAL canonical-translation tier | **0** (clean) |
| 4 | Step 4 — Disney BSDF + `#[repr(C)]` GPU struct contracts | **0** (clean) |
| 5 | Green-by-construction guard sweep (sweep theme) | **3** (2 MEDIUM, 1 LOW) |
| 6 | Sibling-audit verification requests (#2574 / #2564 / bug-enforcing guards) | **0 new** — 3 verifications + 1 correction to `/audit-nif` |

Scratch notes: `/tmp/audit/regression/dim_1.md` … `dim_6.md`.

---

## Dimension 1 — Closed-issue discovery & fix presence · **2 findings**

### Fix-presence sample (all PASS)

Each fix was located by issue-number grep in the live tree and the cited symbol
re-read, not inferred from the commit log.

| Issue | Fix site (live) | Present |
|---|---|---|
| #2856 | `crates/physics/src/world.rs:454`, `:1726` | Yes |
| #2857 | `byroredux/src/systems/character.rs:247` | Yes |
| #2858 | `byroredux/src/scene.rs:146`, `:1069`, `:1126` | Yes |
| #2859 | `byroredux/src/scene.rs:309`, `byroredux/src/render/mod.rs:41`, `:136` | Yes |
| #2860 | `crates/physics/src/convert.rs:22`, `:108`, `:161` | Yes |
| #2714 | `crates/ui/src/host.rs:16`, `:361` | Yes |
| #2715 | `crates/renderer/src/texture_registry_tests.rs:556` | Yes |
| #2736 | `crates/renderer/src/vulkan/presentation.rs:97`, `:179` | Yes |
| #2739 | `crates/renderer/src/vulkan/sync.rs:245-250` (null-out pass) | Yes |
| #2740 | `crates/renderer/src/vulkan/buffer.rs:767`, `:961` | Yes |
| #2741 | `crates/renderer/src/vulkan/caustic.rs:1579`, `taa.rs:1072` | Yes |
| #2742 | `crates/nif/src/import/material/texture_slot_3_4_5_tests.rs:1389` | Yes |
| #2743 | `crates/renderer/src/mesh.rs:172`, `:846`, `:966` | Yes |
| #2744 | `crates/renderer/shaders/cluster_cull.comp:116`, `:193`, `:204` | Yes |
| #2745 | `crates/renderer/shaders/triangle.frag:543` (mesh-ID no longer masked) | Yes |
| #2923 | `crates/renderer/src/vulkan/context/mod.rs:4338-4366` | Yes |
| #2929 | `crates/renderer/src/vulkan/acceleration/memory.rs:205` | Yes |
| #2931 | `crates/renderer/src/vulkan/acceleration/tlas.rs:285` + `acceleration/tests.rs:2036` | Yes |
| #2932 | `crates/core/src/character/regen.rs:178-207` | Yes |
| #2933 | `crates/scripting/src/condition.rs:448`, `:1658` | Yes |
| #2955 | `byroredux/src/npc_spawn.rs:131-137`, `:148-156` | Yes (but see REG-D1-01) |

SKILL-named fresh candidates:

| Issue | Fix site (live) | Present |
|---|---|---|
| #1815 | `crates/pex/src/decompile/boolean.rs:56` (`MAX_REBUILD_DEPTH`), `:140-148`; guard `:759` | Yes |
| #1816 | `crates/scripting/src/translate/mod.rs:111`, `crates/scripting/src/fragment.rs:1445` | Yes |
| #1728 | `crates/pex/src/lib.rs:310` (Skyrim BE), `:411` (Starfield) | Yes |
| #1740 | `crates/scripting/tests/pex_recognize_e2e.rs` (both halves, `#[ignore]`d) | Yes |
| #1731 | `crates/plugin/src/esm/reader.rs:388` + `esm/cell/tests/addn_stat.rs:302`, `:332` | Yes |
| #1718 | `byroredux/src/ragdoll.rs:90`, `:203`, `:1506` | Yes |

Per the SKILL's own note, **#1651 was not re-verified** — its premise was
disproven and reverted by #1823.

---

### REG-2026-08-16-D1-01: `#2955`'s fix was copy-pasted into `inventory.rs` a day after it closed, and only the original copy is guarded

- **Severity**: MEDIUM
- **Dimension**: Closed-issue discovery & fix presence
- **Location**: `byroredux/src/inventory.rs:179-185` (the duplicate), `byroredux/src/npc_spawn.rs:131-137` (the fixed original), `byroredux/src/npc_spawn/tests.rs:891` (the guard that reaches only the original)
- **Status**: NEW
- **Description**: `#2955` (HIGH, closed 2026-08-15 by `4f1eb7dd`) established that
  an `NPC_`'s ACBS `level` field is a **PC-level multiplier**, not a level, when
  `ACBS_PC_LEVEL_MULT` is set, and routed every numeric reader through one
  `effective_actor_level` helper in `byroredux/src/npc_spawn.rs`. The next day,
  `09682c71` added `byroredux/src/inventory.rs` with a **second, private
  `effective_actor_level`** rather than importing the fixed one. Both copies
  currently implement `#2955` correctly, so this is not a code regression — but
  the two have **already diverged** on their clamp (`npc_spawn`: `npc.level.max(0)`;
  `inventory`: `actor.level.max(1)`), and the `#2955` regression test
  `pc_level_mult_actors_resolve_to_calc_min_not_the_raw_multiplier` calls the
  `npc_spawn` copy exclusively. Nothing in the workspace can detect the
  `inventory.rs` copy losing the `calc_min` branch.
- **Evidence**:
  ```rust
  // byroredux/src/npc_spawn.rs:131 — the #2955 fix, guarded
  fn effective_actor_level(npc: &byroredux_plugin::esm::records::NpcRecord) -> i16 {
      if npc.acbs_flags & byroredux_plugin::esm::records::ACBS_PC_LEVEL_MULT != 0 {
          npc.calc_min.max(1) as i16
      } else { npc.level.max(0) }
  }

  // byroredux/src/inventory.rs:179 — the copy, unguarded, divergent clamp
  fn effective_actor_level(actor: &NpcRecord) -> i16 {
      if actor.acbs_flags & ACBS_PC_LEVEL_MULT != 0 {
          actor.calc_min.max(1) as i16
      } else { actor.level.max(1) }
  }
  ```
  `grep -rn "effective_actor_level"` returns two definitions and no shared import.
  The `inventory.rs` copy feeds three `expand_leveled_form_id` calls plus
  `resolve_inherited_inventory` (`inventory.rs:123-170`) — the exact
  "highest eligible tier" selector whose misuse `#2955` was filed to stop.
- **Impact**: `#2955`'s stated blast radius is "every levelled NPC gets top-tier
  gear". That failure mode is now reachable again through a code path with no
  test. It is *latent today* only because `FO3-D4-01` (`/audit-fo3`, HIGH) shows
  `PLAYER_BASE_FORM_ID = 0x14` never resolves, so `build_player_template` returns
  early and the duplicate never executes — which makes this strictly worse, not
  better: the moment `FO3-D4-01` is fixed, an unguarded copy of a HIGH fix goes
  live on the player's starting loadout.
- **Related**: `#2955` (the fix); `/audit-fo3` `FO3-D4-01` (why the copy is
  currently dormant); the global instruction *"always prioritize improving
  existing code rather than duplicating logic"*.
- **Suggested Fix**: Delete `inventory.rs:179-185` and call the `npc_spawn`
  helper (promote it to `pub(crate)`), reconciling the `max(0)`/`max(1)`
  divergence deliberately in the one surviving body. Then extend
  `pc_level_mult_actors_resolve_to_calc_min_not_the_raw_multiplier` to assert
  through `build_player_template` as well, so the guard covers both consumers.

---

### REG-2026-08-16-D1-02: `ActorValues` grew a second key space and `#1663`'s `GetActorValue` consumer was not told

- **Severity**: MEDIUM
- **Dimension**: Closed-issue discovery & fix presence
- **Location**: `crates/core/src/ecs/components/actor_values.rs:13-19` (the widened contract), `crates/plugin/src/esm/records/index.rs:372` (`SKYRIM_HEALTH_ACTOR_VALUE`), `:598-604` (`health_actor_value_key`), `crates/scripting/src/condition.rs:416-431` (the un-updated `#1663` consumer)
- **Status**: NEW
- **Description**: `#1663` established `ActorValues` as a map keyed by **AVIF
  FormID in global load-order space**, and `crates/scripting/src/condition.rs`'s
  `GetActorValue` arm depends on that being the *only* key space — it looks
  `param_1` up directly with no per-game rule. `eb5d76fe` (2026-08-16) widened
  the contract: record-backed values keep AVIF FormIDs, but built-in TES5 values
  now use *Skyrim's engine actor-value enum index*, with
  `SKYRIM_HEALTH_ACTOR_VALUE: u32 = 24` hardcoded. Two consequences, neither
  guarded: (a) the two spaces share one `HashMap<u32, ActorValue>` with no
  assertion that they cannot collide; (b) `condition.rs` was never updated, so
  on Skyrim `GetActorValue(Health)` — a CTDA whose `param_1` is remapped to an
  AVIF FormID — can never find the value now stored under `24`. The new
  `ActorVitals` companion exists precisely to carry the per-game Health key, but
  its only readers are `byroredux/src/combat.rs`, `byroredux/src/commands/view.rs`
  and the save registry; the condition evaluator is not among them.
- **Evidence**: `crates/scripting/src/condition.rs:420-424`, three lines above
  the lookup, still states the now-false invariant:
  > *"`param_1` is the AVIF FormID, already promoted to global load-order space
  > at parse time … **the same space `ActorValues` is keyed in** — a direct
  > lookup, no FormIdPool hop … #1663."*

  Contradicted by `crates/core/src/ecs/components/actor_values.rs:13-19`:
  > *"Built-in TES5 actor values use Skyrim's engine enum index (for example
  > Health is 24), because vanilla does not author AVIF records for them."*

  `grep -rn "ActorVitals"` shows no hit in `crates/scripting/`.
- **Impact**: Any Skyrim CTDA gating on Health (`GetActorValue`, function 14)
  silently evaluates to `0.0` via the absent-AV default — Bethesda's
  "safe-default" contract laundering a lookup miss into a plausible answer, so
  the failure is invisible at runtime. Separately, the un-asserted shared key
  space is a live collision hazard the moment a load order authors an AVIF whose
  global-space FormID is a small integer. Blast radius is Skyrim condition
  evaluation and anything downstream of it (package gating, dialogue, quest
  conditions).
- **Related**: `#1663` (the contract this weakens); `#1666` (the CTDA FormID
  remap that makes the AVIF assumption hold for FNV/FO3); `#2933`
  (the sibling `GetActorValue` correctness fix in the same arm);
  `/audit-character` `CHAR-2026-08-16-D1-01` (the P2 slice as a CHARAL
  non-consumer — same "new gameplay code bypasses the canonical layer" root).
- **Suggested Fix**: Have `condition.rs`'s `GetActorValue` arm consult
  `ActorVitals` (and any future canonical-key companion) before falling back to
  the raw `param_1` lookup, so the per-game key rule lives in exactly one place;
  and add a debug assertion in `ActorValues::from_pairs` that the built-in-enum
  keys and AVIF FormID keys present on one actor are disjoint.

---

## Dimension 2 — Guard-test existence & liveness · **0 findings**

Every closed fix sampled in Dimension 1 has a locatable guard, and the three
guard families that could be run on this host were run green (see the live-gate
table above). The two cases where the *guard* rather than the *fix* is the
problem are reported under D1 (REG-D1-01) and D5 (REG-D5-01, REG-D5-02) rather
than duplicated here.

---

## Dimension 3 — Step 4: NIFAL canonical-translation tier · **0 findings (clean)**

| Contract | Live state |
|---|---|
| Single `ImportedMesh → Material` boundary | `byroredux/src/material_translate.rs:109` is the only production `fn translate_material`; the three other hits are its own tests |
| `Material::metalness` / `roughness` stay plain `f32` | `crates/core/src/ecs/components/material.rs:24-25` — plain `f32`; no `Option<f32>` reintroduced on either. `resolve_pbr` at `:878` |
| Typed particle emitters | `NiPSysEmitterCtlr` (`crates/nif/src/blocks/mod.rs:1121`), `NiPSysEmitterCtlrData` (`:1029`), `NiPSysGrowFadeModifier` (`:1053`) all still typed dispatch arms — no regression to opaque `NiPSysBlock` |
| Emitter param plumbing | `extract_emitter_params` / `extract_emitter_rate` at `crates/nif/src/import/walk/mod.rs:766` / `:865`; consumer `apply_emitter_params` at `byroredux/src/systems/particle.rs:29` |
| Collision shape coverage | `BhkMultiSphereShape` (`crates/nif/src/import/collision/shape.rs:110`) and `BhkConvexListShape` (`:235`) both still resolve to a `CollisionShape` |

---

## Dimension 4 — Step 4: Disney BSDF + GPU struct contracts · **0 findings (clean)**

| Contract | Live state |
|---|---|
| Disney/Burley lobe lives in the include | `crates/renderer/shaders/include/pbr.glsl` present, 498 lines |
| MIT attribution travels with the code | `crates/renderer/shaders/triangle.frag:19-30` — GLSL-PathTracer / Asif Ali MIT notice + Burley 2012 cite intact |
| `resRadiance[]` **stays retired** (verify gone, not intact) | Two mentions workspace-wide, both explanatory comments (`include/lighting.glsl:85`, `triangle.frag:2586`). No array declaration, no re-added G-buffer reservoir attachment |
| WRS recomputes via `shadowableLightRadiance` | Declared `include/lighting.glsl:92`, six live call sites in `triangle.frag` |
| `GpuInstance` = 128 B | `gpu_instance_is_128_bytes_std430_compatible` — green |
| `GpuCamera` = 336 B | `gpu_camera_is_336_bytes` — green |
| `GpuMaterial` = 348 B | `gpu_material_size_is_348_bytes` (`crates/renderer/src/vulkan/material.rs:1382`) — green |

`cargo test -p byroredux-renderer --release gpu_` → **67 passed, 0 failed**.

---

## Dimension 5 — Green-by-construction guard sweep · **3 findings**

### REG-2026-08-16-D5-01: The Oblivion truncation gate is one-directional, so five of its six baseline files are permanently un-guarded — and the `parsed=` count it writes is never read back

- **Severity**: MEDIUM
- **Dimension**: Green-by-construction guard sweep
- **Location**: `crates/nif/tests/block_coverage_baselines.rs:85` (`oblivion_block_count_parity`), `:147-158` (the comparison), `:96-105` (the two silent `continue`s), `crates/nif/tests/data/block_coverage_baselines/oblivion_truncations.tsv:1-7`
- **Status**: NEW (escalation of the OPEN #2564, which frames the same drift as documentation staleness only)
- **Description**: The gate compares only `truncating.keys()` **minus** the
  baseline set and fails on non-empty difference. Improvements are silent by
  design and the module doc says so. The consequence nobody has stated is the
  other half: a file that *is* in the baseline can start truncating, stop
  truncating, and start again without the gate ever going red. Live measurement
  this sweep shows the baseline lists **6** files while only **1** actually
  truncates — so `meshes\marker_arrow.nif`, `marker_divine.nif`,
  `marker_radius.nif`, `marker_temple.nif` and `marker_travel.nif` are five
  vanilla NIFs that the parser now handles correctly and that **no test can
  detect losing again**. Compounding it, the baseline's header carries
  `parsed=8032`, but the reader (`:147-152`) filters out `#` lines and takes only
  the first tab field of the rest — so `parsed` is written and never asserted.
  A regression that turns whole-parsing files into hard `parse_nif` errors drops
  them out of *both* `parsed` and `truncating` via the `continue` at `:103-105`,
  and this gate stays green.
- **Evidence**: live run on this host —
  ```
  [Oblivion] block-count parity: 8031/8032 NIFs whole, 1 truncating
  [Oblivion] no new truncation (1 known, all in baseline)
  test oblivion_block_count_parity ... ok
  ```
  against `oblivion_truncations.tsv:1` = `# Oblivion sizeless-truncation baseline	truncating=6	parsed=8032`
  followed by six file rows. Five of those six no longer reproduce.
  The reader that ignores `parsed`:
  ```rust
  let baseline: BTreeSet<String> = text.lines()
      .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
      .filter_map(|l| l.split('\t').next().map(str::to_string))
      .collect();
  ```
- **Impact**: Oblivion is the only sizeless-header game in the corpus, so this
  gate is the *sole* structural defence against an F-01-class dispatch-arm loss
  there — and it is currently blind on five files and on the whole
  hard-error-count axis. The gate's own docstring names
  `handscythe01.nif` / `oar01.nif` / `ungrdltraphingedoor.nif` as the regression
  it exists to catch; that class of regression on the five stale rows is exactly
  what it can no longer catch.
- **Related**: **#2564** (OPEN — same drift, framed as doc staleness; this is the
  guard-coverage half it does not state); `#1611` (the commit that last
  regenerated the baseline to 6); `/audit-nif`'s Issue-Tracker section, which
  quantifies the live count at 1 but reads the gate as healthy.
- **Suggested Fix**: Regenerate `oblivion_truncations.tsv` (closing #2564's data
  half), then make the gate two-directional: assert set equality rather than
  subset, and assert the baseline's `parsed` count against the live one so a
  file that starts hard-erroring cannot silently leave the corpus. Both changes
  are in the same function.

---

### REG-2026-08-16-D5-02: `run_skinning_invariant` asserts nothing — three `_check` tests in `skinning_e2e.rs` are `eprintln!`-only, in a file whose doc claims "no soft flags"

- **Severity**: MEDIUM
- **Dimension**: Green-by-construction guard sweep
- **Location**: `byroredux/tests/skinning_e2e.rs:188-332` (`run_skinning_invariant`), `:584` (`oblivion_skinning_invariant_check`), `:603` (`fnv_skinning_invariant_check`), `:346-580` (`oblivion_vertex_world_check`)
- **Status**: NEW
- **Description**: `run_skinning_invariant` is the shared body behind both
  `*_skinning_invariant_check` tests. Across its 145 lines it contains **zero**
  `assert`/`panic!` — it computes bone matrices, composes the palette and prints
  them. Both callers therefore pass unconditionally whenever the BSA opens, and
  pass unconditionally when it does not (they `continue`/`return`). The third,
  `oblivion_vertex_world_check` (235 lines, also assertion-free), is more
  explicit still: its own inline comment states *"neither this nor the reverse
  produce identity at bind for Oblivion body NIFs. The formula needs further
  analysis"* — an unresolved investigation shipped as a green test. Meanwhile
  the file's module doc (`:17-21`) advertises the opposite posture: *"The four
  assertions here pin bones, names, palette logic, and per-vertex bounds — all
  live regressions, no soft flags."* That sentence is true of the SSE fixture
  only; a reader takes it for the file.
- **Evidence**: `awk 'NR>=188 && NR<=332 && /assert/' byroredux/tests/skinning_e2e.rs`
  returns **0 lines**. The sibling `fnv_imports_skinned_mesh_with_resolved_bones`
  (`:150`) in the same file *does* assert (`rate >= 0.80`), which is what makes
  the naming asymmetry misleading rather than merely incomplete.
- **Impact**: M29 GPU skinning's end-to-end chain on Oblivion and FNV legacy
  `NiSkinData` content is nominally covered by two tests named
  `..._invariant_check` that report `ok` no matter what the parser, the bone
  remap or the palette composition does. Anyone auditing skinning coverage — or
  bisecting a skinning regression — reads two green invariant checks and moves
  on. The Oblivion skinned-body path is precisely where `#559`-class
  reconstruction bugs live.
- **Related**: `#638` (the SSE global-buffer gap whose guards *are* real, and
  which the module doc is describing); `/audit-oblivion` and `/audit-fnv`, both
  of which treat skinning as covered.
- **Suggested Fix**: Give `run_skinning_invariant` the assertion its name
  promises (at minimum: every bone resolves through `node_by_name`, every
  per-vertex bone index is in range, and the composed palette is finite), or
  rename the three functions to `*_dump` — matching the honestly-named
  `fnv_dump_global_skin_transform` / `fnv_vertex_skin_dump_arms1` siblings in the
  same file — and correct the module doc's "no soft flags" claim to name the one
  fixture it applies to.

---

### REG-2026-08-16-D5-03: `#2567`'s Oblivion creature-asset corpus guard is the only data-dependent corpus test in the tree that is not `#[ignore]`d

- **Severity**: LOW
- **Dimension**: Green-by-construction guard sweep
- **Location**: `byroredux/src/npc_spawn/tests.rs:773-876` (`installed_oblivion_creature_assets_resolve_from_their_records`)
- **Status**: NEW
- **Description**: The guard for `#2567` ("route placed creatures through a
  creature-shaped actor spawn", closed 2026-08-14) reads `Oblivion.esm` and every
  `*meshes.bsa` under the game directory, then asserts ≥90% skeleton and NIFZ-part
  resolution. Its assertions are real and correct. But unlike every other
  data-dependent corpus sweep in the workspace — `parse_real_nifs.rs`,
  `per_block_baselines.rs`, `block_coverage_baselines.rs`, `skinning_e2e.rs`,
  `crates/audio/src/tests.rs`, `crates/scripting/tests/pex_recognize_e2e.rs`,
  all `#[ignore]`d — it carries no `#[ignore]`, so on any machine without
  Oblivion installed it prints a skip line to stderr and is counted as
  **`ok`** by `cargo test`. Its own docstring calls this "self-skips … like the
  other corpus sweeps in this tree", which is the reverse of the tree's actual
  convention.
- **Evidence**: `byroredux/src/npc_spawn/tests.rs:773` is a bare `#[test]`; the
  early return is at `:779-786` (`esm_path not available`) with a second at
  `:806-812` (`no mesh archives`). A repo-wide scan for non-`#[ignore]`d tests
  that self-skip on absent game data returns four hits: this one and the three
  in `crates/ui/src/avm2_host.rs` (already reported by `/audit-ui`, and there the
  non-`#[ignore]` is a deliberate, documented choice).
- **Impact**: On CI and on any contributor machine without Oblivion, `#2567`'s
  only real-data guard is indistinguishable from a passing test. Low severity
  because the dev host does have the data and the assertions do run there.
- **Related**: `#2567`; `/audit-ui` (same class, UI corpus sweeps, already filed);
  `#2262` (OPEN — the tech-debt skill's `#[ignore]`-count baseline recipe, which
  counts these textually).
- **Suggested Fix**: Either add `#[ignore = "requires Oblivion game data"]` to
  match the tree convention, or — better, since the sweep is cheap when data is
  present — keep it un-ignored and have the skip path `panic!` when an explicit
  `BYROREDUX_OBLIVION_DATA` was set, so an intentional run cannot silently
  no-op.

---

## Dimension 6 — Sibling-audit verification requests · **0 new findings, 3 verifications, 1 correction**

### #2574 — independently confirmed **NO LONGER REPRODUCIBLE** (agrees with `/audit-nif`)

`/audit-nif` recommends closing it. Verified two ways:

1. **Static.** `crates/nif/tests/data/per_block_baselines/oblivion.tsv:65` now reads
   `bhkCollisionObject	8730	0` and `:76` reads `bhkPCollisionObject	54	0` —
   exactly the live split the issue reported as absent (it recorded `8784` with no
   `bhkPCollisionObject` row; `8730 + 54 = 8784`).
2. **Dynamic.** Ran the gate on this host:
   `[Oblivion] per-block baseline OK (82 types matched)` / `test per_block_baseline_oblivion ... ok`.

**Recommendation: close #2574.** Correct.

> **Correction to `/audit-nif`.** Its Issue-Tracker section attributes the
> regeneration to `c41e87d8` ("DLC-wide NIF baselines", 2026-08-13). That commit
> never touched the file — `git log -1 c41e87d8 -- crates/nif/tests/data/per_block_baselines/oblivion.tsv`
> resolves to `c1dd2e07`. The real fix is **`c1dd2e07`** (2026-08-08 13:19,
> "Fix #2558 #2559 #2560 #2561"), whose `#2559` paragraph explicitly says the
> full `--ignored` run surfaced the same `bhk*CollisionObject` staleness on
> Oblivion and Skyrim SE and regenerated those too. #2574 was filed
> 2026-08-08 03:47 and was already stale ~9½ hours later, as a side effect of a
> different issue. Worth putting in the close comment so the archaeology is right.

### #2564 — independently confirmed **STILL VALID** (agrees with `/audit-nif`)

- `crates/nif/tests/data/block_coverage_baselines/oblivion_truncations.tsv:1` still
  reads `truncating=6	parsed=8032`, followed by six file rows.
- `ROADMAP.md:543` still reads `**99.93%** (8 026 / 8 032)`; `:390` and `:1255`
  repeat the 99.93% figure.
- Live: `[Oblivion] block-count parity: 8031/8032 NIFs whole, 1 truncating`.

Stale by exactly 5, as the issue title says. **One escalation #2564 does not
carry**: because the gate is one-directional, those 5 stale rows are also 5
files the gate can no longer protect — filed above as REG-2026-08-16-D5-01, and
the two should be fixed in one commit.

### Guard-enforcing-a-bug siblings (per `/audit-oblivion` `OBL-2026-08-16-BR-01`)

**Premise verified against live code.**
`byroredux/src/cell_loader/finish_partial_tests.rs:183` defines
`finish_partial_import_oblivion_bsx_bit5_is_still_editor_marker`, and `:193`
carries the message *"Oblivion-era BSXFlags bit 5 is a genuine editor marker and
must still be skipped"*. Both production drop sites are present and share the
predicate: `byroredux/src/cell_loader/partial.rs:58-60` and
`byroredux/src/cell_loader/references/import.rs:89-90`
(`bsx & 0x20 != 0 && bsver < FALLOUT4`). `/audit-oblivion`'s finding stands
exactly as written; **not re-filed here.**

**Sibling search for the same shape** — a checked-in assertion that *pins*
behaviour another 2026-08-16 report calls wrong — came back with no second hit.
The `#[test]` bodies of all guards for the confirmed-buggy closed fixes named in
this sweep were checked (`#2928` heap budget, `#994` billboard, `#2860`/`#2868`
scale, the CHARAL roster builders, the save `FORMAT_MAJOR` tripwire); each is
either weak (already filed by its owner audit) or absent, but none *asserts* the
wrong behaviour the way the Oblivion BSX test does. The closest structural
analogue found this sweep is REG-2026-08-16-D1-02, where the wrong invariant is
stated in a doc comment rather than an assertion — a softer version of the same
failure, and one that misleads a reader just as effectively.

---

## Not Re-Filed (already covered by 2026-08-16 siblings)

Verified present in the live tree, confirmed to match the sibling report's
premise, and deliberately **not** duplicated:

| Item | Owner report |
|---|---|
| `KNOWN_MISSING_ON_DESTROY_TRAIT` allowlist (`crates/ui/src/avm2_host.rs:1374`) | `/audit-ui` |
| `avm2_host.rs` corpus sweeps not `#[ignore]`d + catalog gap "reported rather than asserted" | `/audit-ui` (lines 61, 259) |
| `#2928` heap-budget fix inverted on multi-heap hardware | `/audit-renderer` (line 169) |
| `#994` SpeedTree billboard insert on the wrong entity | `/audit-speedtree` `SPT-D3-2026-08-16-01` |
| `PLAYER_BASE_FORM_ID = 0x14` + its self-referential fixture | `/audit-fo3` `FO3-D4-01` |
| CHARAL builder fixtures derived from the roster's own strings | `/audit-character` `CHAR-2026-08-16-D6-01` |
| `#2860` + `#2868` composing into a `scale²` bug | `/audit-physics` |
| `FORMAT_MAJOR` serde-default tripwire missing the `cfg_attr` house form | `/audit-save` |
| NIF corpus baseline green while meshes are dropped post-parse | `/audit-fnv` / `/audit-oblivion` |
| `m47-triggers.sh` only exercising the interior path | `/audit-scripting` |

---

## Disproved Candidates (investigated, then falsified — recorded, not reported)

1. **"`eb5d76fe` regressed `#2955` by rewriting `stamp_actor_values`."** It
   rewrote the doc comment and dropped the `#1663` citation, but
   `effective_actor_level` is still called at `npc_spawn.rs:156` and `:698` and
   from four sites in `npc_spawn/resumable.rs`. The fix is intact; only the
   *duplicate* (REG-D1-01) is a real defect.
2. **"`derive_skyrim_actor_values` bypasses CHARAL's single-sink rule."** It
   lives in `crates/plugin/src/esm/records/actor_value_derive.rs` alongside the
   pre-existing FNV/FO3 and FO4 arms — it follows the established arrangement
   rather than creating a new bypass, and `/audit-character` already owns the
   question of whether that arrangement should be CHARAL's. Not a regression.
3. **"`p2-melee-core.sh` is green by construction."** Checked line by line. Its
   seven hardcoded `health_after=` values (42→−6 from a 50-HP base at
   `UNARMED_DAMAGE = 8.0`), the `attacks=7 hits=7 kills=1` terminal assertion and
   the `ragdoll activated (18 bodies)` check all fail loudly on drift. The
   `combat.approach` setup command bypasses navigation, but the script says so in
   its header comment; that is scope, not laundering.
4. **"`Game::mesh_archives` under-walks Oblivion (1 archive vs FO3's 6)."** The
   single-archive choice is deliberate and documented at
   `crates/nif/tests/common/mod.rs:142-151` — the DLC ship only with GOTY, and
   requiring them would make the all-or-nothing rule skip Oblivion entirely on a
   base install. Trading a partial gate for no gate would be the regression.
5. **"`crates/bgsm/tests/parse_all.rs:242` has no assertions."** It delegates to
   `run_variant`, which asserts a `MIN_SUCCESS_RATE` threshold. False positive
   from the brace-scan heuristic; same for the `scene_descriptor_reflection_tests`
   and `world_tests.rs` hits, which are `#[should_panic]` or helper-delegating.
6. **"`da10_pex_is_recognized_as_a_quest_stage_gate` (#1740) silently skips."** It
   is `#[ignore = "needs Skyrim SE game data on disk"]`, and the SKIP branch
   prints before returning. Correct by the tree's convention.

---

## Summary Table

| Issue / Contract | Title | Status | Fix Present | Guard |
|---|---|---|---|---|
| #2856 … #2860 | PHYS wave (wake, door probe, fog probe, collider scale) | PASS | Yes | present |
| #2714 … #2745 | UI / renderer wave (host drain, bindless, destroy idempotence, mesh-ID, cluster_cull) | PASS | Yes | present |
| #2923 / #2929 / #2931 / #2932 / #2933 | PERF / CON / CHAR wave | PASS | Yes | present |
| #2955 | ACBS `calcMin` level | **PARTIAL** | Yes (×2) | guards one of two copies — **REG-D1-01** |
| #1663 | `ActorValues` AVIF keying | **PARTIAL** | Yes | contract widened, consumer not updated — **REG-D1-02** |
| #1815 / #1816 / #1728 / #1740 / #1731 / #1718 | SKILL fresh-candidate wave | PASS | Yes | present, `#[ignore]`d correctly |
| #2567 | Creature actor-spawn routing | PASS | Yes | real, but not `#[ignore]`d — **REG-D5-03** |
| #2564 (OPEN) | Oblivion truncation baseline | **still valid** | n/a | gate one-directional — **REG-D5-01** |
| #2574 (OPEN) | Oblivion per-block baseline | **not reproducible** | n/a | gate green, 82 types matched — **close it** |
| Step 4 — NIFAL tier | 5 contracts | PASS | Yes | present |
| Step 4 — BSDF + GPU structs | 7 contracts | PASS | Yes | 67 tests green |
| M29 skinning chain | Oblivion + FNV legacy skin | **PARTIAL** | Yes | guard asserts nothing — **REG-D5-02** |

---

Publish with:

```
/audit-publish docs/audits/AUDIT_REGRESSION_2026-08-16.md
```
