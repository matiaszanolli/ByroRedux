# Regression Verification Audit — 2026-08-20

Verification that CLOSED bug fixes are still in place at HEAD (`bb0b92f2`), plus
the second question this sweep's meta-finding demands of every fix: **can the
guard that protects it actually fail?**

**Method — static only.** Per the suite briefing, no `cargo build` / `cargo test`
/ `cargo check` was run and the engine was not launched. Every claim below comes
from source reading, `git show` / `git log` archaeology, blob inspection
(`git cat-file`), and byte-level file analysis. Where the 2026-08-16 sibling
report cited *live gate runs*, this one deliberately does not — three of that
report's conclusions rested on runs that cannot be reproduced here, and they are
carried on that report's word, marked as such.

**Scope**
- Every issue **closed since 2026-08-16** that `/tmp/audit/issues.json` covers
  (134 in the `#2900+` band), fix-presence checked.
- The 47 of those with a traceable `Fix #N` commit, guard-checked.
- The delta's **water / streaming / volumetrics / terrain-LOD** surface — 335
  commits, near-monothematic — cross-checked against the older closed fixes whose
  code sits inside it (#1502, #2758, #2765, #2775, #2782, #2784, #2785, #2789,
  #2792, #2804, #2822, #2859, #2864, #2865, #2870, #2872, #2741, #2745).
- The unconditional **Step-4** fragile-area contracts (NIFAL tier, Disney BSDF,
  `#[repr(C)]` GPU struct pins) — plus a new SPIR-V-lockstep sweep across all
  shader sources, since the delta is shader-heavy.
- The eight **verify-and-close nominations** relayed from sibling audits.

## Executive Summary

**No closed fix was found missing.** All 47 traceably-fixed issues are present at
HEAD, all 18 water/terrain-adjacent older fixes survived the 335-commit water
rewrite intact, and all three Step-4 contract families hold. **Zero FAILs, zero
code regressions.**

The yield is in the second question, and this sweep found a new answer shape the
08-16 report did not have a category for: **a guard can also fail by being
undiscoverable.** `crates/plugin/src/esm/records/tests.rs` — 1,944 lines, 40
tests, guards citing 31 distinct issues — has carried three raw `NUL` bytes since
`09682c71` (2026-08-15). GNU `grep` therefore classifies it as a binary file and
silently skips it in every `grep -r` that lacks `-a`, which is exactly the
discovery recipe `audit-regression/SKILL.md` Step 2.3 and `_audit-common.md`'s
"grep before read" rule prescribe. This audit hit it live: `#3095`'s
146-line guard, landed 2026-08-19, read as **deleted** on three separate greps
before blob inspection proved it present. That is a false FAIL this report came
within one command of publishing.

The other four findings are the familiar shapes:
- `#3089`'s two guards pin the *helper* and rayon's own `install`, never the call
  site that is the fix — deleting `stream_pool.install(...)` leaves both green.
- `#2888`'s convergence ("both ends of WATAL pick the *nearest* overlapping water
  plane") is implemented **twice** — inline on the camera side, via a private
  helper on the physics side — and only the camera copy has a test. The exact
  shape of last sweep's `REG-D1-01`, in the delta's hottest file.
- `#3049`'s `oversized_max_log_message_bytes_is_rejected` is green by
  construction: it also pushes `max_log_bytes` past the *same* shared `||`
  ceiling and asserts only `InvalidConfig(_)`.
- `#3038` declares "every producer of a registry key MUST route through this one
  function" and two producers do not; they are correct only because their input
  is already canonical, and nothing pins that.

Plus one archaeology finding that is really about this audit's own preconditions:
**43 of the 134 issues closed in this window (32%) have no commit citing them**,
and 14 have no citation anywhere in the tree. All 14 were hand-verified as
genuinely fixed — so this is *not* a code regression — but the fix→issue link
that Steps 1–2 of this audit are built on is broken for a third of the window.

**Total findings**: 6 (0 CRITICAL, 0 HIGH, 3 MEDIUM, 3 LOW).
**Fixes verified present**: 47 / 47 traceable + 18 / 18 water-adjacent + 14 / 14
untraceable-but-verified. **FAILs**: 0. **Regressions of code**: 0.
**Regression-guard gaps**: 5.

### Verify-and-close verdicts (relayed nominations)

| Issue | State | Verdict |
|---|---|---|
| **#3070** | OPEN | **CLOSE — premise removed.** `#3036` deleted the whole `bsx & 0x20` rejection predicate; there is no bit-5 carve-out comment left to place Skyrim on the wrong side of. |
| **#2767** | CLOSED | **Verified fixed.** `crates/renderer/shaders/include/mesh_id.glsl` centralises `meshIdHasStableHistory` / `stableMeshIdsMatch`; all three consumers use it and their `.spv` recompiled in the same commit. |
| **#2888** | OPEN | **CLOSE — divergence resolved** by `4c383433`; both ends now select by absolute vertical distance. But see **REG-D3-01** — the rule is now duplicated. |
| **#2564** | OPEN | **Half-resolved.** `#3082` regenerated the baseline (now `truncating=0 parsed=8032`). The **ROADMAP row is still stale, and by 6 not 5** — `ROADMAP.md:566` still reads `99.93% (8 026 / 8 032)` plus a "remaining 6 truncated" sentence; live is 8 032 / 8 032. |
| **#3082** | CLOSED | **Fixed** (`parsed` line + `parsed >= baseline_parsed` assertion). Residual noted below — the set-equality half was not implemented. |
| **#3069** | CLOSED | **`0x04` half fixed** and guarded. The `0x02` half is real and is **owned by `/audit-skyrim` `SKY-2026-08-20-D3-01`** (which has the measured 192/279 figure) — not re-filed here. Archaeology note below. |
| **#2424 / #2425 / #2597** | OPEN | **All three premises still valid at HEAD** — `skin.rs:226` `(100..130)`, `sequence.rs:305` `(24..=28)` directly under `:298`'s `ANIM_NOTES_THRESHOLD`, `shader.rs:1057` `(130..=139)`. Note-and-skip. |
| #3102 | CLOSED | Correct: `finish_partial_import_oblivion_bsx_bit5_is_still_editor_marker` is gone, replaced by `..._bsx_bit5_keeps_real_geometry_sibling`. Resolved as a **side effect of `#3036`**; no commit cites #3102. |

## Dimension Roll-Up (every dimension, including clean ones)

| # | Dimension | Findings |
|---|---|---|
| 1 | Closed-issue discovery & fix presence (SKILL Steps 1–3) | **2** (1 MEDIUM, 1 LOW) |
| 2 | Guard existence & liveness for delta-closed fixes | **2** (1 MEDIUM, 1 LOW) |
| 3 | Water / streaming / terrain-LOD churn surface | **1** (1 MEDIUM) |
| 4 | Step 4 — NIFAL canonical-translation tier | **0** (clean) |
| 5 | Step 4 — Disney BSDF, GPU struct pins, SPIR-V lockstep | **0** (clean) |
| 6 | Carry-forward of the 2026-08-16 report's own findings | **1** (1 LOW) |

---

## Dimension 1 — Closed-issue discovery & fix presence · **2 findings**

### Fix-presence sample — 47 / 47 PASS

Every issue closed in the window that has a `Fix #N` commit. Located by
`git log --grep`, then the cited symbol re-read at HEAD (never inferred from the
commit log). Guard column: *inline* = `#[cfg(test)]` in the fixed file,
*sibling* = `*_tests.rs`, *none* = no test added.

| Issue | Fix commit | Fix site (live, verified) | Guard |
|---|---|---|---|
| #2700 | `b87544f0` | `byroredux/src/asset_provider/material.rs` | sibling (asserts rewritten) |
| #2790 / #2797 / #2798 / #2799 / #2800 / #2805 / #2808 | `ee07429f` `071e7281` `85d49410` `5539177b` `529c6b7b` `98c68cb7` `0ba75e42` | comment/doc corrections — **verified comment-only**, no code delta | n/a (doc) |
| #2816 | `e3cf194c` | `byroredux/src/systems/weather.rs` | inline ×3 |
| #2827 | `a0cf37ac` | `scene/world_setup.rs` + `systems/weather.rs` | inline |
| #2862 | `25048671` | `nif/import/collision/{mod,shape}.rs`, `physics/convert.rs` | inline ×8 |
| #2908 | `6381a828` | `plugin/esm/records/grup_walker.rs` | sibling ×3 *(in the grep-invisible file — see REG-D1-01)* |
| #2936 / #2937 / #2939 / #2941 / #2944 / #2947 | `86bfee5e` `8bc7e053` `b9767e43` `9f7f3e11` `010fc6d2` `3a39ca47` | `core/character/{derived,fallout,resistance,profile,components}.rs`, `save/validate.rs` | inline ×10 |
| #2956 | `71766c44` | `npc_spawn.rs`, `plugin/equip.rs`, `actor_value_derive.rs` | sibling ×6 |
| #2962 / #2963 / #2964 / #2966 / #2967 | `5c2d188a` `7a97b423` `c127e62b` `0a87ca54` `38617c58` | `ui/{avm2_host,host,catalog,navigator,player}.rs` | inline + sibling |
| #2974 / #2976 | `150ee25a` `25c46484` | audit skill recipe; `combat.rs` + `p2-melee-core.sh` | inline; smoke `fail`-gated |
| #2999 | `7218f543` | `nif/import/material/slot_role.rs` | sibling ×3 |
| #3013 | `0eaea646` | `asset_provider/animation.rs` | inline (see note) |
| #3015 / #3016 | `b766327d` `3be6c9f1` | `cell_loader/references/synth_child.rs` | sibling ×2 |
| #3017 | `585fd872` | `pex/src/lib.rs` + `examples/pex_corpus_smoke.rs` | inline |
| #3038 | `bff2d5a3` | `cell_loader/nif_import_registry.rs` (`canonical_model_path_key`) | inline ×5 — see **REG-D1-02** |
| #3048 / #3049 | `4e57fcd4` `9725baeb` | `facegen/eval.rs`; `mod-runtime/limits.rs` | inline ×5 each — see **REG-D2-02** |
| #3058 / #3059 / #3060 | `0f786f50` | `byroredux/src/interaction.rs` | inline ×2 |
| #3081 | `17b94d2e` | duplicate deleted; `inventory.rs:16` imports `npc_spawn::effective_actor_level` | existing guard now reaches the single copy |
| #3082 | `17cb417d` | `nif/tests/block_coverage_baselines.rs` + regenerated `.tsv` | is itself the gate |
| #3083 | `59a5ee6f` | `byroredux/tests/skinning_e2e.rs:348-360` | now asserts, **plus** a "no bone was checked" vacuity guard |
| #3089 | `060718cb` | `streaming.rs` (`build_stream_parse_pool`, `stream_pool.install`) | sibling ×2 — see **REG-D2-01** |
| #3092 / #3095 / #3096 / #3097 / #3098 | `08434727` `f4e731f6` `219e876c` `8b72dbdc` `1e9723ab` | `combat.rs`, `plugin/esm/records/tests.rs`, `esm/records/items.rs`, `nif/anim/entry.rs`, `esm/cell/mod.rs` | sibling ×2–5 |

Three prior-report follow-ups confirmed closed by this delta:
- **`#3081`** — `grep -rn "effective_actor_level"` now returns exactly **one**
  definition (`npc_spawn.rs:143`, `pub(crate)`) and `inventory.rs:16` imports it.
  The divergent `max(0)`/`max(1)` clamp is gone with the duplicate.
- **`#3083`** — `run_skinning_invariant` asserts at `:348` and `:357`, the latter
  being a *vacuity* guard ("no skinned bone was checked — the invariant assertion
  above never ran"). That is a better fix than the one suggested.
- **`#3082`** — see the residual note under Dimension 6.

Note on `#3013`: the fix is a `log::warn!` only; the drop behaviour it sits next
to is pre-existing. Its guard asserts the drop, and its own docstring says the
warn "is a diagnostic, not a return value, so it isn't captured by this test."
Deleting the warn leaves the test green — but the test is honest about it and the
impact is diagnostic-only. Recorded as PARTIAL, **not filed**.

---

### REG-2026-08-20-D1-01: `crates/plugin/src/esm/records/tests.rs` is a binary file to `grep` — 40 regression guards are invisible to the discovery recipe every audit skill prescribes

- **Severity**: MEDIUM
- **Dimension**: Closed-issue discovery & fix presence
- **Location**: `crates/plugin/src/esm/records/tests.rs:1678`, `:1686`, `:1691` (three raw `NUL` bytes); introduced by `09682c71` (2026-08-15, "feat: Implement inventory management and UI integration")
- **Status**: NEW
- **Description**: The file contains three literal `0x00` bytes inside byte-string
  literals — `b"Long Barrel<NUL>"`, `b"Desk Fan<NUL>"`, `b"Overdue Book<NUL>"` —
  where the source almost certainly intended the two-character escape `\0`. The
  bytes are valid Rust and compile fine, so nothing in the build complains. But
  GNU `grep` treats any file containing a `NUL` as **binary** and, without `-a`,
  emits `Binary file … matches` at best and (with `--binary-files=without-match`,
  the effective default in this environment) **silently skips it**. The file is
  1,944 lines and holds **40 `#[test]` functions** whose comments cite **31
  distinct issue numbers** — `#442 #443 #445 #448 #458 #519 #624 #629 #630 #631
  #634 #808 #809 #810 #817 #896 #966 #969 #989 #1277 #1304 #1538 #1568 #1666
  #1773 #2081 #2908 #2986 #3093 #3094 #3095`. Two of those guards (`#2908`,
  2026-08-18; `#3095`, 2026-08-19) landed **after** the file went binary, so they
  have never been visible to a plain grep at any point in their life.
- **Evidence**: This audit hit it live and nearly published a false FAIL.
  ```
  $ grep -rn "real_ruleset_falsifiability" --include='*.rs' .      # → nothing
  $ grep -c falsifiability crates/plugin/src/esm/records/tests.rs  # → rc=1, no output
  $ grep -an "mod real_ruleset" crates/plugin/src/esm/records/tests.rs
  56:mod real_ruleset_falsifiability {                              # ← it is there
  $ file crates/plugin/src/esm/records/tests.rs
  crates/plugin/src/esm/records/tests.rs: data
  ```
  Blob-level confirmation that the guard is present and was never deleted:
  `git cat-file -p c9933353 | wc -l` = 1801 → `0decee23` (post-`#3095`) = 1947 →
  `7a304403` (HEAD) = 1943, the −4 being a `cargo fmt` line-join in `73896726`.
  Per-commit NUL count crosses 0 → 3 at `09682c71`.
  A repo-wide scan finds **exactly one** such file:
  ```
     3 NUL    1944 lines  ./crates/plugin/src/esm/records/tests.rs
  TOTAL FILES: 1
  ```
  `rg` and `git grep` **do** see it (both return 2 hits); only plain `grep` does not.
- **Impact**: `audit-regression/SKILL.md` Step 2.3 says *"`grep -rn "<N>" crates/
  byroredux/ --include='*.rs'` — many tests cite the issue number"*, and
  `_audit-common.md`'s context rule is *"grep before read"*. Against this file
  both return empty, so the honest conclusion a following-the-recipe auditor
  reaches is **"fix present, no guard"** — a PARTIAL where the truth is PASS — or,
  worse, **"guard deleted"** when the fix commit's `--stat` says a test file grew
  by 146 lines and the symbol cannot be found. The blast radius is every audit in
  this suite that greps for a symbol, not just this one; `/audit-esm` and
  `/audit-character` both own material in this file. It is also a live hazard for
  `/audit-publish` and for the session-close symbol-drift gate.
- **Related**: `#2908`, `#3095`, `#2986`, `#3093`, `#3094` (guards living inside
  the blind spot); `09682c71` (introduced it); the 2026-08-16 sibling's
  `REG-D5-02`/`REG-D5-03` (same family: a guard that exists but cannot do its job).
- **Suggested Fix**: Replace the three raw `NUL` bytes with the `\0` escape —
  `b"Long Barrel\0"` etc. The compiled byte strings are byte-identical, so no test
  changes meaning. Then add a cheap CI tripwire (or a `_audit-validate.sh` clause)
  that fails on any tracked `*.rs` / `*.md` / shader source containing a `NUL`, so
  the next one is caught at the commit that introduces it rather than five days
  and two audit sweeps later.

---

### REG-2026-08-20-D1-02: `#3038` declares a single-normaliser invariant that two other registry-key producers violate — correct today only by accident of input

- **Severity**: LOW
- **Dimension**: Closed-issue discovery & fix presence
- **Location**: `byroredux/src/cell_loader/nif_import_registry.rs:33-49` (the invariant + `canonical_model_path_key`), `byroredux/src/streaming_helpers.rs:366`, `byroredux/src/cell_loader/partial.rs:25` (the two violators), `nif_import_registry.rs:85-92` (the forward-slash test)
- **Status**: NEW
- **Description**: `#3038`'s fix doc is explicit: *"every producer of a registry
  key MUST route through this one function rather than building the key inline."*
  Two `NifImportRegistry` key producers still build the key inline with a bare
  `model_path.to_ascii_lowercase()` — the negative-cache insert in
  `finish_streaming_import` and the positive-cache path in `finish_partial_import`.
  Both are correct **today** only because the only caller chain feeds them a
  string `pre_parse_cell` already canonicalised at `streaming.rs:1243`; the
  lowercase is a no-op on an already-lowercase key and the `meshes\` prefix rides
  along. Nothing asserts that precondition, and neither function's signature
  expresses it. Separately, `canonical_model_path_key` deliberately does **not**
  unify the two separator forms — `Meshes\Clutter\x.nif` → `meshes\clutter\x.nif`
  but `meshes/clutter/x.nif` → `meshes/clutter/x.nif` — and
  `does_not_double_prefix_forward_slash_form` is a checked-in test that **pins
  that divergence as correct**, in the very module whose reason to exist is "the
  same asset must not land under two keys."
- **Evidence**:
  ```rust
  // streaming_helpers.rs:366 — negative-cache producer, inline normalisation
  let cache_key = model_path.to_ascii_lowercase();
  let mut reg = world.resource_mut::<cell_loader::NifImportRegistry>();
  reg.insert(cache_key, None)

  // partial.rs:25 — positive-cache producer, inline normalisation
  let cache_key = model_path.to_ascii_lowercase();
  ```
  Neither file imports `canonical_model_path_key`. The five `#3038` tests all
  exercise the helper directly; none reaches either call site, so reverting
  `streaming.rs:1243` to a bare lowercase — the precise pre-fix state — leaves all
  five green.
- **Impact**: Low today (latent, not live). The reachable failure is a future
  producer, or a change to what `payload.parsed` carries, silently re-splitting
  the cache key space — which is `#3038`'s original symptom (assets parsed and
  imported twice, cache-hit telemetry undercounting). `precombined.rs:344` is a
  third inline producer but is genuinely safe: `precombine_oc_nif_path` synthesises
  a lowercase-hex `meshes\precombined\…` path in its own namespace that cannot
  collide with an authored `model_path`.
