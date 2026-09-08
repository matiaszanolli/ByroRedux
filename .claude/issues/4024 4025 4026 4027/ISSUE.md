=================== ISSUE #4024 ===================
STATE: OPEN
LABELS: documentation, renderer, low, doc-rot
TITLE: REN-2026-09-06-D23-02: ROADMAP's bench tracker still asserts the FSR harness is "byte-stable since `34074b93`" — three commits have touched the two harness files, one of them changing the reporter's arithmetic

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D23-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: FSR/Presentation (doc-rot, measurement integrity)
- **Location**: `ROADMAP.md`, the `R6a-stale-20` tracker entry — the phrase
  "**Harness still confirmed byte-stable**: no commit against
  `scripts/fsr-bench-matrix.sh` or `scripts/fsr_bench_report.py` since
  `34074b93`" in its **2026-09-01 (Session 77, HEAD `f9dd52b4`)** and
  **2026-09-03 (Session 79, HEAD `4d78dce6`)** fold paragraphs
- **Status**: NEW
- **Description**: The claim was true when first written (2026-08-19) and stayed
  true through the 2026-08-28 fold. It became false on 2026-08-28 and was then
  repeated twice. `git log 34074b93..HEAD` on the two files returns three
  commits, all ancestors of both `f9dd52b4` and `4d78dce6`:

  | Commit | Date | Files | Effect |
  |---|---|---|---|
  | `ff177576` | 2026-08-28 | `fsr-bench-matrix.sh` (+94/−2) | bench sanity gates (entity floor, state-hash rejection) |
  | `0e91fc5e` | 2026-08-28 | **both** (+130/−13) | adds the `gpu_inactive` TSV column and changes `fsr_bench_report.py`'s `render_sum` so brackets flagged inactive are **excluded** rather than summed as `0.000` |
  | `1293dfc0` | 2026-08-29 | `fsr-bench-matrix.sh` (+29) | adds the `gridcross` exterior scene definition (deliberately outside the default `SCENES`) |

  The 2026-09-01 paragraph names `0e91fc5e`'s own #2830 in its body ("Session 76
  changes an over-limit FSR render-extent from clamped to rejected") and then
  asserts the harness untouched since `34074b93` — the same commit did both.
- **Evidence**: The harness's own provenance stamp contradicts the claim
  directly. The two archived records:
  ```
  docs/audits/BENCH_stepped-camera_34074b93.tsv
    # harness=4de5e78e engine=34074b93 …          (23 columns, ends state_hash)
  docs/audits/BENCH_stepped-camera_2da754e7.tsv
    # harness=1293dfc0 engine=2da754e7 …          (24 columns, ends gpu_inactive)
  ```
  `git merge-base --is-ancestor` confirms all three harness commits precede
  `2da754e7`, `f9dd52b4` and `4d78dce6`.
