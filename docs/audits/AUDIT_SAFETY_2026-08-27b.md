# Safety Audit — 2026-08-27 (b)

Run as part of the `comprehensive` `/audit-suite` preset.
Protocol: `.claude/commands/_audit-common.md` · severity scale:
`.claude/commands/_audit-severity.md` · dimensions:
`.claude/commands/audit-safety/SKILL.md` (all 11, including Dim 11
`crates/mod-runtime` and the `crates/fsr3-sys` FFI surface).

## Why this file is `-b`

`docs/audits/AUDIT_SAFETY_2026-08-27.md` already exists and is **committed**
(`c6c8ba55`), from the earlier `streaming-deep` preset run on the same day.
Overwriting it would destroy a 614-line report, so this comprehensive-preset
pass is filed alongside it under the `-b` suffix the repo already uses for
same-day sibling reports (cf. *AUDIT_RENDERER_2026-08-12b*, cited by name from
`crates/fsr3-sys/src/lib.rs`). Nothing below re-reports a finding from that run.

## Delta audited

All **four** findings of the earlier run are **closed and fixed in the tree**:

| Prior finding | Fix |
|---|---|
| SAFE-2026-08-27-01 (#3372, CRITICAL — compacted offsets vs. pre-compaction buffer) | `cd1aa9e9` — `plan_geometry_compaction` / `apply_compaction_plan` split; publish moved to swap-in. Re-verified below |
| SAFE-2026-08-27-02 (#3373, MEDIUM — `sanitize_finite` misses the BGEM glass tail) | `59b85565`. Re-verified below; one residual test-gap filed as SAFE-2026-08-27b-03 |
| SAFE-2026-08-27-03 (#3374, LOW — `MorphSlot` drain nested in the skin guard) | `95005d87` — drain hoisted, source-position pin added |
| SAFE-2026-08-27-04 (#3375, LOW — `EntityId` "is generational") | `2db648ee` |

This pass therefore covers `c6c8ba55..HEAD` — **22 commits**, 72 files, +3945/−433
— plus an independent re-derivation of every standing regression guard.

## Scope line

| Dim | Depth | Notes |
|---|---|---|
| 1 FFI lifetime | **Medium** | `fsr3-sys` read end-to-end (733 LOC); `cxx-bridge` scope guard re-confirmed; `crates/ui` re-confirmed to hold zero `unsafe`. |
| 2 Memory corruption / UB | **Deep** | ECS cached-pointer contract, `read_pod_vec` + the `header.rs` mirror (changed this week by #2272), `pex` transmute contiguity re-counted mechanically, `sfmaterial` checked decode, `GpuMaterial`/`GpuInstance` Rust↔GLSL diffed by script. |
| 3 Leaks & drop ordering | **Medium** | Three regression guards re-verified; `#3372`'s new `deferred_compaction` lifecycle traced through every abandon path; `MorphSlot`/`SkinSlot` unload-victim synchronous-destroy argument re-derived against the fence wait (holds — see PASS). |
| 4 Unsafe-block discipline | **Deep, mechanised** | **718 `unsafe {` blocks, 718 carry a SAFETY comment.** Zero gaps. (A backward-only scan reports 101–157 "missing"; this codebase routinely puts the comment *inside* the block — e.g. `vulkan/exposure.rs:73-75`. Scanning ±25/+5 lines closes it to zero.) |
| 5 Vulkan spec | **Static only** | No engine launched, no validation-layer evidence. Nothing here asserts a barrier/render-pass/pipeline bug. |
| 6 R1 material layout | **Deep, mechanised** | `GpuMaterial` 108 Rust scalars ↔ 108 GLSL scalars, exact field-order match, 432 B; five `GpuInstance` GLSL copies mutually consistent; `MAX_MATERIALS` intern cap + upload clamp in lockstep. |
| 7 RT IOR / glass | **Shallow** | `triangle.frag` and `shader_constants_data.rs` are unchanged in this delta; recorded as PASS-by-reference to the 2026-08-27 run rather than re-derived. |
| 8 NPC / animation spawn | **Deep** (the delta's centre) | `crates/core/src/animation/` changed this week (#3258). Two NEW findings. |
| 9 NIFAL NaN/Inf | **Deep, mechanised** | `Material` float-field list diffed against `sanitize_finite` by script (33/33 covered). |
| 10 debug-ui teardown | **Shallow** | `egui_pass.rs` / `teardown.rs` unchanged in the delta; PASS-by-reference. |
| 11 mod-runtime | **Medium** | Whole crate (1183 LOC) read; audited as a contract. |

**Un-owned subsystems** (per `_audit-common`): `crates/sdk` — audited this time,
including its live engine consumer `byroredux/src/studio_host.rs` (the earlier
run checked it only for `unsafe`). `crates/facegen` and `crates/hkx` —
allocation-bound sweep run. `crates/debug-server` — audited, one LOW filed.
`crates/mod-runtime` — Dim 11. **Not covered**: the gameplay slice
(`combat.rs`/`inventory.rs`/`settings_io.rs`) beyond incidental reads, and the
FSR3 renderer-side passes beyond the `#2519` delta.

**No engine process was launched** and no Vulkan render-pass / barrier /
pipeline-state change is proposed anywhere in this report, per the standing
no-speculative-Vulkan-fixes rule.

Dedup performed against `gh issue list --limit 1000 --state all` (1000 issues)
and all 30 prior `docs/audits/AUDIT_SAFETY_*.md`.

## Summary

**5 findings**: 0 CRITICAL · 0 HIGH · 2 MEDIUM · 3 LOW.

---

## Findings

### MEDIUM

#### SAFE-2026-08-27b-01: `#3258` sanitised `frequency` and stopped there — `NiControllerSequence`'s `duration` and `weight` are equally raw, and both still latch a NaN into the pose

- **Severity**: MEDIUM
- **Dimension**: 8 (NPC / animation spawn safety) + 9 (NaN/Inf on the GPU)
- **Location**: producer `crates/nif/src/anim/sequence.rs:20` (`duration`) and `:23` (`weight`); boundary `byroredux/src/anim_convert.rs:506` + `:520`; consumers `crates/core/src/animation/player.rs:62-64` + `:133-138`, `crates/core/src/animation/stack.rs:172-181`, `:332-334`, `:378-380`
- **Status**: NEW. Sibling of #3258 (CLOSED, `medium`, fixed in `bbfd742f`); nothing in the issue list or `docs/audits/` covers `duration`/`weight`.
- **Description**:

  #3258 established the rule: `NiControllerSequence` scalars are raw file data,
  and a non-finite one that reaches the animation clock latches the entity's
  pose to NaN for the rest of its life. It fixed exactly one scalar,
  `frequency`, at the translate boundary (`sanitized_clip_frequency`), plus a
  defense-in-depth `finite_time_delta` on the `dt * speed * frequency` product.

  The **two adjacent fields of the same struct, read by the same parser
  function, and passed through the same three lines of `convert_nif_clip`, were
  not touched** — and each has its own latch:

  1. **`duration`** — `CycleType::Reverse` routes through `fold_reverse_time`
     (`player.rs:62-83`), whose only guard is `if duration <= 0.0`. `NaN <= 0.0`
     is **false**, so a NaN duration falls through: `period = 2.0 * NaN = NaN`,
     `m = (phase + delta).rem_euclid(NaN) = NaN`, and the `m > duration` branch
     is `NaN > NaN` = false, so it returns `(NaN, false)`. `local_time` is NaN
     from that tick onward and never recovers. `advance_stack`
     (`stack.rs:172-181`) carries the byte-identical arm.
  2. **`weight`** — `sample_blended_transform`'s per-layer skip is
     `let ew = layer.effective_weight() * clip.weight; if ew < 0.001 { continue; }`
     (`stack.rs:332-334`, repeated at `:378-380`). `NaN < 0.001` is **false**, so
     a NaN-weighted layer is *not* skipped; `total_weight` becomes NaN, the
     `total_weight < 0.001` early return at `:363` is likewise false, and the
     blended position / rotation / scale come out NaN.

  `find_key_pair` (`crates/core/src/animation/interpolation.rs:14-46`) does not
  rescue either: it handles ±inf correctly (`time <= time_at(0)` / `time >=
  time_at(last)` clamp to an endpoint) but a NaN `time` fails **both**
  comparisons, falls into the binary search, and emits `t = (NaN - t_lo) / dt`
  = NaN. There is no `is_finite` check anywhere between there and the GPU:
  `grep -rn "is_finite\|is_nan" byroredux/src/systems/animation.rs
  byroredux/src/render/skinned.rs crates/core/src/ecs/components/transform.rs`
  returns nothing.

  The affected import path is the one that matters: `import_sequence` is what
  `import_kf` calls for **both** standalone `.kf` files and embedded
  `NiControllerManager` sequences (`crates/nif/src/anim/entry.rs:53`, `:76`).
  The *other* path, `import_embedded_animations`, is already immune — it derives
  duration from key times behind a `> 0.0` guard (`entry.rs:645`), which is
  precisely the shape the sequence path lacks.
- **Evidence**:

  Producer — no finiteness gate on either field:
  ```rust
  // crates/nif/src/anim/sequence.rs:20-23
  let duration = seq.stop_time - seq.start_time;
  let cycle_type = CycleType::from_u32(seq.cycle_type);
  let frequency = seq.frequency;
  let weight = seq.weight;
  ```
  Both `seq.stop_time`/`seq.start_time` and `seq.weight` are bare
  `stream.read_f32_le()?` reads (`crates/nif/src/blocks/controller/sequence.rs:318`,
  `:360-368`).

  Boundary — the gap is visible in three consecutive lines:
  ```rust
  // byroredux/src/anim_convert.rs:504-520
  AnimationClip {
      name: nif.name.clone(),
      duration: nif.duration,                          // ← unvalidated
      cycle_type,
      // #3258 — `NiControllerSequence.frequency` is raw file data …
      frequency: sanitized_clip_frequency(nif.frequency),
      weight: nif.weight,                              // ← unvalidated
  ```

  Float semantics verified by execution rather than by reading, since every one
  of these is a NaN-comparison inversion:
  ```
  f32::MIN - f32::MAX = -inf   finite=false
  (0.25f32 + 0.1).min(-inf)    = -inf
  NaN <= 0.0                   = false     // fold_reverse_time's only guard
  (0.35f32).rem_euclid(2.0*NaN)= NaN,  NaN > NaN = false
  NaN < 0.001                  = false     // sample_blended_transform's skip
  ```
- **Impact**: A `.kf` or embedded sequence carrying a non-finite `stop_time`/
  `start_time` pair (or a literal NaN `weight`) poisons the affected entity's
  bone transforms permanently — `Transform` → `GlobalTransform` → the
  `GpuInstance` model matrix and the bone palette. NaN on the GPU is UB by this
  project's own severity rules. Corrupt or hostile archive content is the
  realistic source, which is exactly the reachability #3258 was accepted on.
  Rated MEDIUM to match #3258's own label rather than escalated.
- **Related**: #3258 (the fix that stopped one field short), #3194 (the same
  NaN-transparency class on the SpeedTree wind field), SAFE-2026-08-27-02 / #3373
  (the same "a later field was added past the sanitiser" shape in `Material`)
- **Suggested Fix**: Sanitise both at the same boundary `frequency` already uses.
  `duration`: reject non-finite (and negative) to `0.0`, which every cycle arm
  already treats as "no wrap / no fold". `weight`: reject non-finite to `1.0`,
  nif.xml's own default. Then make the gates NaN-safe rather than
  NaN-transparent — `if !(ew >= 0.001) { continue; }` and
  `if !(duration > 0.0) { return (0.0, false); }` — so a future producer cannot
  reopen it.

---

#### SAFE-2026-08-27b-02: pre-`10.1.0.106` `NiControllerSequence` defaults `cycle_type` to `0` = `CYCLE_LOOP` where nif.xml specifies `CYCLE_CLAMP` (`= 2`), and its comment names the wrong constant

- **Severity**: MEDIUM
- **Dimension**: 8 (animation) — NIF parse mismatch (`_audit-severity`: MEDIUM)
- **Location**: `crates/nif/src/blocks/controller/sequence.rs:310-332`; mapping at `crates/nif/src/anim/types.rs:35-42`; spec at `/mnt/data/src/reference/nifxml/nif.xml:1024-1026`, `:4218`, `:4221-4222`
- **Status**: NEW
- **Description**:

  For `stream.version() < V10_1_0_106` the `NiControllerSequence` fields are
  absent and the parser substitutes literals. Its own comment states what those
  should be:

  > Defaults are nif.xml's own (`weight` 1.0, `frequency` 1.0,
  > `cycle_type` **CYCLE_CLAMP = 0**, `start_time` FLT_MAX, `stop_time` FLT_MIN)

  `CYCLE_CLAMP` is **not** `0`. nif.xml's `CycleType` enum is
  `CYCLE_LOOP = 0` / `CYCLE_REVERSE = 1` / `CYCLE_CLAMP = 2`
  (`nif.xml:1024-1026`), and the block's stated default *is* `CYCLE_CLAMP`
  (`nif.xml:4218`). The engine's own `CycleType::from_u32` agrees with nif.xml
  (`0 => Self::Loop`), so the substituted `0` is decoded as **Loop**. Every
  `NiControllerSequence` in the `10.0.1.0 ≤ v < 10.1.0.106` window therefore
  plays looping where the format says clamp — the comment asserting they are
  the same value is what makes it look correct.

  The same `else` branch has a second, coupled property. `start_time` defaults
  to `f32::MAX` and `stop_time` to `f32::MIN`; both match nif.xml
  (`#FLT_MAX#` = `3.402823466e+38`, `#FLT_MIN#` = **`-3.402823466e+38`**,
  `nif.xml:82-83`). But `import_sequence` then computes
  `duration = stop_time - start_time` = `f32::MIN - f32::MAX`, which **overflows
  to `-inf`** (verified by execution, see previous finding's evidence block).
  Today the wrong `cycle_type` masks it: the `Loop` arm gates on
  `if clip.duration > 0.0`, which is false, so nothing wraps and `local_time`
  stays finite. Correcting `cycle_type` to `2` **alone** routes these clips into
  the `Clamp` arm, `(local_time + delta).min(-inf) = -inf` on the first tick,
  and every such clip freezes at key 0 (`find_key_pair` clamps `-inf` to the
  first key). The two must be fixed together.
- **Evidence**:
  ```rust
  // crates/nif/src/blocks/controller/sequence.rs:328-332
  let cycle_type = if has_ctlr_seq_fields {
      stream.read_u32_le()?
  } else {
      0                       // ← decoded as CYCLE_LOOP, not CYCLE_CLAMP
  };
  ```
  ```rust
  // crates/nif/src/anim/types.rs:35-42 — agrees with nif.xml, not with the comment
  pub fn from_u32(v: u32) -> Self {
      match v {
          0 => Self::Loop,
          1 => Self::Reverse,
          2 => Self::Clamp,
          _ => Self::Clamp,
      }
  }
  ```
  ```xml
  <!-- nif.xml:1024-1026 -->
  <option value="0" name="CYCLE_LOOP">Loop</option>
  <option value="1" name="CYCLE_REVERSE">Reverse</option>
  <option value="2" name="CYCLE_CLAMP">Clamp</option>
  <!-- nif.xml:4218 -->
  <field name="Cycle Type" type="CycleType" default="CYCLE_CLAMP" since="10.1.0.106" />
  <!-- nif.xml:82-83 -->
  <default token="#FLT_MAX#" string="3.402823466e+38" />
  <default token="#FLT_MIN#" string="-3.402823466e+38" />
  ```
  The version window is live rather than theoretical: `NiControllerSequence` is
  "Root node in Gamebryo .kf files (version 10.0.1.0 and up)" (`nif.xml:4215`),
  and `crates/nif/src/version.rs:674-686` carries a deliberate
  *"old Oblivion" (v10.0.x)* layout predicate family (#1337). **Honest limit**:
  I did not census how many `NiControllerSequence` blocks in the supported
  titles actually land below `10.1.0.106`, so the blast radius is code-provable
  but not measured.
- **Impact**: Clips in the pre-`10.1.0.106` window play with the wrong cycle
  semantics (loop instead of clamp) — a visible animation defect on old-Oblivion
  content, and one that a naive one-line "fix the constant" turns into frozen
  poses because of the `-inf` duration in the same branch.
- **Related**: SAFE-2026-08-27b-01 (the `duration` half), #1337 (the v10.0.x
  layout family), #687 (the last envelope-field misalignment in this parser)
- **Suggested Fix**: Substitute `2` (`CYCLE_CLAMP`) and correct the comment to
  name nif.xml's actual numbering. In the same change, gate `duration` in
  `import_sequence` — `let duration = (seq.stop_time - seq.start_time); let
  duration = if duration.is_finite() && duration > 0.0 { duration } else { 0.0 };`
  — so the corrected `Clamp` arm sees a sane envelope. Add a unit test that
  parses a `< 10.1.0.106` sequence and asserts `CycleType::Clamp` **and** a
  finite duration, so the two cannot be separated again.

---

### LOW

#### SAFE-2026-08-27b-03: `sanitize_finite`'s new whole-struct pin is a hand-typed field list, so it cannot catch the defect class its own doc-comment claims

- **Severity**: LOW (test-coverage gap; the code is correct today)
- **Dimension**: 9 (NIFAL boundary — NaN/Inf on the GPU)
- **Location**: `crates/core/src/ecs/components/material.rs:1970-2070` (`sanitize_finite_leaves_no_non_finite_float_anywhere`)
- **Status**: NEW — the residual of SAFE-2026-08-27-02 / #3373 (CLOSED, fixed in `59b85565`)
- **Description**: #3373's fix is complete: a mechanised diff of `struct
  Material`'s float fields against the `fix_scalar!`/`fix_vec!` calls now shows
  **33 float fields, 33 covered, 0 missing**. The prior report also asked for a
  durable guard, and one was added — but its doc-comment overstates what it does:

  > This is the guard that catches the #3373 defect *class* — a float field
  > added to `Material` without a matching `fix_scalar!`/`fix_vec!` line —
  > rather than only the four fields that were missing this time. **Extend the
  > literal below whenever `Material` gains a float.**

  The two halves of that sentence contradict each other. The test poisons a
  hand-written list of 33 field initialisers and then re-reads a hand-written
  list of 33 accessors. A field added to `Material` without a `fix_scalar!` line
  is added without a test line by the identical omission — the test's own
  instruction admits the maintenance burden it was supposed to remove. #3373 was
  exactly that omission (four fields added on 2026-08-25, sanitiser not
  extended), so the guard does not close the loop that produced it.

  This codebase already has the right instrument for a Rust-side structural
  invariant: `crates/renderer/src/shader_constants.rs` and
  `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:1002-1046` both use
  `include_str!` source scans to pin properties that are invisible to ordinary
  unit tests.
- **Evidence**: verified mechanically — a script extracting `pub <name>: f32 |
  [f32; N]` from `struct Material` and `fix_(scalar|vec)!\((\w+)\)` from
  `sanitize_finite` reports `float fields: 33 / MISSING: [] /
  covered-but-not-a-float-field: []`, i.e. the *code* is complete while the
  *test* remains a literal transcription of that same list.
- **Impact**: None today. The next `Material` float — the BGEM/Bethesda material
  surface has grown four times in the last month (300 → 348 → 364 → 396 → 432 B)
  — can silently reopen the same hole, and the report that closed #3373 will
  read as if it were guarded.
- **Related**: #3373, #2687 (the finding that created `sanitize_finite`)
- **Suggested Fix**: Replace the literal with an `include_str!("material.rs")`
  source scan: extract every `f32` / `[f32; N]` field name from the `struct
  Material` block, extract every `fix_scalar!`/`fix_vec!` argument from
  `sanitize_finite`, and assert set equality. That is what the doc-comment
  already promises, and it needs no maintenance.

---

#### SAFE-2026-08-27b-04: `#2771`'s source-scan pin cannot see the one remaining `(f + 1) % MAX_FRAMES_IN_FLIGHT` — the fence wait every synchronous GPU destroy depends on

- **Severity**: LOW (latent; correct at today's `MAX_FRAMES_IN_FLIGHT == 2`, and a bump is `const_assert`-gated)
- **Dimension**: 5 (Vulkan spec / sync) + 3 (drop ordering)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1626`; guard at `crates/renderer/src/shader_constants.rs:504-543`; contract at `crates/renderer/src/vulkan/sync.rs:8-49`
- **Status**: NEW — residual of #2771 (CLOSED, fixed in `f8eee12a`)
- **Description**: `f8eee12a` replaced `(f + 1) % N` with the general
  `(f + N - 1) % N` previous-slot form in `taa.rs`, `svgf.rs` and `restir.rs`,
  and added `temporal_history_indexing_uses_the_general_previous_slot_form` to
  keep it that way. The commit message states the pins "cover every file in
  their class". They do not — a repo-wide sweep finds one production site left:

  ```rust
  // crates/renderer/src/vulkan/context/draw.rs:1624-1637
  let prev = (frame + 1) % super::super::sync::MAX_FRAMES_IN_FLIGHT;
  self.device.wait_for_fences(
      &[self.frame_sync.in_flight[frame], self.frame_sync.in_flight[prev]],
      true, u64::MAX,
  ).context("wait_for_fences")?;
  ```

  Two independent reasons the pin misses it: `context/draw.rs` is not in the
  test's four-file list, **and** the needle is the literal string
  `"+ 1) % MAX_FRAMES_IN_FLIGHT"`, which cannot match this site's fully-qualified
  `% super::super::sync::MAX_FRAMES_IN_FLIGHT` spelling. Adding the file to the
  list would not fix it.

  This is also the site with the largest blast radius of the family, because it
  is not only a temporal-history read. The both-fences wait is what makes
  "the GPU is idle with respect to every prior submission" true at this point,
  and three separate synchronous-destroy arguments cite it as their safety
  premise — `pending_skin_unload_victims` and `pending_morph_unload_victims`
  (`skinned_blas_refit.rs:728-736`, `:795-801`: *"released NOW (post-fence-wait,
  so no in-flight command buffer still references the output buffer)"*) and the
  deferred-destroy tick. At `N == 3` the pattern covers 2 of 3 slots and leaves
  the immediately-previous frame unwaited, at which point those destroys become
  use-after-free rather than merely aliasing history.

  `sync.rs:45-49`'s `const_assert!(MAX_FRAMES_IN_FLIGHT == 2)` is a real gate
  and is why this is LOW rather than higher — a bump cannot happen silently.
  But `sync.rs:36-41` enumerates the two remedies that would let it be relaxed,
  so relaxing it is a contemplated change, which is the exact argument #2771 was
  accepted on.
- **Evidence**: `grep -rn "+ 1) % MAX_FRAMES_IN_FLIGHT\|+ 1) % super::super::sync::MAX_FRAMES_IN_FLIGHT" crates/renderer/src byroredux/src`
  returns three production hits: the two `shader_constants.rs` needle strings,
  `draw.rs:1626` (above), and `draw.rs:3953`
  (`self.current_frame = (self.current_frame + 1) % MAX_FRAMES_IN_FLIGHT`, which
  is a *next*-slot advance and correct as written).

  Sibling doc-rot found in the same read: `sync.rs:28` cites the double-fence
  wait as living at *`context/draw.rs:108-120`*. That range now holds
  `camera_frame_deltas`' doc comment; the wait is at `:1624-1638`. The line
  reference is the one thing a reader follows to check the load-bearing claim.
- **Impact**: None at `MAX_FRAMES_IN_FLIGHT == 2`. Under a future sync-tier
  raise, the guard added specifically to prevent this class would pass while the
  highest-consequence instance of it silently regressed.
- **Related**: #2771, #870 (`sync.rs`'s `== 2` contract), #282 (the double-fence
  wait), #1003 / #643 / #2494 (the synchronous-destroy sites that cite it)
- **Suggested Fix**: Either wait on **all** `MAX_FRAMES_IN_FLIGHT` fences here
  (`&self.frame_sync.in_flight[..]`), which makes the site N-agnostic and is
  remedy (b) from `sync.rs:40-41` anyway, or rename `prev` to `other_slot` with
  an explicit "correct only at N == 2, gated by sync.rs" note. Either way, widen
  the pin's needle to a regex over `\+ 1\) % (?:[\w:]+::)?MAX_FRAMES_IN_FLIGHT`
  and add `context/draw.rs` to its file list, and refresh `sync.rs:28`'s line
  reference.

---

#### SAFE-2026-08-27b-05: the debug server is a **default** cargo feature and its accept loop spawns an uncapped OS thread per connection

- **Severity**: LOW (hardening; loopback binding is the mitigation that keeps it here)
- **Dimension**: un-owned subsystem — `crates/debug-server` (`_audit-common`'s coverage table: *"a TCP listener that evaluates queries against the live `World`; nothing audits its command surface"*)
- **Location**: `byroredux/Cargo.toml:8` (`default = ["debug-server"]`); `crates/debug-server/src/listener.rs:158`, `:185-230`
- **Status**: NEW — no issue and no prior audit finding covers it; `crates/debug-server` appears in the 2026-08-16 / 2026-08-20 tech-debt reports only as a named scope gap.
- **Description**: `spawn` binds `TcpListener::bind(("127.0.0.1", port))` — the
  loopback binding is correct and is the reason this is not higher. What is
  unbounded is what happens after `accept()`: every connection gets its own
  named OS thread (`thread::Builder::new().name(format!("byro-debug-client-{addr}")).spawn(...)`)
  with no concurrent-connection cap, no accept rate limit, and no
  authentication. `active_streams` is pruned opportunistically, but it only
  tracks `Weak` handles for shutdown teardown — it never refuses a connection.

  The reason this is worth a line rather than nothing is `byroredux/Cargo.toml:8`:
  `debug-server` is in `default`, so an ordinary `cargo build --release`
  produces a binary that listens. The command surface behind it mutates the live
  `World` (`setav`/`modav`, `script.activate`, `door.teleport`, debug cell loads)
  and writes screenshots to disk, so a local process — not a remote one — can
  drive the engine and can also exhaust its thread budget.
- **Evidence**:
  ```rust
  // crates/debug-server/src/listener.rs:158
  let listener = TcpListener::bind(("127.0.0.1", port))?;
  // :224-230 — no cap between accept and spawn
  thread::Builder::new()
      .name(format!("byro-debug-client-{}", addr))
      .spawn(move || handle_client(stream_arc, q, s))
  ```
  ```toml
  # byroredux/Cargo.toml:7-9
  [features]
  default = ["debug-server"]
  debug-server = ["dep:byroredux-debug-server"]
  ```
- **Impact**: A local process opening connections in a loop exhausts the thread
  budget and can wedge the engine. Not remotely reachable — the loopback bind is
  what bounds this. Filed as hardening, and as the first finding of any kind
  against this crate's command surface.
- **Related**: #3007 (the last debug-listener bind defect), #1009 / #1172 (the
  `active_streams` shutdown side channel this sits next to)
- **Suggested Fix**: Cap concurrent clients (an `AtomicUsize` incremented before
  spawn, decremented in `handle_client`'s exit; refuse and close past ~8), which
  is a few lines inside the critical section that already exists. Separately,
  decide deliberately whether `debug-server` should remain in `default` once
  there is a shipping profile — and record that decision next to the feature.

---

## PASS — verified intact

Recorded so the next sweep does not re-derive them. Everything here was
re-checked against the current tree in this pass, not carried forward.

### Dimension 1 — FFI lifetime
- `crates/cxx-bridge/src/lib.rs` is still the placeholder: one
  `unsafe extern "C++"` block exposing `native_hello() -> String`. No `*const`,
  `&[u8]`, `Box<…>`, or reference-taking signature. Scope guard holds.
- `crates/fsr3-sys`: read end to end. `Context::create` / `Context::dispatch`
  both carry `# Safety` sections with the device-outlives-context and
  handles-belong-to-this-device contracts; `Drop`'s SAFETY comment
  cross-references `create`'s idle clause (the #2692 fix, still in place); every
  free-function `unsafe` block has its own comment. `Context` holds a
  `NonNull<RawContext>` and no `unsafe impl Send/Sync` exists anywhere in the
  crate, so it stays `!Send`/`!Sync` — no cross-thread aliasing of the opaque
  context is possible. `vendored_sdk_contract_tests` still pins the three
  control-flow properties the renderer's dispatch-failure recovery depends on.
- Ruffle / wgpu (`crates/ui`): still **zero** `unsafe` — the only grep hit is a
  log string in `navigator.rs:484`.

### Dimension 2 — Memory corruption / UB
- `QueryRead` / `QueryWrite` / `ComponentRef` (`crates/core/src/ecs/query.rs:58-64`,
  `:128-143`, `:255-289`): guard field precedes the cached pointer in every
  struct, each SAFETY comment still matches the layout, and `&mut *self.storage`
  is still gated on `&mut self`. The raw pointer keeps all three `!Send`/`!Sync`.
- `NifStream::read_pod_vec` (`crates/nif/src/stream.rs:438-470`) — `checked_mul`
  + `check_alloc` + sealed `AnyBitPattern` + big-endian compile gate, all intact.
  Its header mirror `read_pod_vec_from_cursor` (`crates/nif/src/header.rs:367-397`)
  changed this week: **#2272 (`bf5cc041`) moved the byte-budget guard inside the
  helper** (`check_header_alloc`), so a new caller can no longer forget it. Verified.
- `OpCode::from_u8` (`crates/pex/src/opcode.rs:130-137`): re-counted mechanically
  — **51 variants, `Nop = 0` the only explicit discriminant** (therefore
  contiguous `0..51`), `#[repr(u8)]`, `MAX_OPCODE = 51`, range check before the
  transmute. Both preconditions hold.
- `BuiltinType::from_u32` (`crates/sfmaterial/src/types.rs`): still a checked
  `match` with an `Err(Error::UnsupportedBuiltin { raw })` arm. No transmute.
- SSE skin-partition indices (`crates/nif/src/import/mesh/sse_recon.rs:100-160`,
  rewritten by #3355/#3360 this week): the new global-index reading bounds every
  index against `decoded.positions.len()` before pushing, and drops the whole
  triangle otherwise. No out-of-range index can reach the index buffer or the BLAS.

### Dimension 3 — Leaks & drop ordering
- **#3372's new state machine**: `deferred_compaction` is set only on the chunked
  branch (`mesh.rs:1269`), taken only at swap-in (`:1459`), and both
  publish-immediately paths (`:1232-1236`, `:1283-1285`) apply the plan before
  returning. A mid-copy `copy_bytes_range` error propagates with **both**
  `geometry_rebuild` and `deferred_compaction` retained, so the next frame
  resumes the same job rather than stranding compacted pools against stale
  offsets. `destroy()` (`:1685`) drops an in-flight job's buffers explicitly.
  The new `scene_geometry_resident` predicate additionally holds post-plan
  latecomers (`handle >= plan.mesh_count`) out of raster/TLAS. Fix is sound.
- **Synchronous unload-victim destroy** (`skinned_blas_refit.rs:728-736`,
  `:795-801`): the safety premise checks out — `draw_frame` waits
  **both** `in_flight` fences (`draw.rs:1624-1638`), which at
  `MAX_FRAMES_IN_FLIGHT == 2` is every slot, so no in-flight command buffer can
  reference a `SkinSlot`/`MorphSlot` freed later in the same call. (The
  `N == 3` caveat is SAFE-2026-08-27b-04.) `#3374`'s hoist is in place and pinned
  by `morph_eviction_drain_sits_outside_the_skin_compute_accel_guard`.
- **Deferred-destroy queue**: exactly three production instantiations, matching
  the skill's inventory — `mesh.rs:356/426` (vertex/index buffers) and
  `acceleration/mod.rs:208/225/317-318` (BLAS entries + BLAS scratch).
- **TLAS resize wait (#1390)**: `acceleration/tlas.rs:988` still calls
  `device_wait_idle()` before freeing the old allocation.
- **LOD availability memo (#3385, new this week)**: `lod_terrain_available` /
  `lod_object_available` are keyed `(level, qx, qy)` with no worldspace
  component, which would be a stale-answer hazard if a `WorldStreamingState`
  outlived a worldspace change. It cannot: `wctx` is assigned once in
  `WorldStreamingState::new` (`streaming.rs:778`) and never reassigned, and
  `drain_streaming_state` takes the whole state. Growth is bounded by the
  worldspace's quad ladder. Hypothesis tested and **disproved**.
- `WaterDisturbanceScratch` (#3257) *is* installed on the real world
  (`boot.rs:531`), not only in its own test — the take/restore round trip has no
  early return between the two halves.

### Dimension 4 — Unsafe-block discipline
Mechanised sweep of every `.rs` under `crates/` + `byroredux/src` + `tools/`,
excluding `unsafe fn` / `unsafe impl` / `unsafe trait` / `unsafe extern`
declarations, over a −25/+5-line window: **718 `unsafe {` blocks, 718 with a
SAFETY comment, zero gaps.** The wide/forward window is load-bearing — a
backward-only scan reports 101–157 false positives because this codebase
routinely writes the comment as the block's first line
(`vulkan/exposure.rs:73-75`, `vulkan/buffer.rs:1093`). Per #2692 no token-count
gap was chased.

### Dimension 5 — Vulkan spec (static only, no validation-layer evidence)
- `initialize_layouts` present on all seven storage-image passes: `bloom`,
  `gbuffer`, `caustic`, `svgf`, `water_caustic`, `taa`, `volumetrics`.
- `VOLUMETRIC_OUTPUT_CONSUMED` is `true` (`volumetrics.rs:546`) and the single
  call site gates on it by name (`context/post_passes.rs:498`).
- **#2768 re-verified across the whole family**: every compute dispatch grid now
  derives from the generated `WORKGROUP_X`/`WORKGROUP_Y`/`WORKGROUP_Z` —
  `taa`, `svgf` (both passes), `ssao`, `caustic`, `bloom` (×3), `volumetrics`
  (×2) — and `grep` finds no surviving `div_ceil(8)` literal in the crate. The
  two skinning passes correctly use the distinct `SKIN_WORKGROUP_SIZE = 64`,
  matching `skin_palette.comp` / `skin_vertices.comp`.
- **#2769 re-derived rather than taken on trust**: `build_tlas_instances` stamps
  `last_used_frame` at `tlas.rs:536` (skinned) and `:556` (rigid), both strictly
  *before* the `instance_map` lookup at `:563` — so the deleted second walk was
  genuinely redundant. The only statement between the stamp pass and the deleted
  loop is `ensure_tlas_state(..)?`, which in the old code also short-circuited
  the loop, so the `?` path is unchanged too.
- **#2519 ordering checked**: `signal_temporal_discontinuity` sets
  `svgf_recovery_frames` (`context/mod.rs:1975-1988`) from inside
  `record_upscale_pass`, which runs at `draw.rs:3682` — *after* the SVGF α state
  machine consumed it at `:3534`. The latch therefore lands on the next frame,
  which is what the fix intends. The added `.expect("frame upscaler must exist
  while recording")` duplicates one two lines above it, so it adds no new panic
  surface.
- **Needs runtime confirmation** (`BYRO_VALIDATION=1` or RenderDoc), out of reach
  for a static sweep: per-frame image-layout transitions across the bloom /
  volumetric / caustic mip sets.

### Dimension 6 — R1 material table
- `GpuMaterial` is **432 B**, pinned by `gpu_material_size_is_432_bytes`
  (`vulkan/material.rs:1494-1495`) — test name and asserted size agree.
- Rust ↔ GLSL diffed by script, expanding multi-name GLSL declarations:
  **108 Rust scalars, 108 GLSL scalars, exact one-to-one order match** after
  snake→camel normalisation. 108 × 4 = 432. No `[f32; 3]` anywhere in the
  struct, so no std430 vec3 hazard.
- All five `GpuInstance` GLSL copies (`include/bindings.glsl`, `triangle.vert`,
  `ui.vert`, `water.vert`, `caustic_splat.comp`) expand to the same scalar-slot
  count as each other; the Rust side is additionally pinned by
  `scene_buffer/gpu_instance_layout_tests.rs`.
- `MAX_MATERIALS = 16384` intern cap (`material.rs:1360`) and `upload_materials`'
  `debug_assert` + `.min(MAX_MATERIALS)` clamp (`scene_buffer/upload.rs:652-657`)
  are still in lockstep.

### Dimension 7 — RT IOR refraction
`triangle.frag`, `shader_constants_data.rs` and `include/shader_constants.glsl`
are **unchanged** in `c6c8ba55..HEAD`. Recorded as PASS-by-reference to
`AUDIT_SAFETY_2026-08-27.md`'s Dimension 7 rather than re-derived; the guards
there (`MAX_REFRACT_PASSTHRUS = 8`, the adaptive `refractPassthruBudget`, the
`materialKind` passthrough key, `GLASS_RAY_BUDGET` lockstep, Frisvad,
`DBG_VIZ_GLASS_PASSTHRU = 0x80`) cannot have regressed without a diff.

### Dimension 8 — NPC / animation spawn
- `#772` `FLT_MAX` sentinel and `#790` case-insensitive clip dedup: files
  unchanged in this delta; PASS-by-reference.
- `MAX_TOTAL_BONES` / `SkinSlotPool` overflow warn: unchanged; PASS-by-reference.
- **#3258 (`bbfd742f`) verified as landed**: `finite_time_delta` gates the
  `dt * speed * frequency` product in **both** `advance_time` (`player.rs:118`)
  and `advance_stack` (`stack.rs:159`), and `sanitized_clip_frequency` resolves
  the file-data half at the translate boundary. The two fields it did *not*
  cover are SAFE-2026-08-27b-01.

### Dimension 9 — NaN/Inf on the GPU
- `Material::sanitize_finite` coverage re-derived mechanically: **33 float
  fields, 33 covered, 0 missing** — #3373's fix is complete. Only the test-shape
  residual (SAFE-2026-08-27b-03) is open.
- `translate_material` still seeds the NaN sentinel and `resolve_pbr` is still
  its only detector/clamp; `sanitize_finite` calls `resolve_pbr` first
  (`material.rs:1106`) so the sentinel path and the repair path stay consistent.
- **`crates/sdk` + `byroredux/src/studio_host.rs` (audited for the first time)**:
  the SDK is a live surface, not a contract — `studio_host::apply_command` writes
  `StudioCommand::SetMaterial` straight onto the ECS `Material`. It is gated:
  `valid_material` (`studio_host.rs:172-177`) rejects any non-finite
  `diffuse_color`/`metalness`/`roughness`/`alpha`/`ior` **before** the write, and
  each field is then `clamp`ed to a sane range; `valid_transform` does the same
  for the transform command. `AssetBounds::from_spheres` and `pick_spheres`
  (`crates/sdk/src/studio.rs`) both skip non-finite inputs, and
  `CornellFit::around` falls back to a unit box on a non-finite envelope. The
  crate contains no `unsafe`. Clean.
- `crates/facegen` (un-owned): both parsers bound every file-driven allocation
  *before* it happens — `egt.rs:100-135` caps `width`/`height` at
  `MAX_TEXTURE_DIM`, caps `num_morphs` at `MAX_MORPHS`, `checked_mul`s the pixel
  count, and requires `bytes.len() == needed` exactly; `egm.rs:107-135` mirrors
  it with `MAX_VERTICES`/`MAX_MORPHS`. No unbounded `with_capacity` reachable.
- `crates/hkx` (un-owned): `decode_spline_animation`
  (`animation.rs:129-157`) validates every dimension before allocating —
  `transform_count`/`float_count`/`num_blocks` ≤ 4096, `sample_count =
  transform_count × frame_count` `checked_mul`'d and capped at
  `MAX_TRANSFORM_SAMPLES`, `duration`/`frame_duration` finiteness-checked, and
  `mask_size == transform_count * 4 + float_count` enforced — which is exactly
  what makes the raw slice `&data[block_start..block_start + transform_count * 4]`
  (`:204`) provably in bounds via the `mask_end > float_start` check at `:201`.
  Zero `unsafe`; that absence is real.

### Dimension 10 — debug-ui / egui overlay
`crates/debug-ui/` and `crates/renderer/src/vulkan/egui_pass.rs` are unchanged
in `c6c8ba55..HEAD`. PASS-by-reference to `AUDIT_SAFETY_2026-08-27.md`'s
Dimension 10 (teardown-first ordering, one-frame `pending_free` defer, the
`set_textures`-scoped queue mutex).

### Dimension 11 — Sandboxed mod runtime
Whole crate read (1183 LOC). Still no engine consumer; not reported as a finding.
- **No `unsafe` anywhere** — re-confirmed by grep. That absence is the property.
- **WASI absent at the manifest**: `crates/mod-runtime/Cargo.toml` depends only
  on `thiserror` + `wasmtime`, and the workspace `wasmtime` entry is
  `default-features = false` with no `wasi` feature.
- **Capability gating**: `logging::Host::log` (`runtime.rs:264-270`) checks
  `self.grants.contains(LOG_CAPABILITY)` and `bail!`s — an error to the guest,
  not a silent no-op. `context::Host`'s two functions expose only the guest's own
  identity and its own grant set, so they need no capability of their own.
- **Per-instance isolation**: each `ModInstance` owns its own `Store<HostState>`
  with its own `Principal`, `CapabilitySet` and `StoreLimits`. The shared
  `Engine`/`Linker` are immutable after `SandboxRuntime::new`. No `static`,
  `lazy_static`, `OnceLock` or shared `Arc<Mutex<…>>` in the crate.
- **Resource limits**: `Config::consume_fuel(true)` + `max_wasm_stack`;
  `StoreLimitsBuilder` bounds memory, tables, table elements, instances and
  memories with `trap_on_grow_failure(true)`; fuel is re-armed at every
  `enter()` and a trap or exhaustion routes to `quarantine`.
- **Lifecycle**: `initialize` requires `Ready`, `shutdown` requires `Active`, and
  `quarantine` is terminal — a faulted instance can never be re-entered, so a
  trapping guest cannot be retried in a loop.
- **Log DoS bounded three ways** (per-message bytes, entry count, `checked_add`
  total bytes, `runtime.rs:271-289`). #3050 (no *drain*) and #3051 (no
  hostile-bytes `compile` test) are still OPEN; noted, skipped.

### Un-owned: debug server / protocol
`TcpListener::bind(("127.0.0.1", port))` — loopback only, verified
(`listener.rs:158`); `tools/byro-dbg` defaults to the same host. The shutdown
side channel (`active_streams` `Weak` registry, #1009/#1172) still folds the
post-accept shutdown check into the push critical section. The one gap found is
SAFE-2026-08-27b-05.

---

## Next step

```
/audit-publish docs/audits/AUDIT_SAFETY_2026-08-27b.md
```