- **Related**: `#3038`; `#862` / `#864` (the cache-key snapshot the invariant
  protects); the global instruction *"always prioritize improving existing code
  rather than duplicating logic."*
- **Suggested Fix**: Route both violators through `canonical_model_path_key`
  (it is documented idempotent, so this is a safe no-op today and a real guard
  tomorrow), and either normalise `/` → `\` inside the helper or rewrite
  `does_not_double_prefix_forward_slash_form` to assert *convergence* rather than
  pinning the split.

---

## Dimension 2 — Guard existence & liveness for delta-closed fixes · **2 findings**

Every one of the 47 traceable fixes landed with a guard except `#2700` (asserts
rewritten in existing tests) and `#3081` (a deletion; the pre-existing guard now
reaches the sole surviving copy). A mechanical sweep for the crudest form of the
sweep theme found **zero** hits: of 580 functions added across the 270 changed
`.rs` files in the delta, **no** newly-added `#[test]` has an assertion-free body.
The two findings below are the subtler forms.

### REG-2026-08-20-D2-01: `#3089`'s two guards pin the pool constructor and rayon's own `install` — never `pre_parse_cell`'s use of it, which is the fix

- **Severity**: MEDIUM
- **Dimension**: Guard existence & liveness
- **Location**: `byroredux/src/streaming_tests.rs` (`stream_parse_pool_leaves_headroom_for_the_frame_pool`, `stream_parse_pool_runs_tasks_on_its_own_dedicated_threads`); fix site `byroredux/src/streaming.rs:1318`
- **Status**: NEW
- **Description**: `#3089` (MEDIUM, `sync`/`performance`, closed 2026-08-19 by
  `060718cb`) is about **contention**: the cell-stream worker's Phase 2 fan-out
  was dispatching into rayon's *global* pool, which the ECS scheduler's
  `Stage::Update` parallel batch also uses. The fix is one line —
  `stream_pool.install(|| extracted.into_par_iter()…)` at `streaming.rs:1318`.
  Neither guard touches it. The first constructs `build_stream_parse_pool()` and
  asserts its thread count; the second constructs a pool and asserts that
  `pool.install(…)` runs on a `byro-stream-parse-*` thread — which is a property
  of `rayon::ThreadPool`, not of this repo's code. Reverting `streaming.rs:1318`
  to the pre-fix `extracted.into_par_iter().map(parse_one_nif).collect()` leaves
  **both tests green** and reinstates the exact contention the issue describes.