- **Impact**: The tracker is the only place in the repo that records whether two
  bench records are comparable, and it currently licenses an apples-to-apples
  read of the 2026-08-14 and 2026-09-03 matrices that is not valid: the column
  set differs, the acceptance gates differ, and `render_sum` — the input to the
  "render rec." column — is computed differently. The practical damage is bounded
  because the **live** bench-of-record section (2026-09-03, `2da754e7`) does the
  right thing independently: it declines old-vs-new attribution outright ("The
  1059-commit gap is too large for an uncontrolled old-vs-new attribution").
  Hence LOW.
- **Related**: #2835 (the harness provenance stamp that makes this checkable),
  `0e91fc5e` (#2821, the `gpu_inactive` change), REN-2026-09-06-D23-03
- **Suggested Fix**: Replace the assertion in the last two fold paragraphs with
  the measured fact — three harness commits, what each changed, and that the two
  archived records therefore carry different `harness=` stamps and are not
  directly comparable. Going forward, derive the sentence from
  `git log <record>..HEAD -- scripts/fsr-bench-matrix.sh scripts/fsr_bench_report.py`
  at fold time rather than carrying it forward verbatim; the fold ritual copied
  this line through five updates unverified.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **TESTS**: A regression test pins this specific fix

=================== ISSUE #4025 ===================
STATE: OPEN
LABELS: documentation, renderer, low, doc-rot
TITLE: REN-2026-09-06-D23-03: `fsr_bench_report.py` discards the provenance line the harness writes for exactly this purpose, and stamps nothing of its own

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D23-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: FSR/Presentation (debug/telemetry)
- **Location**: `scripts/fsr_bench_report.py` (`main` — the
  `not line.startswith("#")` filter, and the per-scene `print` block);
  `scripts/fsr-bench-matrix.sh` (the `# harness=%s engine=%s …` `printf`)
- **Status**: NEW
- **Description**: #2835 added the `# harness=… engine=… mode=… camera=… runs=…
  frames=…` header to the TSV because "nothing in a committed table said which
  harness produced it". The tool that turns the TSV into the table people
  actually quote drops that line as metadata and never re-emits it, so the
  human-readable output still says nothing about harness or engine commit. The
  reporter also records no version of its own — and it is not a pure formatter:
  `0e91fc5e` changed `render_sum` so brackets named in `gpu_inactive` are
  excluded from the render-resolution sum instead of summed as measured zeros.
  The same TSV therefore yields different "render rec." figures before and after
  that commit, with nothing in the output distinguishing them.
- **Evidence**:
  ```python
  # main(): the provenance line is filtered out and never referenced again
  lines = [line for line in handle if line.strip() and not line.startswith("#")]
  ```
  The only per-scene header printed is
  `f"\n=== {scene} — {mode}/{camera}, {entities} entities, {runs} runs, median (min–max)"`
  — scene state, no provenance. Contrast the harness, which went to the trouble
  of computing `HARNESS_COMMIT` from
  `git log -1 --format=%h -- scripts/fsr-bench-matrix.sh`.
- **Impact**: Bench tables are pasted into `ROADMAP.md` and audit reports. A
  pasted table carries no way to tell which harness/engine produced it or which
  reporter computed its recovery columns — which is precisely the gap that let
  D23-02's stale byte-stability claim survive two folds unchallenged. Purely a
  measurement-hygiene issue, no runtime effect.
- **Related**: #2835, `0e91fc5e` (#2821), REN-2026-09-06-D23-02
- **Suggested Fix**: Echo the `#` provenance line(s) verbatim at the top of the
  report, and add a `report=<git log -1 --format=%h -- scripts/fsr_bench_report.py>`
  token next to them so both halves of the harness pair are stamped. The
  self-test already fixtures the `# harness=deadbeef engine=cafef00d` header, so
  the assertion is a one-line addition to the existing loop.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

=================== ISSUE #4026 ===================
STATE: OPEN
LABELS: documentation, renderer, low, shaders, doc-rot
TITLE: REN-2026-09-06-D23-04: the FSR plan's phase-3 status line still says exposure is "consumed by the composite tonemap" — contradicted five lines later in the same header

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D23-04), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: FSR/Presentation (doc-rot)
- **Location**: `docs/engine/fsr3-upscaler-integration-plan.md`, the status
  header's phase-3 paragraph ("…the 1×1 `R32_SFLOAT` exposure producer consumed
  by the composite tonemap…")
- **Status**: NEW
- **Description**: Phase 4 moved exposure and ACES out of composite into the
  output-resolution presentation pass, and the very next paragraph of the same
  header says so ("an output-resolution presentation pass that owns exposure +
  ACES"). The phase-3 sentence was never updated. It is checkable and wrong:
  `composite.frag` contains no `exposure` uniform and no `aces()` — verified by
  grep — and `composite.rs`'s single mention of exposure is a comment pointing
  the reader at `frame_upscaler.rs` / `exposure.rs`. The live consumer is
  `presentation.frag`'s `vec3 presented = aces(graded * params.exposure)`.
- **Evidence**: `grep -i "exposure\|aces" crates/renderer/shaders/composite.frag`
  returns only prose comments about pre-ACES linear space (composite's *output*
  is pre-tone-map by design); the sole `params.exposure` reader in the tree is
  `presentation.frag`.
- **Impact**: This is the authoritative FSR document, and exposure agreement
  between the upscaler and the tone-mapper is exactly the invariant #2833 was
  filed about (`NO_EXPOSURE_RESOURCE_FALLBACK`). A reader chasing an exposure
  mismatch is sent to the wrong shader. Documentation only.
- **Related**: #2833, `docs/engine/shader-pipeline.md` (which the SKILL's Dim 8
  bullet already records correctly: "ACES tone-map is NOT in `composite.frag` —
  it lives in `presentation.frag`")
- **Suggested Fix**: Change "consumed by the composite tonemap" to "consumed by
  the presentation tone-map (`presentation.frag`, since phase 4)". One clause.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix

=================== ISSUE #4027 ===================
STATE: OPEN
LABELS: bug, renderer, low, shaders, test-gap
TITLE: REN-2026-09-06-D3-03: the terrain-tile shift/mask is the last `GpuInstance.flags` bitfield hand-written shader-side, with no generated `#define` and no lockstep pin

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D3-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/constants.rs` (`INSTANCE_TERRAIN_TILE_SHIFT`, `INSTANCE_TERRAIN_TILE_MASK`), `crates/renderer/shaders/triangle.frag`, `crates/renderer/src/shader_constants_data.rs`
- **Status**: NEW
- **Description**: Every other packed field in `GpuInstance.flags` reaches GLSL through the generated `include/shader_constants.glsl` header: `INSTANCE_FLAG_NON_UNIFORM_SCALE`/`ALPHA_BLEND`/`CAUSTIC_SOURCE`/`TERRAIN_SPLAT`/`FLAT_SHADING`/`DIFFUSE_ALPHA` plus `INSTANCE_RENDER_LAYER_SHIFT`/`_MASK`. The terrain-tile window does not. The CPU packs it with the named constants (`f |= (tile_idx & INSTANCE_TERRAIN_TILE_MASK) << INSTANCE_TERRAIN_TILE_SHIFT` in `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`), while `triangle.frag` unpacks it with the literals `(inst.flags >> 16) & 0xFFFFu`. No `#define` is emitted and no test pins the two halves equal.
- **Evidence**:
  - `grep -n "TERRAIN" crates/renderer/shaders/include/shader_constants.glsl` returns only `#define INSTANCE_FLAG_TERRAIN_SPLAT 8u` — no shift, no mask.
  - `shader_constants_data.rs`'s own header comment *names* the two constants in prose ("the upper 16 bits pack the terrain-tile slot per `INSTANCE_TERRAIN_TILE_SHIFT/MASK`") while not mirroring them.
  - The identical defect for the render-layer bits was fixed by #2045 / TD7-101, whose comment reads: *"Previously hand-written as `INST_RENDER_LAYER_SHIFT`/`_MASK` directly in `triangle.frag` with no lockstep test, unlike every other `INSTANCE_FLAG_*` bit"*.
- **Impact**: No live drift — the values are 16 and `0xFFFF` on both sides today, and `instance_flag_bits_unique_and_outside_packed_windows` guards the CPU side against collisions. The gap is one-directional: a future widening of the tile window (`MAX_TERRAIN_TILES` is capped at 65535 *by this encoding*) would move the Rust constants and leave `triangle.frag` reading a stale window, indexing `terrainTiles[nonuniformEXT(…)]` with a truncated slot — wrong diffuse/normal/specular layers on every exterior cell, no test failure, no validation error. Same failure mode `gpu_terrain_tile_is_96_bytes`' doc describes for the sibling stride hazard.
- **Related**: #2045 / TD7-101 (the same fix for the render-layer bits); #470 (the encoding).
- **Suggested Fix**: Mirror `INSTANCE_TERRAIN_TILE_SHIFT` / `INSTANCE_TERRAIN_TILE_MASK` into `shader_constants_data.rs`, add them to `build.rs`'s emit and to `generated_header_contains_all_defines`, add an *instance_terrain_tile_bits_match_scene_buffer_consts* pin alongside the existing render-layer one, and replace the two literals in `triangle.frag`.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix

