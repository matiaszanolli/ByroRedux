# Incremental / Delta Audit — 2026-09-09

**Command**: `/audit-incremental --commits 10` (run as part of the `esm-deep` audit suite)
**Scope**: `HEAD~10..HEAD` as resolved at audit start — `b396cafc..cf8a044f`, eleven
commits once the task's own commit list (`0dcb5cf0` … `bb843dad`) is unioned with the
live range (which had already advanced to `cf8a044f`).
**Constraints honoured**: no engine launch; no `-- --ignored` in `crates/plugin`;
static analysis + `cargo check` / `cargo test` / `cargo clippy` only.

> **The working tree moved under this audit.** Between the first and last tool call the
> repository advanced from `cf8a044f` to `b85b22c0` — nine further `#3860` `GpuImage`
> migration commits (`2766b1cc` svgf, `59dda6e4` bloom, `4757342e` caustic, `2d61b8b3`
> exposure, `f971d1be` ssao, `b85b22c0` placeholder, …) landed while the report was being
> written. Findings below are pinned to the assigned range and were each re-verified
> against the code at the moment of writing; the one transient failure observed
> mid-flight is recorded in *Regression checks run* rather than as a finding.

---

## 1. Change summary

| Commit | Subject | Shape |
|---|---|---|
| `0dcb5cf0` | fix(esm): resolve both TPLT chains for actor values, and honour the absent sentinel | **behavioural fix** (#3480 / #3481) |
| `b396cafc` | fix(renderer): un-nest the skin_slots teardown drain from skin_compute | teardown/lifecycle fix (#3657) |
| `8c5e02aa` | refactor(boot): split boot.rs into boot/ — one file per concern | file split (#3855) |
| `9aae918b` | refactor(nif): split the three satellite walkers out of walk/mod.rs | file split (#3856) |
| `42f0ead4` | refactor(material): split asset_provider/material.rs and decompose the merge | file split + fn decomposition (#3857) |
| `12494c8f` | refactor(scene): decompose load_nif_bytes_with_skeleton in place | in-file decomposition (#3858) |
| `e142e3d4` | refactor(sdk): make the FourCC → FormType mapping a table | data-shape change (#3859) |
| `3a7d55cf` | refactor(nif): re-export FloatTarget/ColorTarget from core | twin-type deletion (#3862) |
| `6079db3b` | refactor(core): hoist ImageSpaceModifier into core | twin-type deletion (#3861) |
| `bb843dad` | refactor(renderer): add GpuImage | new Vulkan resource type (#3860 part 1/15) |
| `cf8a044f` | refactor(renderer): migrate taa.rs history slots to GpuImage | Vulkan lifecycle migration (#3860 part 2/15) |

50 files, +9 837 / −8 195. Themes: one ESM correctness fix, one renderer teardown fix,
and nine refactors — the highest-risk shape for a delta audit, because behaviour can
change silently while `cargo check` and the test suite stay green.

**Overall assessment: the refactors are unusually well-executed.** Every "pure move" in
the range was verified mechanically (below) rather than taken on the commit message's
word, and all of them hold. The four findings are small, and none is a behaviour
regression introduced by the range.

---

## 2. Routing map

| Changed path | Owning audit(s) applied | Verdict |
|---|---|---|
| `crates/plugin/src/esm/records/actor_value_derive.rs` | `/audit-esm`, `/audit-character`, `/audit-skyrim`, `/audit-fo4` | 1 finding (INC-01) |
| `crates/renderer/src/vulkan/image.rs` *(new)* | `/audit-renderer`, `/audit-safety`, `/audit-concurrency` | 1 finding (INC-04) |
| `crates/renderer/src/vulkan/taa.rs` | `/audit-renderer` | clean |
| `crates/renderer/src/vulkan/context/teardown.rs` | `/audit-renderer`, `/audit-safety` | clean |
| `crates/renderer/src/vulkan/skin_compute.rs` | `/audit-renderer`, `/audit-concurrency` | clean |
| `crates/renderer/src/vulkan/context/{draw,post_passes}.rs`, `presentation.rs`, `src/lib.rs`, `vulkan/mod.rs` | `/audit-renderer` | clean (rename-only) |
| `byroredux/src/boot.rs` → `byroredux/src/boot/**` (10 files) | `/audit-concurrency` Dim 4, `/audit-ecs` Dim 5 | 2 findings (INC-02, INC-03) |
| `byroredux/src/scheduler_access_tests.rs` | `/audit-concurrency` Dim 4 | clean |
| `crates/nif/src/import/walk/**` (mod + 4 new files) | `/audit-nif`, `/audit-nifal` | clean |
| `crates/nif/src/anim/types.rs`, `crates/core/src/animation/{mod,registry,types}.rs` | `/audit-nif`, `/audit-ecs` Dim 10 | clean |
| `byroredux/src/asset_provider/material.rs` → `material/**` (4 files) | `/audit-nifal`, `/audit-fo4`, `/audit-starfield` | clean |
| `byroredux/src/asset_provider/{mod.rs,tests/bgsm_merge.rs}`, `byroredux/src/material_translate.rs` | `/audit-nifal` | clean |
| `byroredux/src/scene/nif_loader.rs` | per-game `/audit-<game>`, `/audit-nifal` | clean |
| `byroredux/src/anim_convert.rs`, `byroredux/src/app_frame.rs` | `/audit-ecs`, `/audit-renderer` | clean |
| `crates/core/src/imagespace.rs` *(new)*, `crates/core/src/lib.rs` | `/audit-ecs`, `/audit-save` | clean |
| `crates/scripting/src/{cinematic,lib}.rs` | `/audit-scripting`, `/audit-save` | clean |
| `byroredux/src/save_io/serde_default_guard_tests.rs` | `/audit-save` | clean (verified, §4) |
| `crates/sdk/src/compatibility/storage_util.rs` | **no owner audit** — `crates/sdk` is on `_audit-common`'s un-owned list; audited under `/audit-esm` (record-signature vocabulary) + `/audit-ecs` | clean (verified, §4) |
| `crates/sfmaterial/src/lib.rs` | `/audit-starfield`, `/audit-tech-debt` (doc rot) | clean (doc-path fix) |
| `.claude/issues/3855 3856 3857 3858/*.md` | `/audit-tech-debt` (doc rot) | **skipped** — issue scratch notes, not code or shipped docs |

**Nothing was skipped for lack of time.** The only skip is the `.claude/issues/**`
scratch pair above. Per `_audit-common` § un-owned subsystems: this delta *did* touch
one un-owned subsystem (`crates/sdk`), and it is audited above rather than silently
passed over.

**Un-owned subsystems NOT reached by this delta** (none changed in range, listed for the
coverage-honesty rule): Gameplay slice (P2), launcher (`boot-request` / `game-detect` /
`settings-io` / `byro-launcher` / `byro-detect`), FaceGen, mod-runtime, FSR3+FFI,
`crates/hkx`, debug-server / debug-protocol.

---

## 3. Findings

### INC-2026-09-09-01: TPLT absent-sentinel fallback only consults the two chain endpoints, so an intermediate template's authored `DNAM` Health is still lost
- **Severity**: MEDIUM
- **Dimension**: ESM / record inheritance (`/audit-esm`), CHARAL population
- **Location**: [`crates/plugin/src/esm/records/actor_value_derive.rs:305-360`](../../crates/plugin/src/esm/records/actor_value_derive.rs)
- **Changed in**: `crates/plugin/src/esm/records/actor_value_derive.rs` (commit `0dcb5cf0`)
- **Status**: NEW. Dedup: `gh issue list --state all --limit 400` refreshed 2026-09-09 —
  #3481 (the sibling this partially fixes) is **CLOSED** by this very commit; no open or
  closed issue covers the multi-hop case. `docs/audits/` scan: no prior report mentions
  `baked_or_shell` (the symbol did not exist before this commit). Not sourced from a
  pre-2026-06-07 report, so the pre-cutoff caveat does not apply; the premise below was
  verified against the code at HEAD, not against issue absence.
- **Description**: `0dcb5cf0` correctly restored the "`0` = absent" fallback for the FO4
  baked `DNAM` pair and for an empty `PRPS`. But the fallback compares only the *shell*
  against the *terminal* record of the `TPLT` chain. `resolve_inherited_record` does not
  merge per field — it recurses to the deepest record that does not itself delegate and
  returns that one — so every intermediate `NPC_` in a chain of length ≥ 2 is invisible
  to the new fallback. If the terminal record leaves `calculated_health` at the absent
  sentinel while an intermediate authors it, the value the engine should use is skipped
  and the code falls all the way back to the shell, which is very likely `0` as well.
- **Evidence**: the fallback is strictly two-ended —
  ```rust
  // actor_value_derive.rs:317
  let props = if stats.actor_value_props.is_empty() {
      &shell.actor_value_props
  } else {
      &stats.actor_value_props
  };
  // actor_value_derive.rs:354
  fn baked_or_shell(resolved: u16, shell: u16) -> u16 {
      if resolved > 0 { resolved } else { shell }
  }
  ```
  and `stats` is the *terminal* of the walk, not a per-field merge:
  ```rust
  // crates/plugin/src/equip.rs:427-435
  if npc.template_flags & flag == 0 || npc.template_form_id == 0 { return npc; }
  if let Some(base) = index.npcs.get(&npc.template_form_id) {
      return resolve_inherited_record(base, actor_level, index, flag, depth + 1);
  }
  ```
  `TPLT_MAX_DEPTH = 6` (`equip.rs:331`), so up to four intermediate records can sit
  between the two endpoints the fallback inspects.
- **Impact**: the exact symptom #3481 was filed for — `stamp_actor_values` only inserts
  `ActorVitals` when the Health key is present, so an affected actor spawns undamageable
  and unkillable. **Reachability is narrow**: `resolve_inherited_record`'s own doc
  records that "vanilla template chains are flat (Lvl* template → base NPC, one hop)",
  and for a one-hop chain the two endpoints *are* the whole chain, so vanilla FO4 is
  fully covered by the fix as written. The gap is mod content that chains a per-faction
  wrapper (the case `TPLT_MAX_DEPTH`'s doc says it exists to accommodate). I could not
  measure the real-data count: doing so needs the `crates/plugin` `--ignored` ESM-parsing
  tests, which are barred by the OOM constraint (see §5).
- **Related**: #3480, #3481 (both CLOSED by `0dcb5cf0`); #2956, #3381, #3382, #3390.
- **Suggested Fix**: make the absent-sentinel resolution walk the chain instead of
  sampling its ends — e.g. a `resolve_inherited_field(npc, flag, index, |r| r.calculated_health)`
  that returns the first record from shell to terminal whose accessor is non-sentinel,
  reusing the same depth cap and `LVLN`/`LVLC` pick. That collapses the `PRPS` half and
  both `DNAM` halves into one rule and removes the two-endpoint assumption entirely.

---

### INC-2026-09-09-02: the boot `SOURCES` completeness gate walks two hard-coded directories, so a new `boot/` subdirectory is invisible to it
- **Severity**: LOW
- **Dimension**: concurrency / scheduler source-shape coverage (`/audit-concurrency` Dim 4), test-gap
- **Location**: [`byroredux/src/boot/mod.rs:576-605`](../../byroredux/src/boot/mod.rs)
- **Changed in**: `byroredux/src/boot/mod.rs` (commit `8c5e02aa`)
- **Status**: NEW. Dedup: no issue title matches `SOURCES`, `boot/`, or the gate's name;
  no prior report in `docs/audits/` covers it (the file is one day old). Verified against
  code, not issue absence.
- **Description**: `the_concat_list_covers_every_file_in_the_boot_directory` exists
  because "a new file carrying registrations that nobody adds to `SOURCES` is invisible
  to the scheduler's source-shape tests: they keep passing while covering strictly less
  than they claim to." Its own directory walk is hard-coded to exactly two directories,
  so a file under any *third* directory — `boot/schedule/extra/foo.rs`, or a future
  `boot/registries/` — reproduces precisely the hole the gate was written to close, and
  the gate stays green.
- **Evidence**:
  ```rust
  // byroredux/src/boot/mod.rs:586
  for dir in [root.clone(), root.join("schedule")] {
      let prefix = if dir == root { "" } else { "schedule/" };
      for entry in std::fs::read_dir(&dir).unwrap() {
  ```
  Contrast the sibling gate in the same range, which *does* recurse:
  ```rust
  // crates/renderer/src/vulkan/image.rs:455-462
  let mut stack = vec![root.clone()];
  while let Some(dir) = stack.pop() {
      for entry in std::fs::read_dir(&dir)… { if path.is_dir() { stack.push(path); continue; } …
  ```
- **Impact**: coverage loss only; no runtime effect. Scheduler declared-access
  invariants (the boot deadlock proof) are the thing that would silently stop being
  checked, which is why it is worth closing rather than tolerating.
- **Related**: INC-2026-09-09-03 (same gate family), #3855.
- **Suggested Fix**: replace the two-element array with the same explicit
  directory-stack walk `image.rs` uses, deriving `prefix` from `strip_prefix(&root)`.

---

### INC-2026-09-09-03: `TRUNCATING_TEST_MODULES` is hand-maintained with no completeness gate, and the `SOURCES` doc overstates the ordering invariant that actually holds
- **Severity**: LOW
- **Dimension**: concurrency / scheduler source-shape coverage (`/audit-concurrency` Dim 4), test-gap, doc-rot
- **Location**: [`byroredux/src/boot/mod.rs:42-52`](../../byroredux/src/boot/mod.rs) (the doc), [`byroredux/src/boot/mod.rs:512-568`](../../byroredux/src/boot/mod.rs) (the list and its pin)
- **Changed in**: `byroredux/src/boot/mod.rs` (commit `8c5e02aa`)
- **Status**: NEW. Dedup: no issue or prior report covers it; file is one day old.
  Verified against code.
- **Description**: two related gaps in the same convention.
  1. The pin `every_production_file_precedes_the_first_test_module` proves the ordering
     property only for the three module names listed in `TRUNCATING_TEST_MODULES`.
     Nothing checks that the list is *complete*. A fourth source-shape module that
     truncates `SOURCES` at its own `mod` declaration — the convention this file
     explicitly invites — that is not added to the list would truncate silently and
     produce exactly the "scans silently assert on nothing" outcome. This is the same
     hazard the sibling gate `the_concat_list_covers_every_file_in_the_boot_directory`
     was written to close for *files*, left open for *modules*.
  2. The `SOURCES` doc states the enabling condition too broadly: *"That convention only
     holds while every production registration appears **before the first test module's
     text**"*. That is not true of the current ordering, and the concat is nevertheless
     correct — because the three truncation sentinels all live in `schedule/mod.rs`,
     which is deliberately last. `world.rs` is second in `SOURCES` and carries
     `#[cfg(test)] mod ai_storage_registration_tests` at line 388, ahead of all five
     stage files' registrations; `cli.rs` carries two more test modules at lines 85 and
     456. The pin's own doc (`mod.rs:535-539`) states the real invariant precisely —
     "before the *earliest such declaration*". The reader most likely to be misled is
     whoever adds truncation module #4 and reasons from the wrong rule.
- **Evidence**:
  ```rust
  // byroredux/src/boot/mod.rs:512  — hand-maintained, no completeness check
  const TRUNCATING_TEST_MODULES: &[&str] = &[
      "fragment_activation_order_tests",
      "scheduler_timings_gate_tests",
      "system_access_declaration_tests",
  ];
  ```
  ```
  $ grep -n '^#\[cfg(test)\]' byroredux/src/boot/world.rs
  388:#[cfg(test)]
  $ grep -n '^#\[cfg(test)\]' byroredux/src/boot/cli.rs
  85:#[cfg(test)]
  456:#[cfg(test)]
  ```
  and `SOURCES` orders `mod.rs, world.rs, schedule/early.rs, …, cli.rs, schedule/mod.rs`.
- **Impact**: coverage loss only. Same blast radius as INC-02.
- **Related**: INC-2026-09-09-02, #3855.
- **Suggested Fix**: derive `TRUNCATING_TEST_MODULES` from the source instead of
  restating it — scan `SOURCES` for `.split("mod <name>")` call sites, or add an
  assertion that every `mod *_tests {` declaration found in a file *other than*
  `schedule/mod.rs` is not used as a truncation sentinel anywhere. Separately, reword
  `SOURCES`'s doc to match the pin's own precise phrasing ("the earliest truncation
  sentinel", not "the first test module").

---

### INC-2026-09-09-04: `GpuImage` handles a poisoned allocator mutex two different ways inside the one type whose stated purpose is to state the allocator-lock rules once
- **Severity**: LOW
- **Dimension**: renderer resource lifecycle (`/audit-renderer`), `/audit-safety`
- **Location**: [`crates/renderer/src/vulkan/image.rs:188-190`](../../crates/renderer/src/vulkan/image.rs) vs [`crates/renderer/src/vulkan/image.rs:289-302`](../../crates/renderer/src/vulkan/image.rs)
- **Changed in**: `crates/renderer/src/vulkan/image.rs` (commit `bb843dad`)
- **Status**: NEW. Dedup: no issue matches; #3860 is the parent refactor issue and
  describes the consolidation, not this. Verified against code at HEAD.
- **Description**: `GpuImage`'s module doc says the allocator-lock rules "now live here
  once". They are stated twice, differently. The allocation path panics on a poisoned
  mutex (`.expect("allocator lock")`, inherited verbatim from the `taa.rs` copy it
  replaced); the free path — added new in this commit — deliberately recovers from
  poisoning via `into_inner()`. So the same `Mutex<Allocator>`, in the same type, aborts
  the process when locked to allocate and is recovered when locked to free.
- **Evidence**:
  ```rust
  // create — panics on poison
  let allocation = match allocator
      .lock()
      .expect("allocator lock")
      .allocate(&vk_alloc::AllocationCreateDesc { … })
  ```
  ```rust
  // free_allocation — recovers from poison
  match allocator.lock() {
      Ok(mut guard) => { if let Err(e) = guard.free(allocation) { log::error!(…); } }
      Err(poisoned) => { if let Err(e) = poisoned.into_inner().free(allocation) { log::error!(…); } }
  }
  ```
- **Impact**: no behaviour change versus the copies it replaced (this is preserved, not
  introduced). It matters because this is the consolidation site: whichever rule stands
  here is the one all fourteen migrations inherit, and the migrations are landing now
  (nine had already landed by the end of this audit). A poisoned allocator means another
  thread panicked mid-allocation; the free path treats that as recoverable and the
  create path turns it into a second panic during teardown.
- **Related**: #3860, #1163, #1165, #927.
- **Suggested Fix**: pick one and say why in the doc. Given `free_allocation` already
  models poison as recoverable, `create` should use the same `Ok/Err(poisoned)` shape and
  return an `anyhow::Error` on the poisoned path rather than panicking — that keeps a
  failed allocation on the same recoverable path as every other `create` error arm.

---

### INC-2026-09-09-05: `cargo clippy` is red at HEAD — `crates/renderer` under `--all-targets`, and CI's own `--workspace -- -D warnings` on `byroredux-core`
- **Severity**: LOW
- **Dimension**: `/audit-tech-debt`, CI gate health
- **Location**: [`crates/core/src/animation/player.rs:110`](../../crates/core/src/animation/player.rs), [`crates/core/src/animation/stack.rs:426`](../../crates/core/src/animation/stack.rs) (+458, +503), [`crates/renderer/src/vulkan/buffer.rs:1630`](../../crates/renderer/src/vulkan/buffer.rs), [`crates/renderer/src/vulkan/context/draw.rs:1876`](../../crates/renderer/src/vulkan/context/draw.rs), [`crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:3929`](../../crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs)
- **Changed in**: **outside the audited range** — `git blame` dates these to `2026-09-01`
  (`d06f6df9`), `2026-09-02` (`19742e80`), `2026-09-03` (`b796ed20`) and `2026-08-20`
  (`f5abee08`). Reported here because a delta audit's regression pass ran the gates and
  found them red, not because the range caused it.
- **Status**: NEW. Dedup: refreshed `gh issue list --state all --limit 400` has no
  clippy / lint / `-D warnings` issue; `grep -rl 'clippy --workspace\|neg_cmp_op_on_partial_ord\|undocumented_unsafe_blocks' docs/audits/` returns nine reports, all
  pre-dating these commits and none describing the current failures. Verified against
  code + a local run, not against issue absence.
- **Description**: CI runs `cargo clippy --workspace -- -D warnings`
  ([`.github/workflows/ci.yml:155-156`](../../.github/workflows/ci.yml)) on
  `dtolnay/rust-toolchain@stable`, which matches the local toolchain (1.96.0). That
  command fails today with four `neg_cmp_op_on_partial_ord` errors in `byroredux-core`.
  Separately, `cargo clippy --workspace --all-targets` fails `byroredux-renderer` with
  two `undocumented_unsafe_blocks` errors (deny-listed at
  `crates/renderer/src/lib.rs:21`) plus one `approx_constant`. Both unsafe sites *do*
  carry SAFETY comments — the lint fires because intervening statements were inserted
  between the comment and the `unsafe` block, so the comment no longer immediately
  precedes it.
- **Evidence**:
  ```
  $ cargo clippy --workspace -- -D warnings
  error: the use of negated comparison operators on partially ordered types …
     --> crates/core/src/animation/stack.rs:426:12
  426 |         if !(ew >= 0.001) {
  error: could not compile `byroredux-core` (lib) due to 4 previous errors
  ```
  The `!(x >= y)` form is a deliberate NaN guard, so the fix is a rewrite for the lint's
  benefit, not a logic change.
- **Impact**: the clippy gate is currently unreachable, because the same CI job fails
  one step earlier: `cargo test` fails eight `byroredux-ui` tests on the runner
  (`navigator::tests::*`, `player::render_failure_tests::*`) — those pass locally, so the
  failure is the headless runner's lack of a wgpu adapter, not the code. Five further CI
  jobs are also red at `12494c8f`. Net effect: **nothing in the audited range was gated
  by CI**, and the clippy error will surface only once the `byroredux-ui` step is fixed.
- **Related**: `docs/audits/AUDIT_UI_2026-09-09.md` (same-day UI audit; the `byroredux-ui` half likely belongs there).
- **Suggested Fix**: two separate small changes — rewrite the four `!(x >= y)` NaN guards
  as `if !(x.partial_cmp(&y) == Some(Ordering::Greater | Ordering::Equal))` or, more
  readably, `if x.is_nan() || x < y`; and move the two SAFETY comments back to
  immediately precede their `unsafe` blocks.

---

## 4. Verifications performed (findings ruled out)

These are the hazards the task flagged as documented traps. Each was checked
mechanically rather than accepted on the commit message's word; all passed.

**`e142e3d4` — 106 match arms → `FORM_TYPE_IDS` table.** Extracted both the pre-commit
match arms and the post-commit table with a script and compared as maps: **106 signatures
each, zero only-in-old, zero only-in-new, zero differing values**, and the table is
lexicographically sorted (which `binary_search_by_key` requires and
`form_type_table_is_sorted_and_unique` now pins). Arm-for-arm equivalent. No finding.

**`6079db3b` — deleting the `ImageSpaceModifier` twin.** The surviving
`crates/core/src/imagespace.rs` carries all 14 fields in the deleted twins' names, order
and types, and all 14 defaults byte-identical (including the four non-obvious ones:
`radial_blur_down_start: 1.0`, `radial_blur_center: [0.5, 0.5]`,
`tint_color: [1.0, 1.0, 1.0, 0.0]`, unity saturation/brightness/contrast). All consumers
were retargeted (`app_frame.rs`, `renderer/lib.rs`, `context/draw.rs`,
`context/post_passes.rs`, `presentation.rs`, `scripting/{cinematic,lib}.rs`); the
14-assignment bridge is gone with no residue.
*Feature-gating checked separately*: core's `save` feature gates the serde derives, and
`crates/scripting`'s `save = ["serde", "byroredux-core/save"]` propagates it — so this is
**not** the `--features inspect` unification trap `CLAUDE.md` documents.
*Save compatibility independently re-derived*: the runtime `schema_fingerprint`
(`crates/save/src/registry.rs:305`) hashes **registered column names**, not Rust type
names or field shapes, and the payload is `serde_json` (`snapshot.rs:212`), which does
not emit `serialize_struct`'s name argument. Renaming the type therefore cannot move the
fingerprint `decode` checks at `snapshot.rs:251-257`. The test-only
`BASELINE_SHAPE_FINGERPRINT` move without a `FORMAT_MAJOR` bump is correct. No finding.

**`3a7d55cf` — `FloatTarget` / `ColorTarget` twin deletion.** The deleted NIF-side enums
had 13 and 7 variants matching core's element-for-element, including the payload on
`MorphWeight(u32)`; the 20-arm bridge in `anim_convert.rs` was pure identity. Neither enum
carries explicit discriminants and neither is serialized by discriminant, so ordering is
not load-bearing. `target_enum_identity_tests` makes a future redeclaration a *build*
error rather than a test failure. No finding.

**`include_str!` / source-shape tests across the four splits.** This is the trap the task
called out, and all four splits handled it deliberately:
- `42f0ead4` introduced `asset_provider::material::SOURCES` (a 4-file `concat!`) and
  retargeted all three text-reading tests to it (`tests/bgsm_merge.rs` ×2 plus
  `material_translate.rs`) instead of pointing each at whichever new file it landed in.
- `8c5e02aa` introduced `boot::SOURCES` (a 10-file `concat!`) and retargeted all 30 sites,
  including `scheduler_access_tests.rs`'s `BOOT_RS`. The **load-bearing order** is
  documented, and two pins guard it (`every_production_file_precedes_the_first_test_module`,
  `the_concat_list_covers_every_file_in_the_boot_directory`) — INC-02 and INC-03 are the
  two remaining holes in those pins, not a defanged assertion.
- `12494c8f` decomposed `load_nif_bytes_with_skeleton` **in place** rather than splitting
  the file, explicitly because three tests read `nif_loader.rs` as text.
- `9aae918b` split `walk/mod.rs`, and its new `satellite_independence_tests` slices the
  two scene-graph walkers' bodies out of `mod.rs` before scanning, so the assertions are
  not vacuous.
No defanged assertion found in any of the four.

**Behavioural equivalence of the four splits/decompositions.** For each, I compared the
whitespace- and comment-normalised multiset of removed vs added lines:
- `8c5e02aa` (boot): **zero lines removed that are not also added** — a pure move.
- `12494c8f` (`nif_loader.rs`): 13 distinct removed lines, every one accounted for by
  line reflow or by the three documented `continue;` → `return false;` conversions; the
  call site (`nif_loader.rs:492-505`) preserves `count += 1` semantics exactly.
- `9aae918b` (walk): removals are entirely `use` blocks, function signatures and
  `super::` → `super::super::` path adjustments.
- `42f0ead4` (material): removals are the six inline LRU eviction blocks (replaced by
  `half_evict`, whose loop I verified equivalent — `>= cap` gate, `cap / 2` pops, and the
  new `else { break; }` is a no-op change once the deque empties), the `set_*`/`touched`
  locals moving into the extracted arms as a `&mut bool`, and the two `return X` →
  `return Some(X)` conversions that the `if let Some(outcome) = merge_bgsm_arm(…) { return outcome; }`
  dispatch consumes verbatim. `merge_external_material` is still the single exported
  entry point and the only path to a `&mut ImportedMaterial` (#2412's invariant), now
  pinned by `merge_external_material_is_the_only_exported_fn_in_this_file`.

**GPU-struct lockstep (`bb843dad` / `cf8a044f`).** `git diff b396cafc~1..cf8a044f | grep 'repr(C)'`
returns **nothing**, and no file under `crates/renderer/shaders/` (`.glsl` / `.comp` /
`.vert` / `.frag`) changed in the range. `bindings.glsl` and the four standalone
`GpuInstance` copies (`triangle.vert`, `ui.vert`, `water.vert`, `caustic_splat.comp`)
therefore cannot have drifted. The lockstep rule is N/A here — verified, not assumed.
`crates/renderer` lib tests confirm: `gpu_instance_glsl_copies_stay_in_lockstep`,
`gpu_light_glsl_copies_stay_in_lockstep`, `gpu_water_params_…`, `gpu_terrain_tile_…` all pass.

**Vulkan teardown ordering (`b396cafc` / `cf8a044f`).** The `skin_slots` drain is now
unconditional with the pipeline guard moved *inside* the loop body, so the descriptor-set
free still happens whenever a pipeline exists and the buffer + its `Arc<Mutex<Allocator>>`
clone are released either way. `destroy_slot`'s signature was correctly widened to
`mut slot: SkinSlot` and is now the composition of the free + `SkinSlot::destroy`, not a
second copy. The load-bearing local ordering (`skin_slots` drain **before**
`SkinComputePipeline::destroy`, VUID-vkFreeDescriptorSets-descriptorPool-parameter) still
holds at `teardown.rs:79` vs `:138` and is pinned by the new source-shape test. I
independently re-checked the commit's "sibling sweep" claim against
`teardown.rs`: `image_health_buffers` (`:53`) and `morph_slots` (`:90`) are indeed
unconditional drains, so `skin_slots` was the last `Option`-nested one. Correct.
For `cf8a044f`, `HistorySlot = GpuImage` preserves the field names so all eight read
sites are untouched; the new view's `ImageSubresourceRange` is byte-identical to the
`color_subresource_single_mip()` it replaced (`descriptors.rs:144-152`); and every
`GpuImage` reachable from `TaaPipeline` is destroyed on all three paths — the
`try_or_cleanup!` error arm (`taa.rs:245`, which calls `partial.destroy`), the
`recreate_on_resize` partial-failure arm (`taa.rs:800-806`), and `destroy` itself — so
the new `Drop` safety net's `debug_assert!(false)` cannot fire on a normal error path.

**NIFAL boundary.** `translate_material` was not touched. `merge_external_material`'s
signature is unchanged (`&mut ImportedMaterial`, not `&mut ImportedMesh`) and its
`textures_before` snapshot / `record_external_texture_sources` tail is intact. Both load
paths (`cell_loader/spawn.rs`, `scene/nif_loader.rs`) still route through the same
boundary. No divergence.

**Visibility widening in the walk split.** The extracted functions became `pub(crate) fn`,
but they live in `mod emitter;` / `mod lights;` / `mod node_attrs;` / `mod texture_effect;`
— all private — inside `mod walk;`, itself private in `import/mod.rs`. Effective
visibility is therefore unchanged from the pre-split `pub(super)`; the `pub(super) use`
re-exports in `walk/mod.rs` are what any outside caller still goes through. Not a finding.

---

## 5. Regression checks run

| Check | Result |
|---|---|
| `cargo check --workspace --all-targets` | **clean** |
| `cargo test --workspace` | **green** after the transient below |
| `cargo test -p byroredux-renderer --lib` (re-run at moved HEAD) | **975 passed, 0 failed** |
| `cargo clippy --workspace -- -D warnings` (CI's exact command) | **RED** → INC-05 |
| `cargo clippy --workspace --all-targets` | **RED** (3 errors in `byroredux-renderer`) → INC-05 |
| GitHub Actions `ci.yml` at `12494c8f` | 6 of 10 jobs failing, cause pre-dates the range → INC-05 |

**Transient observed mid-audit, not a finding.** The first `cargo test --workspace` run
failed exactly one test —
`vulkan::image::tests::no_file_outside_this_module_rolls_its_own_image_chain` — with the
`PENDING` ledger listing `bloom.rs` while `bloom.rs` no longer contained
`bind_image_memory`. This was a mid-commit snapshot of `59dda6e4`
(*migrate bloom.rs mip images to GpuImage*) landing under the audit; the ledger's own
staleness assertion caught it, which is the gate working as designed. Re-running after
the commit completed is green. Recorded here rather than as a finding because it was
never a committed state.

**Issue/commit traceability check** (the documented `Fix #A #B #C` trap): every issue
claimed by the range — #3480, #3481, #3657, #3855, #3856, #3857, #3858, #3859, #3861,
#3862 — is **CLOSED**. #3860 correctly remains OPEN (it is a 15-part series, 2 parts
landed in range). `0dcb5cf0` repeated the keyword per issue (`Fix #3480, Fix #3481`) and
both closed. No trap hit.

---

## 6. Missing tests

Changed code paths with no corresponding test, flagged even where the code is correct.

1. **`GpuImage`'s error-cleanup ordering is untested** — and it is the entire reason the
   type exists. `crates/renderer/src/vulkan/image.rs` ships two tests: a pure descriptor
   pairing check and a filesystem scan of the `PENDING` ledger. Neither exercises
   `create`'s three error arms, which is where #1163 / #1164 / #1165 / #2178's shared
   defect lived. Those arms need a device, so `cargo test` cannot reach them — but a
   fault-injection seam (the `BYRO_VALIDATION` / `BYRO_FSR_FORCE_DISPATCH_FAIL` pattern
   already in the tree) could force an allocate/bind/view failure once and assert the
   allocator's live-allocation count returns to baseline. Without it, fourteen call sites
   are being migrated onto an untested cleanup path.
2. **`SkinSlot::destroy`'s pipeline-less branch has no runtime test** — acknowledged in
   `b396cafc` (no live configuration reaches `skin_compute == None`) and pinned only by
   a source-shape test. Correct given the constraint; listed so the gap stays visible if
   a device-lost recovery path ever lands.
3. **`baked_or_shell` has no multi-hop chain test** — the four new tests in `0dcb5cf0`
   all use single-hop shell → template chains. A `shell → A → B` fixture is what would
   have surfaced INC-01.
4. **The boot `SOURCES` gates have no negative test for a new subdirectory or a fourth
   truncation sentinel** — INC-02 and INC-03. Both other pins in that module were
   negative-tested by breaking them; these two cases were not.

---

## 7. Next step

```
/audit-publish docs/audits/AUDIT_INCREMENTAL_2026-09-09.md
```