- **Evidence**: `grep -an "pre_parse_cell\|stream_pool" byroredux/src/streaming_tests.rs`
  returns only `pre_parse_cell_panic_safe` (a `#854` guard, unrelated) — no test
  in the workspace names `stream_pool` or reaches `pre_parse_cell`'s Phase 2. The
  production `stream_pool` mentions are `streaming.rs:1051` (construction),
  `:1071` / `:1187` (threading it through) and `:1318` (the only `install`).
- **Impact**: The regression this restores is invisible to `cargo test` by
  construction (a thread-pool routing choice with no functional output), so the
  guard is the *only* possible detector, and it detects nothing. `#3089`'s own
  framing — *"defeating the whole point of running cell parsing on its own thread
  in the first place"* — is the failure mode that silently returns. The delta's
  streaming surface is one of its two hottest, which raises the odds.
- **Related**: `#3089` (`CONC-2026-08-16-01`); `#877` / `#1262` (the two-phase
  pre-parse split the pool sits inside); `#862` (the cache snapshot in the same
  function).
- **Suggested Fix**: Give `pre_parse_cell` an observable: have `parse_one_nif`
  (or a thin wrapper) record `std::thread::current().name()` into the existing
  `StreamingWorkerTimings`, then assert in `streaming_tests.rs` that a
  fan-out above `PRE_PARSE_RAYON_MIN` reports only `byro-stream-parse-*` names.
  That reaches the actual `install` and fails on its removal.

---

### REG-2026-08-20-D2-02: `#3049`'s `max_log_message_bytes` ceiling test is satisfied by the sibling ceiling in the same `||` chain

- **Severity**: LOW
- **Dimension**: Guard existence & liveness
- **Location**: `crates/mod-runtime/src/limits.rs:167-169` (the shared `||` chain), `:330-341` (`oversized_max_log_message_bytes_is_rejected`)
- **Status**: NEW
- **Description**: `#3049` added ceilings to `SandboxConfig::validate()` and, to
  its credit, a table-driven completeness test plus two extra tests for the log
  fields the table omits. One of those two cannot fail for the field it names.
  All three log ceilings share a single `if a > MAX || b > MAX || c > MAX` arm
  returning one `InvalidConfig`; the test sets `max_log_message_bytes =
  MAX_SANE_LIMIT + 1` **and** `max_log_bytes = MAX_SANE_LIMIT + 2`, so the
  `max_log_bytes` clause alone rejects the config. The assertion is
  `matches!(…, Err(SandboxError::InvalidConfig(_)))` — a wildcard that cannot
  distinguish which clause fired. Delete the `self.max_log_message_bytes >
  MAX_SANE_LIMIT` term and the test stays green.
- **Evidence**: The test's own docstring explains why the second field was raised
  — *"so the pre-existing cross-check doesn't fire for an unrelated reason and
  mask which guard actually caught it"* — which fixes one masking and introduces
  another. The `max_log_message_bytes > max_log_bytes` cross-check at `:175` is
  indeed sidestepped (`MAX+1 < MAX+2`), but only by pushing the sibling over the
  very ceiling under test.
  ```rust
  // :167 — one arm, three fields, one error
  if self.max_log_entries > MAX_SANE_LIMIT
      || self.max_log_message_bytes > MAX_SANE_LIMIT
      || self.max_log_bytes > MAX_SANE_LIMIT
  { return Err(SandboxError::InvalidConfig("a log limit exceeds the sane ceiling")); }
  ```
