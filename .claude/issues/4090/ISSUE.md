# #4090 — INC-2026-09-09-05

`cargo clippy` is red at HEAD — `crates/renderer` under `--all-targets`, and CI's own `--workspace -- -D warnings` on `byroredux-core`

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_INCREMENTAL_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4090 --json state`).

---

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