- **Impact**: Low — `crates/mod-runtime` has no engine consumer yet and these are
  explicitly a *sanity backstop*, not derived physical limits. Filed because it is
  a clean, verified instance of the sweep's theme in code closed two days ago, and
  because the sibling `oversized_max_log_bytes_is_rejected` shows the correct
  construction one function above it. The other nine ceilings are properly guarded
  and `oversized_wasm_stack_is_rejected` / `wasm_stack_at_the_ceiling_is_accepted`
  correctly bracket the `>` vs `>=` boundary.
- **Related**: `#3049` (`SAFE-2026-08-16-02`); `#2543` (`MAX_SANE_SHAPE_EXTENT`,
  the posture this fix cites as precedent).
- **Suggested Fix**: Drop `max_log_bytes` from the fixture (leave it at its 1 MiB
  default — `MAX_SANE_LIMIT + 1` for the message already exceeds it, so assert on
  the cross-check *or* split the `||` chain into three arms with distinct messages
  and match the message). Splitting the arm is the smaller change and makes all
  three fields independently falsifiable.

---

## Dimension 3 — Water / streaming / terrain-LOD churn surface · **1 finding**

The delta's stated risk was that near-monothematic water work silently reverts
unrelated fixes. **It did not.** Every closed fix whose code sits in or beside
that surface is intact at HEAD:

| Issue | Contract | Live state |
|---|---|---|
| #2782 | water.frag early depth test | `water.frag:13` `layout(early_fragment_tests) in;` |
| #2784 | uv01 upper-edge off-by-one | `water.frag:1189-1202` — rejects on the **integer pixel**, comment intact |
| #2804 | dead `SHORELINE_RAY_MAX` + no-op `reflColor` mix | `water.frag:192`, `:918-922` — both stay **removed**, with the explanatory negatives in place |
| #1502 | water UV precision bound | `water.frag:242`, `:552` (rebased #1997) |
| #2789 | caustic deposit normalisation | `renderer/vulkan/water.rs:1518` |
| #2775 | caustic occluded-light budget | `renderer/vulkan/caustic.rs:1418` |
| #2870 | waterline-band wet→dry restore | `physics/water.rs:734-745` — damping + `reset_forces` on the band-exit branch, mirrored on the full-exit branch at `:753-759`. `authored_lin/ang` still sourced from `RigidBodyData`, not the live body, so it cannot latch water damping |
| #2872 | WATR flow-speed units | `byroredux/src/env_translate.rs:954` |
| #2865 | `pull_dynamic` Transform dirty bit | `physics/sync.rs:1058-1069` — sleeping-and-matching bodies `continue` before `updates.push` |
| #2864 | double QBVH rebuild on streaming frames | `physics/world.rs:160`, `physics/sync.rs:883` |
| #2859 | fog ground probe self-hit | `physics/world.rs:645`, `:1807` (caster exclusion) |
| #2822 | LAND `bitangent_sign = -1` | `renderer/vertex.rs:177` (`new_terrain`, the **sole** terrain vertex constructor) + `cell_loader/water.rs:142`, `:719` — no `+1` site survives |
| #2758 | distant-LOD GPU-handle leak on `entities.is_empty()` | `object_lod.rs:335-343` and `placement_lod.rs:596-599` — both early returns now call `release_lod_gpu_resources` |
| #2741 / #2745 / #2765 / #2785 / #2792 | destroy idempotence, mesh-ID write mask, caustic-source flag, `fog_near` reader, submersion clear | all present (`taa.rs:1075`, `pipeline.rs:590`, `systems/water.rs:159`+`:469-472`, guard at `systems/water.rs:820`) |

### REG-2026-08-20-D3-01: WATAL's "nearest overlapping surface" rule is implemented twice — and only the camera copy has a test

- **Severity**: MEDIUM
- **Dimension**: Water / streaming churn surface
- **Location**: `crates/physics/src/water.rs:234-237` + `:660-663` (helper + physics use), `byroredux/src/systems/water.rs:226-236` (the inline camera copy), `byroredux/src/systems/water.rs` `overlapping_water_volumes_choose_nearest_surface` (the sole guard)
- **Status**: NEW (resolves the premise of the OPEN **#2888**, and re-opens it in a new form)
- **Description**: `#2888` (`PHYS-D6-05`, OPEN) says *"the two ends of WATAL
  disagree on which overlapping water plane wins — physics takes the first match,
  the camera the nearest."* `4c383433` fixed it on both ends **in the same
  commit** — but with two independent implementations. The physics side got a
  named private helper, `fn nearest_surface_distance(surface_y, reference_y) ->
  (surface_y - reference_y).abs()`, consumed by a `min_by`. The camera side got
  the same rule written inline as `depth.abs() < prev_depth.abs()` inside
  `submersion_system`'s selection loop. The helper is private to
  `crates/physics` (`fn`, not `pub`), so `byroredux` could not reuse it even
  deliberately. `4c383433` added exactly one test —
  `overlapping_water_volumes_choose_nearest_surface`, in
  `byroredux/src/systems/water.rs`, exercising the **camera** side. The 17 tests
  in `crates/physics/src/water.rs` contain no `nearest` / `overlapping` case, so
  the physics half of the convergence has **no guard at all**.
- **Evidence**:
  ```
  $ grep -rn "nearest_surface_distance" --include='*.rs' .
  crates/physics/src/water.rs:235:fn nearest_surface_distance(surface_y: f32, reference_y: f32) -> f32 {
  crates/physics/src/water.rs:661:  nearest_surface_distance(a.1, center_y)
  crates/physics/src/water.rs:662:      .total_cmp(&nearest_surface_distance(b.1, center_y))
  ```
  — one definition, both uses inside the same crate; nothing in `byroredux`.
  The camera copy (`byroredux/src/systems/water.rs:231-235`):
  ```rust
  match best {
      None => best = Some(candidate),
      Some((prev_depth, _)) if depth.abs() < prev_depth.abs() => best = Some(candidate),
      _ => {}
  }
  ```
  Test census of `crates/physics/src/water.rs`: 17 `#[test]`, none covering
  surface selection.
- **Impact**: This is the exact shape of the last sweep's `REG-D1-01` (`#3081`,
  a fix copy-pasted into a second unguarded site) — filed, fixed, and now
  recurring in the delta's single hottest file (`crates/physics/src/water.rs`, 17
  commits since 08-16). The two copies already differ in reference point (camera
  position vs body-AABB centre, correct per consumer) which is precisely the kind
  of local divergence that makes the *next* edit touch one and not the other.
  Reverting either half to a signed comparison restores `#2888`'s disagreement,
  and only one revert is detectable. WATAL's own design premise is a single
  canonical rule shared by its render and physics ends.
- **Related**: **#2888** (OPEN — its stated divergence is *resolved*; recommend
  closing it and filing this in its place); `4c383433`; `#3081` (the prior
  instance of this shape); `#2887` (OPEN — the sibling question of which
  reference point `WaterContact::depth` should use); `docs/engine/watal.md`.
- **Suggested Fix**: Make `nearest_surface_distance` `pub` in `crates/physics`
  (it is already the crate `byroredux` imports `authored_wave_height_with_weather`
  and `weather_wave_adjustment` from in this same function) and call it from
  `submersion_system`, deleting the inline comparison. Then add the physics-side
  twin of `overlapping_water_volumes_choose_nearest_surface` — two stacked
  `WaterVolume`s, one body between them — so both ends fail loudly on a revert.

---

## Dimension 4 — Step 4: NIFAL canonical-translation tier · **0 findings (clean)**

| Contract | Live state at HEAD |
|---|---|
| Single `ImportedMesh → Material` boundary | `byroredux/src/material_translate.rs:268` is the only production `fn translate_material`; the three other hits are its own tests |
| `Material::metalness` / `roughness` stay plain `f32` | `crates/core/src/ecs/components/material.rs:24-25` — plain `f32`, no `Option<f32>` reintroduced. The nine `Option<f32>` fields at `:403-411` are unrelated FaceTint/POM/multi-layer slots, all pre-existing |
| Typed particle emitters | `NiPSysBoxEmitter` / `CylinderEmitter` / `SphereEmitter` / `MeshEmitter` (`blocks/mod.rs:1099-1103`), `NiPSysEmitterCtlrData` (`:1043`), `NiPSysGrowFadeModifier` (`:1067`), `NiPSysEmitterCtlr` (`:1135`) — all still typed arms; no regression to opaque `NiPSysBlock` |
| Emitter param plumbing | `extract_emitter_params` / `extract_emitter_rate` at `nif/import/walk/mod.rs:766` / `:865`; consumer `apply_emitter_params` at `byroredux/src/systems/particle.rs:29` with three guards |
| Collision shape coverage | `BhkMultiSphereShape` (`import/collision/shape.rs:110`) and `BhkConvexListShape` (`:235`) both still resolve to a `CollisionShape`, with fixtures at `:1132`, `:1160`, `:1177` |

---

## Dimension 5 — Step 4: Disney BSDF, GPU struct pins, SPIR-V lockstep · **0 findings (clean)**

| Contract | Live state at HEAD |
|---|---|
| Disney/Burley lobe lives in the include | `crates/renderer/shaders/include/pbr.glsl`, 498 lines |
| MIT attribution travels with the code | `triangle.frag:19-30` — GLSL-PathTracer / Asif Ali MIT notice + Burley 2012 cite intact |
| `resRadiance[]` **stays retired** (verify gone, not intact) | Two workspace mentions, both explanatory comments (`include/lighting.glsl:85`, `triangle.frag:2637`). No array declaration, no re-added G-buffer reservoir attachment |
| WRS recomputes via `shadowableLightRadiance` | Declared `include/lighting.glsl:92`, live call sites in `triangle.frag` |
| `GpuInstance` = 128 B + field offsets | `gpu_instance_is_128_bytes_std430_compatible` + `gpu_instance_field_offsets_match_shader_contract` (15 `offset_of!` pins) |
| `GpuCamera` = **352 B** | Grew 336 → 352 in this delta (`8e7582ed`, `render_debug` uvec4 appended). Pin updated in lockstep: `gpu_camera_is_352_bytes` |
| `GpuMaterial` = 348 B | `gpu_material_size_is_348_bytes` (`renderer/vulkan/material.rs:1382`) |
| `CameraUBO` Rust↔`.spv` lockstep | `reflect.rs:541` `camera_ubo_size_matches_gpu_camera_in_every_shader` derives `expected` from `size_of::<GpuCamera>()` and reflects all **6** declaring `.spv` blobs. **This is the model of a guard that cannot be satisfied by construction** — the compiled binary is the other side of the comparison. `8e7582ed` recompiled all six `.spv` in the same commit |

**New this sweep — SPIR-V freshness sweep across all shader sources.** Because the
delta is shader-heavy, every `.glsl`/`.vert`/`.frag`/`.comp` was compared against
its committed `.spv`. Five sources have a source commit newer than their `.spv`
commit; **all five are benign** and were verified by diffing out comments:
`bloom_upsample.comp` (`#2805`), `triangle.frag` (`#2808`, `#2798`), `ui.vert`
(`#2797`), `volumetrics_inject.comp` (`#2242`) are **comment-only** changes
(non-comment diff is empty in every case), and `skin_palette.comp`'s `#1758`
`SKIN_WORKGROUP_SIZE` substitution resolves to the same literal `64`, so the
recompile (filesystem mtime Jun 26, same day as the commit) produced an identical
blob. **No stale `.spv` at HEAD.**

---

## Dimension 6 — Carry-forward of the 2026-08-16 report's own findings · **1 finding**

| 08-16 finding | Status at HEAD |
|---|---|
| `REG-D1-01` — `#2955` copy-pasted into `inventory.rs` | **RESOLVED** — filed as `#3081`, fixed by `17b94d2e`; single `pub(crate)` definition, `inventory.rs` imports it |
| `REG-D1-02` — `ActorValues` grew a second key space, `#1663`'s consumer not told | **Functionally RESOLVED** by `#2987` — `SKYRIM_HEALTH_ACTOR_VALUE` is gone, `health_actor_value_key` returns the real `AVHealth` FormID (`0x3E8`), pinned by `parse_real_esm.rs:191`. **Doc half survives** — see REG-D6-01 |
| `REG-D5-01` — Oblivion truncation gate one-directional, `parsed=` never read | **RESOLVED** — filed as `#3082`, fixed by `17cb417d`. Residual: the *set-equality* half was not implemented (the comparison at `block_coverage_baselines.rs:177-180` is still `truncating − baseline`, subset-only). It is currently vacuously safe because the regenerated baseline is empty (`truncating=0`), so subset ≡ equality; the moment a row is ever baselined again, that row becomes permanently unguarded exactly as before. Worth one line in a follow-up, **not re-filed** |
| `REG-D5-02` — `run_skinning_invariant` asserts nothing | **RESOLVED** — filed as `#3083`, fixed by `59a5ee6f`; asserts at `:348` **plus** a vacuity guard at `:357` |
| `REG-D5-03` — `#2567`'s Oblivion corpus guard not `#[ignore]`d | Unchanged since 2026-08-16 — `byroredux/src/npc_spawn/tests.rs:773` is still a bare `#[test]`. No issue was filed for it. Carried, not re-filed |
| 08-16 live-gate results (per-block / block-coverage / `gpu_`) | Not re-run — this audit is static-only. Carried on the 08-16 report's word |

### REG-2026-08-20-D6-01: `#2987` removed the Skyrim engine-enum key space, but `ActorValues`' contract doc still declares it

- **Severity**: LOW
- **Dimension**: Carry-forward
- **Location**: `crates/core/src/ecs/components/actor_values.rs:13-19` (the false contract), `byroredux/src/commands_tests.rs:563` (a fixture still using the retired key)
- **Status**: Residual of `REG-2026-08-16-D1-02` (this audit's own prior finding), narrowed
- **Description**: `#2987` (`ESM-2026-08-16-D7-02`, HIGH, closed 2026-08-17)
  established that the premise behind the second key space was false — *"Vanilla
  `Skyrim.esm` contains 149 `AVIF` records and one of them is `AVHealth`, FormID
  `0x000003E8`"* — and removed `SKYRIM_HEALTH_ACTOR_VALUE`. `health_actor_value_key`
  now returns the real remapped AVIF FormID for Skyrim, so `ActorValues` is back
  to a single key space and `crates/scripting/src/condition.rs`'s `GetActorValue`
  arm (whose doc comment asserts exactly that) is correct again. The module doc
  that *created* the confusion was not updated: `actor_values.rs:15-18` still
  reads *"Built-in TES5 actor values use Skyrim's engine enum index (for example
  Health is 24), because vanilla does not author `AVIF` records for them."* That
  sentence is now false in both clauses, and it sits at the top of the file that
  defines the canonical component's key contract.
- **Evidence**: `grep -rn "engine enum" --include='*.rs' crates/ byroredux/`
  returns **one** hit — the doc comment. No production code produces enum-index
  keys any more. `crates/plugin/tests/parse_real_esm.rs:191` asserts
  `health_actor_value_key() == Some(0x0000_03E8)`. Meanwhile
  `byroredux/src/commands_tests.rs:563` still constructs
  `ActorVitals { health: 24 }` — the retired enum index — as a fixture.
- **Impact**: Doc-only, hence LOW. Recorded because this exact sentence is what
  produced a MEDIUM finding in the previous sweep (`REG-2026-08-16-D1-02`), and
  leaving it in place will produce the same false finding again next sweep: an
  auditor reading the contract doc will conclude the two-space hazard is live when
  it is not. A stale invariant in a doc comment misleads as effectively as a stale
  assertion in a test.
- **Related**: `#2987`, `#2986` (the `AV`-prefix root cause), `#1663` (the original
  single-space contract), `REG-2026-08-16-D1-02` (the finding this closes out).
- **Suggested Fix**: Rewrite `actor_values.rs:13-19` to state the restored
  single-space rule, cite `#2987` for why the engine-enum workaround was withdrawn,
  and describe `ActorVitals` as the per-game *Health key carrier* rather than as a
  bridge between two key spaces. Change the `commands_tests.rs:563` fixture to a
  plausible AVIF FormID so no reader mistakes `24` for a live convention.

---

## Archaeology finding — the fix→issue link is broken for a third of the window

Not filed as a numbered finding (it is a process observation, not a code defect),
but it is the precondition this entire audit runs on and it degraded sharply in
this delta.

Of the **134** issues closed since 2026-08-16 in `/tmp/audit/issues.json`,
**43 (32%) have no commit whose message cites them** (`git log --grep="#N"`
empty), and **14 have no citation anywhere in the tree** — not in a commit, not in
a code comment, not in a doc. Every one of those 14 was hand-verified at HEAD
this sweep and **all 14 are genuinely fixed**:

| Issue | Verified fixed at | Closing commit (cites nothing) |
|---|---|---|
| #2930 | `acceleration/blas_static.rs:566` `record_scratch_serialize_barrier` | — |
| #2938 | `core/character/derived.rs:74` `debug_assert!` | — |
| #2987 | `SKYRIM_HEALTH_ACTOR_VALUE` removed; `parse_real_esm.rs:191` pins `0x3E8` | — |
| #2988 | 5 / 5 VMAD sites now `parse_with_remap` | — |
| #2993 | `items.rs:404-408` — FO4 arm reads `value, weight, health` | — |
| #2994 | `items.rs:449` — `b"FNAM"` FO4 arm added | — |
| #2995 | `items.rs:521` — FO4 `AMMO DATA` 8-byte arm added | — |
| #3000 / #3001 / #3002 / #3003 / #3007 | `23068af0` "fix(smoke): make playable gates truthful" | `23068af0` |
| #3023 / #3024 | `save/validate.rs:207-230` `EquippedWeapon` inventory-index + FormID cross-check | — |
| #3026 | `InputAction::{Quicksave, Quickload}` (`interaction.rs:63-64`) | — |

The cost is borne entirely by this audit type. `audit-regression/SKILL.md` Step 2.1
is *"`git log --oneline --grep="#<N>"`"*; for a third of the window that returns
nothing, and Step 2.3's grep fallback returns nothing for 14 of them. The result
is a report full of `UNVERIFIABLE`, or — worse and more likely — a `FAIL` filed
against a fix that is present. Two structural aggravators showed up in the same
delta: `23068af0` closed **five** issues in one commit while naming none, and
`73896726` ("Refactor water shader and related code for improved clarity and
functionality") touched 30+ files across 8 crates with a bullet-list body that
names no issue at all. The project memory already records the sibling hazard
(*"Multi-issue Commit Close — `Fix #A #B #C` auto-closes ONLY `#A`"*); this is the
opposite failure of the same discipline.

**Suggested direction**: require the `Fix #N` keyword per issue in the commit that
closes it (the repo convention already assumes this), and where an issue is closed
as a *side effect* of another fix — `#3102` via `#3036`, `#3095`'s siblings via
`#2986` — say so in the GitHub close comment so the archaeology survives.

---

## Disproved Candidates (investigated, then falsified — recorded, not reported)

1. **"`73896726` (the water mega-refactor) deleted `#3095`'s 146-line guard."**
   The most alarming lead of the sweep, and false. `git show f4e731f6:…tests.rs |
   grep` returned nothing, `git log -1 -- …tests.rs` resolved to a *water* commit,
   and the file was 4 lines shorter than the post-fix version — three independent
   signals all pointing at a silent revert. Blob inspection disproved it: the
   commit's only change to that file is a `cargo fmt` line-join, and the guard is
   at `:56`. The greps failed for the reason filed as **REG-D1-01**. Verifying the
   *disproof* is what produced the sweep's most useful finding.
2. **"Five shaders ship stale `.spv`."** All five source-newer-than-`.spv` pairs
   are comment-only edits (`#2805`, `#2808`, `#2798`, `#2797`, `#2242`) or resolve
   to an identical binary (`skin_palette.comp`, `#1758`). Non-comment diff is empty
   in every case.
3. **"14 closed issues have no fix."** Zero-citation is not zero-fix; all 14 were
   verified present. Reported instead as the archaeology observation above.
4. **"`precombined.rs:344` violates `#3038`'s single-normaliser rule."** It builds
   its key inline, but `precombine_oc_nif_path` synthesises a deterministic
   lowercase-hex `meshes\precombined\…` path in a namespace no authored
   `model_path` can reach. Self-consistent; not a defect. (The other two inline
   producers *are* filed — REG-D1-02.)
5. **"`#3013`'s guard is green by construction."** Technically true — the fix is a
   `log::warn!` and the test asserts the pre-existing drop — but the test's own
   docstring states this plainly, and the impact is diagnostic-only. Recorded as
   PARTIAL, not filed.
6. **"`m41-equip.sh` prints FAIL but exits 0."** A census artefact of a bad regex;
   the script accumulates `hard_fail` per cell and exits `$total_rc` at `:333`.
   The neighbouring observation that 8 of 11 smoke scripts lack SKIP-77 discipline
   is real but belongs to `/audit-runtime` (`#3003` covered p0/p1/p2 only) — not
   re-filed here.
7. **"`#3095`'s `derived_row_len()` assertion can't discriminate."** Checked the
   builder: `add_fnv_fo3_shared` and `fallout3_ruleset` gate every `push_derived`
   behind `if let (Some(out), Some(x)) = (resolve(..), resolve(..))`, so an
   unresolved EditorID genuinely drops a row and the count genuinely falls. Good
   guard.
8. **"`#3049`'s completeness table omits two ceiling fields."** It does, but both
   have their own dedicated tests one function below. Only one of those two is
   green-by-construction — filed narrowly as REG-D2-02 rather than as a
   completeness gap.

---

## Summary Table

| Issue / Contract | Title | Status | Fix Present | Guard |
|---|---|---|---|---|
| #2700 … #3098 (47 issues) | Every traceably-fixed closure in the window | PASS | 47 / 47 | present (45 with tests, 2 by construction) |
| #2930 / #2938 / #2987 / #2988 / #2993 / #2994 / #2995 / #3000-#3007 / #3023 / #3024 / #3026 | Untraceable closures, hand-verified | PASS | 14 / 14 | n/a — **no issue↔commit link** |
| #1502 … #2872 (18 issues) | Water / caustic / terrain-LOD / streaming fixes under 335 commits of churn | PASS | 18 / 18 | present |
| #3038 | `NifImportRegistry` key normalisation | **PARTIAL** | Yes | helper guarded, invariant unguarded — **REG-D1-02** |
| #3049 | `SandboxConfig::validate()` ceilings | **PARTIAL** | Yes | 10 of 11 fields falsifiable — **REG-D2-02** |
| #3089 | Dedicated cell-stream rayon pool | **PARTIAL** | Yes | guards miss the call site — **REG-D2-01** |
| #2888 (OPEN) | WATAL overlapping-surface tie-break | **fixed by `4c383433`** | Yes ×2 | one copy guarded — **REG-D3-01**; **recommend close** |
| #2987 | Skyrim Health AVIF key | PASS | Yes | pinned by `parse_real_esm.rs:191`; contract doc stale — **REG-D6-01** |
| #3070 (OPEN) | BSXFlags bit-5 comment | **premise removed** | n/a | **recommend close** |
| #2767 | SVGF/TAA mesh-ID masking | PASS | Yes | `include/mesh_id.glsl` + `.spv` lockstep |
| #2564 (OPEN) | Oblivion baseline + ROADMAP row | **half-resolved** | n/a | baseline regenerated by #3082; **ROADMAP still stale, by 6** |
| #2424 / #2425 / #2597 (OPEN) | Bare band constants | **still valid** | n/a | note-and-skip |
| #3069 | TES5 `LVLF` Use All | PASS (`0x04`) | Yes | guarded; `0x02` half owned by `/audit-skyrim` |
| Step 4 — NIFAL tier | 5 contracts | PASS | Yes | present |
| Step 4 — BSDF + GPU structs + SPIR-V | 8 contracts | PASS | Yes | `CameraUBO` reflection pin is the model guard |
| `esm/records/tests.rs` | 40 guards, 31 issues | **UNDISCOVERABLE** | Yes | binary to `grep` — **REG-D1-01** |

---

Publish with:

```
/audit-publish docs/audits/AUDIT_REGRESSION_2026-08-20.md
```

TALLY: CRITICAL=0 HIGH=0 MEDIUM=3 LOW=3
