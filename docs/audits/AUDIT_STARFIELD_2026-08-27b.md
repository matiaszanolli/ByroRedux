# Starfield Compatibility Audit — 2026-08-27b

**Repo**: `/mnt/data/src/gamebyro-redux` · **Branch**: `main` @ `969d81c8`
**Type**: Depth/correctness re-audit of the Starfield bring-up surface (all 9 dimensions, single-auditor static analysis + real-data probes)
**Priors reconciled**: [`AUDIT_STARFIELD_2026-08-24.md`](AUDIT_STARFIELD_2026-08-24.md) (0 findings) and [`AUDIT_STARFIELD_2026-08-27.md`](AUDIT_STARFIELD_2026-08-27.md) (7 findings) — **all 7 of the latter were filed as #3391–#3397 and 6 are already fixed on `main`**; this pass re-verifies those fixes and audits the delta they created.
**Game data**: present (`/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/`, 186 entries) — every real-data check below ran this session.

---

## Executive Summary

Starfield remains a first-class `GameKind`. The headline numbers reproduced
their baseline for the **third consecutive audit**: NIF parse rate
**89,270 / 89,276 = 99.9933%** across the five vanilla mesh archives, zero hard
failures. Nothing regressed in the shipped bring-up surface, and the six fixes
that landed since the last pass (`caa14cc5`, `969d81c8`) were each verified
correct against the code and, where measurable, against real data.

**Total: 5 new findings — 0 CRITICAL · 0 HIGH · 2 MEDIUM · 3 LOW.**

This pass is deliberately weighted toward the two things the previous report
could not do: it **closes three of that report's own admitted coverage gaps
with measurements**, and it **characterises the residual NIF truncation tail**
that two prior audits recorded as unexplained.

### The three prior coverage gaps, now closed by measurement

1. **The non-ASCII `.mesh` name corpus scan** the 2026-08-27 report named as
   "the highest-value follow-up measurement in the report" (it would have
   escalated the `canonical_mesh_path` panic to CRITICAL). **Run: 882,072
   external mesh names across all five archives, zero non-ASCII.** The HIGH
   finding's blast-radius bound was correct. But the same scan falsified the
   *other* half of that premise — see `SF-2026-08-27b-D2-01`.
2. **The `starfield_tail` length distribution** ("quoted from in-tree
   docstrings, not measured"). Measured: uniformly **30 B**, not the 38 B the
   field docstring still asserts — see `SF-2026-08-27b-D6-01`.
3. **The `shader_type` histogram on retail Starfield BLSPs** ("the cheapest way
   to settle #3364's parse-side half"). Measured: **`shader_type == 0` on all
   406,100 retail `BSLightingShaderProperty` blocks**, in every archive. #3364's
   parse-side half cannot fire on vanilla Starfield. Recorded as informational,
   appended rather than re-filed.

A fourth gap — "Dim 8 only spot-checked the collision slice's dispatch arms
without verifying the matching `resolve_shape` arms" — was checked and is
**clean**: all 16 `bhk*Shape` dispatch arms in `crates/nif/src/blocks/mod.rs`
have a matching `downcast_ref` arm in
`crates/nif/src/import/collision/shape.rs`, `BhkMultiSphereShape` (`:110`) and
`BhkConvexListShape` (`:235`) included. No finding.

### The one thing to fix first

`SF-2026-08-27b-D7-01` (**MEDIUM**): the six residual `Starfield -
MeshesPatch.ba2` truncations — carried as an *unexplained* family across two
prior audits and the #746/#747 chain — are **all six `BSWeakReferenceNode`, all
bsver 175, all failing in the same water-reference loop**. They are the
remainder of #2105/#2201, not a distinct cause. Five of the six stop with
`consumed == block_size − 10` and a byte-identical 10-byte run
(`01 00 | <u16> | cb c0 1a 00 | 00 00`) sitting where the parser expects
`[2-B gap][unk_int1][num_water_refs]`; a byte-identical clean sibling in the
same directory has that run absent. That converts "6 unexplained" into a
bounded, reproducible, single-block-type gap with a byte-level trace.

### Cross-cutting, not Starfield-specific

`SF-2026-08-27b-X-01` (**MEDIUM**): `cargo clippy --workspace -- -D warnings`
— the CI gate at `.github/workflows/ci.yml:94` — is **red on `main`**. Two
warnings, both introduced on 2026-08-27 by audit-fix commits. Reported here
because it blocks every gate including the Starfield ones; flagged explicitly
as likely also visible to `/audit-esm` so a duplicate is easy to reconcile.

---

## Dimension Findings

| Dimension | New | Severity | Status |
|---|---|---|---|
| 1 — BA2 v2/v3 LZ4 block decompression | 1 | 1 LOW | #3392/#3393/#3394 fixes verified correct; `lz4_flex` pin confirmed byte-identical to upstream `default`; one doc-comment misattachment left behind |
| 2 — BSGeometry mesh extraction | 1 | 1 LOW | #3391 fix verified; corpus scan confirms 0 non-ASCII names but **falsifies** the "vanilla is bare stems only" premise |
| 3 — CDB material database correctness | 0 | — | #3395 fix verified (ROADMAP repointed at the new #3398); parser hardening (`index_chunks`, `probe_header`, `peek_magic`, `#2614` reserve cap) all intact |
| 4 — Starfield ESM resolve-rate baseline | 0 | — | #2636 SECH/AOPF dispatch reviewed; neither enters `statics`, so the 91.2% baseline is unchanged by construction |
| 5 — ESM + cell bring-up regression surface | 0 | — | `XCLL_SIZES_STARFIELD = [28, 108]`, the PDCL named skip, and the `EsmIndex` category wiring all verified present |
| 6 — NIF shader blocks, BSVER 155+ | 1 | 1 LOW | #1510 + #1606 guards GREEN on real data (0 `NiUnknown` BLSPs); field docstring is 8 bytes stale |
| 7 — Real-data validation | 1 | **1 MEDIUM** | Baseline reproduced exactly; #3397's tightened floor confirmed correct; residual tail characterised |
| 8 — NIFAL canonical material translation | 0 | — | `pack_imported_material_flags` derivations intact; collision dispatch↔resolve parity verified clean |
| 9 — BGSM/BGEM external material flow | 0 | — | No loose `.bgsm`/`.bgem` in any Starfield archive (prior census); `merge_external_material` signature still narrow |
| X — cross-cutting build gate | 1 | **1 MEDIUM** | CI clippy gate red on `main` |

---

### MEDIUM

#### SF-2026-08-27b-D7-01: the six residual MeshesPatch truncations are all `BSWeakReferenceNode` — the #2105 tail is characterised, not unexplained

- **Severity**: MEDIUM
- **Dimension**: 7 — Real-data validation (with root-cause in Dim 6's block-parser territory)
- **Location**: `crates/nif/src/blocks/node.rs:887-952` (`BsWeakReferenceNode::parse_inner` — the `SF_WEAK_REF_GAP` skip at `:936-938`, `unk_int1` at `:941`, and the water-reference loop at `:944-949`)
- **Status**: NEW. #2105 and #2201 are both CLOSED/COMPLETED; no open issue tracks the residual. This is the remainder of #2105's fix, not a regression of it.
- **Description**: Two prior audits recorded the six residual `Starfield -
  MeshesPatch.ba2` truncations as a stable-but-unexplained family, explicitly
  "distinct unexplained cause from the closed `BSWeakReferenceNode` / cloth /
  `BSShaderType155` tails". They are not distinct. **All six are
  `BSWeakReferenceNode`**, all at `user_version_2 == 175` (i.e. at-or-above
  `SF_WEAK_REF_GAP`, so the #2105 2-byte skip *is* applied), and all six drop to
  `NiUnknown` inside the same water-reference loop.
- **Evidence** (measured this session, `Starfield - MeshesPatch.ba2`):

  | File | block | size | consumed | failure |
  |---|---|---|---|---|
  | `meshes\terrain\cydoniacity\objects\cydoniacity.4.-2.-2.nif` | 0 | 150 324 | 150 314 | `skip(80)` past EOF |
  | `meshes\terrain\sb004templeworld\objects\sb004templeworld.1.-1.0.nif` | 1 | 14 764 | 14 754 | `skip(80)` past EOF |
  | `meshes\terrain\lc174world\objects\lc174world.1.0.1.nif` | 1 | 208 | 174 | `skip(1634533376)` |
  | `meshes\terrain\cydoniacity\objects\cydoniacity.8.-6.-6.nif` | 0 | 302 052 | 302 042 | `skip(80)` past EOF |
  | `meshes\terrain\cydoniacity\objects\cydoniacity.1.-1.-1.nif` | 1 | 14 284 | 14 274 | `skip(80)` past EOF |
  | `meshes\terrain\cydoniacity\objects\cydoniacity.2.-2.-2.nif` | 0 | 35 860 | 35 850 | `skip(80)` past EOF |

  1. **Five of six stop at exactly `block_size − 10`.** `80 = 64 + 12 + 4` is
     the water-reference struct skip (`node.rs:948`), so the parser read
     `num_water_refs` as a non-zero garbage value 10 bytes before the block end.
  2. **The 10 bytes are byte-regular across files.** Hex at the block tail,
     immediately after the last weak-ref entry's `num_materials = 0`:
     ```
     cydoniacity.1.-1.-1.nif   01 00  33 00  cb c0 1a 00  00 00 | 00 00 00 00 00 00 00 00 00 00
     cydoniacity.2.-2.-2.nif   01 00  6a 00  cb c0 1a 00  00 00 | 00 00 00 00 00 00 00 00 00 00
     cydoniacity.4.-2.-2.nif   01 00  d8 00  cb c0 1a 00  00 00 | 00 00 00 00 00 00 00 00 00 00
     ```
     `u16 = 1`, then a per-file-varying `u16`, then the **constant** `u32
     0x001AC0CB` (1 753 291) in all three sampled files, then two zero bytes.
     The `[2-B gap][unk_int1 = 0][num_water_refs = 0]` triple then lands
     exactly on the block end, which is self-consistent.
  3. **A clean sibling proves the run is conditional, not universal.**
     `meshes\terrain\cydoniacity\objects\cydoniacity.4.-6.2.nif` — same
     directory, same `user_version_2 = 175`, same `BSWeakReferenceNode` — has
     only `[gap 2][unk_int1 4][num_water_refs 4] = 10` bytes after
     `num_materials`, no 10-byte run, and parses clean.
  4. **The sixth file diverges earlier.** `lc174world.1.0.1.nif` attempts
     `skip(1634533376)`; `1634533376 == 0x616D0000`, i.e. the ASCII bytes
     `\0 \0 m a` — the parser is misaligned *inside* a `materials\…`
     null-terminated string in the `UnkMaterialStruct` loop
     (`read_past_cstring`, `node.rs:966-976`). A distinct sub-mode of the same
     block type, worth separating in any fix.
- **Attempts to disprove** (all failed): the files are not corrupt (the BA2
  extract succeeds and the header `block_sizes` sum + 8-byte footer accounts for
  the file exactly); the #2105 gate is not mis-applied (all six are bsver 175,
  the gate's own attested boundary, and removing the skip moves the misread
  *further* off); the outer recovery is working as designed (`truncated ==
  false`, `dropped_block_count == 0`, only `recovered_blocks == 1`), so this is
  content loss, not stream corruption.
- **Impact**: Six `BSWeakReferenceNode` blocks — Starfield's composite-LOD /
  packin reference nodes — are replaced with `NiUnknown`, so their entire weak-
  reference payload (the terrain-object LOD placements) is dropped. Four of the
  six are **Cydonia** terrain-object LOD tiles, i.e. the audit's flagship
  walkable cell. Blast radius is bounded and non-fatal (6 / 29 849 files, and
  the parse-rate gate at 99.5% still passes at 99.98%), but the family is now
  actionable rather than mysterious.
- **Related**: #2105 (the 325 → 6 fix), #2201 (its `SF_WEAK_REF_GAP` correction),
  #1882 (the +2 B opaque tail on the same block), #746/#747 (the original
  mis-attribution this closes out).
- **Suggested Fix**: Do **not** guess the field's semantics. Two safe steps:
  (a) make the water-reference loop defensive — bail to the block-size boundary
  rather than issuing a `skip()` that provably exceeds it, so the block keeps
  its `NiNode` base and children instead of collapsing to `NiUnknown`;
  (b) byte-audit the 10-byte run against nifly's `BSWeakReference` to determine
  whether it is a conditional per-entry field or a block-level one, using the
  clean/failing sibling pair above as the differential.
- **Labels**: `medium` · `bug` · `nif-parser` · `nif` · `game:starfield` · `legacy-compat`

---

#### SF-2026-08-27b-X-01: the CI clippy gate is red on `main` — two warnings, both from 2026-08-27 audit-fix commits

- **Severity**: MEDIUM
- **Dimension**: cross-cutting (build gate). **Not Starfield-specific** — reported here because it gates every Starfield check; a concurrent `/audit-esm` or `/audit-tech-debt` may raise the same item.
- **Location**: `.github/workflows/ci.yml:94` (the gate); `crates/plugin/src/esm/records/outfit.rs:71-92`; `byroredux/src/cell_loader/object_lod.rs:250-259`
- **Status**: NEW
- **Description**: CI runs `cargo clippy --workspace -- -D warnings`. On
  `969d81c8` the workspace emits **two** warnings, so that command exits
  non-zero and the gate fails. Both were introduced the same day, by commits
  whose whole purpose was closing audit findings.
- **Evidence**: `cargo clippy --workspace` on `969d81c8`:
  ```
  warning: you seem to be trying to use `match` for an equality check. Consider using `if`
    --> crates/plugin/src/esm/records/outfit.rs:71:9      [clippy::single_match]
  warning: this function has too many arguments (8/7)
    --> byroredux/src/cell_loader/object_lod.rs:250:1     [clippy::too_many_arguments]
  warning: `byroredux-plugin` (lib) generated 1 warning
  warning: `byroredux` (bin "byroredux") generated 1 warning
  ```
  Attribution by `git log -- <file>`: `outfit.rs` last touched by `fa71f1a2`
  ("Fix #3356: INAM is one array of FormIDs…", 2026-08-27) — the fix collapsed
  the `INAM` handling to a single `match` arm plus `_ => {}`, which is exactly
  the `single_match` shape. `object_lod.rs` last touched by `c7a70d45`
  ("Fix #3385: memoise the distant-LOD archive-presence probe", 2026-08-27),
  which pushed `spawn_object_lod_quad` from 7 to 8 parameters.
  No other crate in the workspace emits a warning — these two are the whole gate
  failure.
- **Attempts to disprove**: the gate is not `--all-targets`, so the (larger)
  crop of warnings in tests and `_tmp_*` examples is genuinely out of scope and
  does not muddy this; both warnings are on-by-default lints, not pedantic ones;
  the repo has no `#![allow]` covering either site, and no `clippy.toml`
  raising `too-many-arguments-threshold`. There is **no** `cargo fmt --check`
  job, so the unrelated rustfmt drift in these files is correctly not a gate
  failure.
- **Impact**: Every PR and every push to `main` fails CI until fixed. The
  second-order cost is worse than the first: a permanently-red `-D warnings`
  gate is the standard way a workspace learns to ignore clippy, and this one is
  the only static-analysis gate the repo has.
- **Related**: #3356 (`fa71f1a2`), #3385 (`c7a70d45`).
- **Suggested Fix**: `if sub.sub_type == *b"INAM" { … }` at `outfit.rs:71`
  (the clippy suggestion preserves the comment block); for
  `spawn_object_lod_quad`, bundle `(qx, qy)` or the `world`/`ctx`/`tex_provider`
  triple into a struct, matching the `Dx10TexInfo` precedent in
  `crates/bsa/src/ba2.rs:857-863` that was introduced for this exact lint.
- **Labels**: `medium` · `bug` · `tech-debt` · `esm-plugin` · `terrain-exterior`

---

### LOW

#### SF-2026-08-27b-D2-01: `canonical_mesh_path`'s "vanilla is unaffected" premise is false — 13,713 vanilla FaceGen names are already fully composed

- **Severity**: LOW
- **Dimension**: 2 — BSGeometry mesh extraction (documentation / premise)
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:28-32` (the doc comment); corroborating block-level doc at `crates/nif/src/blocks/bs_geometry.rs:213-216`
- **Status**: NEW (doc/premise only — the code is correct)
- **Description**: `canonical_mesh_path`'s doc comment justifies #2361's
  head/tail-detection fix with:
  ```rust
  /// A name already carrying `geometries\` and/or `.mesh` composed into
  /// `geometries\geometries\x.mesh.mesh` — a guaranteed miss, and a silent one
  /// until #2357 gave the resolve-miss path a log. Vanilla is unaffected (every
  /// sampled `.mesh` name is a bare 20-hex stem); this is authoring-tool output
  /// and mods that use readable paths.
  ```
  The parenthetical is false against retail data. A full scan of every
  `BSGeometry` external mesh name in all five vanilla mesh archives finds
  **13,713 already fully composed names**, all of them in
  `Starfield - FaceMeshes.ba2` — that is **100% of that archive's external mesh
  names**, and the `.mesh` files really do live at exactly the composed path in
  the same archive.
- **Evidence** (measured this session):

  | Archive | external names | non-ASCII | already headed | already tailed | name length |
  |---|---|---|---|---|---|
  | Meshes01 | 430 418 | 0 | 0 | 0 | 41 (uniform) |
  | Meshes02 | 0 | — | — | — | — |
  | MeshesPatch | 388 982 | 0 | 0 | 0 | 41 (uniform) |
  | LODMeshes | 48 959 | 0 | 0 | 0 | 41 (uniform) |
  | **FaceMeshes** | **13 713** | 0 | **13 713** | **13 713** | **57 (uniform)** |

  Sample, from `meshes\actors\character\facegendata\facegeom\starfield.esm\0026fdf0.nif`:
  `"Geometries\\526277e35270101cf88e\\9b0d60d3a60db8befad9.mesh"` — and
  `geometries\526277e35270101cf88e\9b0d60d3a60db8befad9.mesh` is present in
  `Starfield - FaceMeshes.ba2` (6 114 entries, 4 832 of them under
  `geometries\`). Lookup case is not an issue: `Ba2Archive::extract` normalises
  through `normalize_path` (`crates/bsa/src/ba2.rs:1089-1091`), which lowercases,
  so the capitalised `Geometries\` head resolves.
- **Impact**: No runtime defect — the current code takes the `(true, true)`
  arm and passes the name through verbatim, which is right. The cost is
  epistemic and pointed: pre-#2361 this composition produced
  `geometries\Geometries\<hash>\<hash>.mesh.mesh` for **every vanilla Starfield
  FaceGen head-geometry reference**, so #2361 was a vanilla-content fix, not the
  mods-only hardening its own comment describes. A future reader deciding how
  carefully to guard that helper is reading an understatement. The same
  parenthetical is also what the 2026-08-27 report used to bound #3391's blast
  radius — the *conclusion* there survives (0 non-ASCII names, confirmed above),
  but its stated premise does not.
- **Related**: #2361 (`61520a39`), #1292, #2357, #3391 (CLOSED), and the
  concurrently-filed `BSFaceGenNiNode` under-read on the same 1,417 facegen head
  nodes.
- **Suggested Fix**: Replace the parenthetical with the measurement — the
  vanilla corpus is 868,359 bare 41-char stems (Meshes01/02/Patch/LODMeshes)
  **plus 13,713 fully-composed 57-char paths in `FaceMeshes.ba2`**, so both
  arms of the head/tail test are exercised by vanilla content. Consider adding a
  seventh unit test using a real FaceMeshes-shaped name.
- **Labels**: `low` · `documentation` · `doc-rot` · `nif-parser` · `game:starfield`

---

#### SF-2026-08-27b-D6-01: the `starfield_tail` field docstring still describes a 38-byte tail; the corpus tail is 30 bytes and starts two floats later

- **Severity**: LOW
- **Dimension**: 6 — NIF shader blocks, BSVER 155+ (documentation)
- **Location**: `crates/nif/src/blocks/shader.rs:753-765` (the `starfield_tail` field docstring on `BSLightingShaderProperty`)
- **Status**: NEW
- **Description**: The field docstring is the only in-repo description of what
  those undecoded bytes *are* — the capture itself is deliberately opaque, and
  #2625 removed the drift telemetry that would otherwise contradict it. It says:
  ```rust
  /// byte-audited over
  /// `Starfield - LODMeshes.ba2` as **38 B = 9× f32 + 2 B**, byte-identical
  /// across all 26 LOD instances (`[2.0, 3.0, 0.1, 0.0, 0.02, 0.0289,
  /// 0.095, 0.003, 1.0, 0x00, 0x00]`)
  ```
  Measured today over the same archive: the tail is **30 B = 7× f32 + 2 B**,
  byte-identical across the same **26** instances, with value
  `[0.1, 0.0, 0.02, 0.0289, 0.09504, 0.00298, 1.0, 0x00, 0x00]`. That is the
  docstring's own literal **minus its first two floats** (`2.0, 3.0`) — a later
  parser fix reclaimed those 8 bytes into named fields and the docstring was
  never moved with it.
- **Evidence**:
  1. Dump of every non-empty `BSLightingShaderProperty::starfield_tail` in
     `Starfield - LODMeshes.ba2`: **one distinct tail, ×26** —
     `len=30 floats=[0.1, 0.0, 0.02, 0.0289, 0.09504, 0.00298, 1.0] rem=[0, 0]`.
     Same population size as the docstring's "all 26 LOD instances", so this is
     the same set of blocks, not a different one.
  2. The distribution is uniform corpus-wide — **2,538** full-body Starfield
     BLSPs carry a tail and **every one is 30 B**: Meshes01 1,879, MeshesPatch
     633, LODMeshes 26. Zero blocks at 38 B anywhere.
  3. The repo already knows this **30 lines away in another file**: the sibling
     test docstring at
     `crates/nif/src/blocks/shader_tests/starfield.rs:167-181` says
     "the real remaining opaque tail is corpus-verified at 30 B today". So the
     two authorities disagree, and the stale one is the one attached to the
     field a future decoder-writer would read. (That test docstring also carries
     its own arithmetic slip — "#2622 … reclaimed 16 of those 38 bytes" does not
     reach 30; the measured delta is 8 — worth correcting in the same edit.)
- **Attempts to disprove**: not an archive-version artefact — the same 30 B
  appears in Meshes01 and MeshesPatch too; not a parser bug —
  `read_starfield_tail` (`shader.rs:774-791`) consumes exactly
  `block_size − consumed` with no hardcoded length, so it is correct at any
  size and produces zero drift; not a sampling artefact — this is the complete
  population, not a sample.
- **Impact**: Documentation only, but of the class `_audit-common.md` calls out
  by name (the `GpuMaterial` 300 B → 348 B precedent): a wrong literal in a
  byte-layout contract is not a typo. Anyone writing the eventual decoder from
  this docstring would assign every field two slots off and read `2.0`/`3.0` for
  values the parser already consumed elsewhere.
- **Related**: #1606 (`497700e7`, the capture), #2622 (the reclaim), #2625
  (drift telemetry disabled, so nothing contradicts the docstring automatically),
  #3396 (OPEN — the sibling false-citation finding in the same file).
- **Suggested Fix**: Rewrite the field docstring to `30 B = 7× f32 + 2 B`,
  `[0.1, 0.0, 0.02, 0.0289, 0.09504, 0.00298, 1.0, 0x00, 0x00]`, note that 2,538
  corpus blocks (not 26) carry it and all are byte-identical, and reconcile the
  "reclaimed 16 of 38" arithmetic in `shader_tests/starfield.rs`. Zero code change.
- **Labels**: `low` · `documentation` · `doc-rot` · `nif-parser` · `game:starfield`

---

#### SF-2026-08-27b-D1-01: the #3393 fix orphaned the `#2097` panic-guard rationale onto a string helper

- **Severity**: LOW
- **Dimension**: 1 — BA2 LZ4 (delta review of `caa14cc5`)
- **Location**: `crates/bsa/src/ba2.rs:1695-1723` (the doc comment, now heading `prefix_up_to` at `:1714`) and `:1778` (`lz4_decompress_is_panic_guarded`, now undocumented)
- **Status**: NEW
- **Description**: `caa14cc5` inserted the new `prefix_up_to` helper **between**
  `lz4_decompress_is_panic_guarded`'s doc comment and the test it documents.
  The two comment blocks fused, so the file now reads:
  ```rust
  /// #2097 / LZ4-01 — pins that the LZ4 arm still routes through
  /// `catch_unwind`.
  ///
  /// … delete the `catch_unwind` and this test fails, which is the
  /// only way this fix can be kept from silently regressing on a future
  /// dependency bump — exactly the scenario the issue was filed about.
  /// #3393 — take at most `max_bytes` of `s`, backing up to the nearest
  /// char boundary.
  …
  fn prefix_up_to(s: &str, max_bytes: usize) -> &str {
  ```
  `prefix_up_to` is now documented as pinning a `catch_unwind` it has nothing to
  do with, and `lz4_decompress_is_panic_guarded` at `:1778` has **no doc comment
  at all**.
- **Evidence**: read directly from the current file; the fusion is visible in
  `git diff bbfd742f..HEAD -- crates/bsa/src/ba2.rs`, where the `#3393` block is
  added immediately after the existing `#2097` block with no separating item.
  The `#3392` and `#3393` test doc comments below it are correctly attached, so
  this is a single misplacement, not a systematic one.
- **Impact**: Cosmetic today. The cost lands later: `#2097`'s rationale — an
  explicit "this test exists so a future dependency bump cannot silently remove
  the guard" — is the sort of comment that gets deleted alongside a helper
  someone decides is unnecessary. The guard would survive; its reason would not.
- **Attempts to disprove**: not a rendering artefact of the diff — `rustdoc`
  and the raw file agree that both blocks precede `prefix_up_to`; not harmless
  by position — `prefix_up_to` is `#[cfg(test)]`-module-local and genuinely
  deletable, unlike the test.
- **Related**: #3393 (CLOSED, `caa14cc5`), #2097, #3392.
- **Suggested Fix**: Move the `#2097 / LZ4-01` block back down to immediately
  precede `#[test] fn lz4_decompress_is_panic_guarded`, leaving `prefix_up_to`
  with only its own `#3393` doc. Three-line move, no behaviour change.
- **Labels**: `low` · `documentation` · `import-pipeline` · `game:starfield`

---

## Prior-Pass Reconciliation

Both priors are reconciled here rather than re-filed.

**`AUDIT_STARFIELD_2026-08-24.md` (0 findings)** — its conclusion still holds:
nothing it examined has regressed. Its central observation, that the Starfield
entry-point files had seen no commits in its window, remains the reason it found
nothing.

**`AUDIT_STARFIELD_2026-08-27.md` (7 findings)** — all seven were published as
#3391–#3397. Verified this session:

| Finding | Issue | State | Verification |
|---|---|---|---|
| D2-01 `canonical_mesh_path` char-boundary panic | #3391 | CLOSED, `caa14cc5` | Byte-slice fix present at `bs_geometry.rs:56-58`; new `non_ascii_names_compose_instead_of_panicking` test covers `"модель"` + lossy `[0xFF,0xFF]`. **Corpus scan confirms 0 non-ASCII names in 882,072 vanilla external mesh names**, so the "not CRITICAL" bound the report asked to be checked is confirmed. |
| D1-01 `lz4_flex` feature not pinned | #3392 | CLOSED, `caa14cc5` | `Cargo.toml:151-157` now pins `default-features = false` + the five features. **Verified byte-identical to `lz4_flex 0.11.6`'s own `default`** by reading `~/.cargo/registry/.../lz4_flex-0.11.6/Cargo.toml:55-71` — the fix's claim to that effect is true. `crates/bsa/Cargo.toml:9` takes it via `workspace = true`, so the source-order pin governs the only dependent. |
| D1-02 fixed-byte source slicing in pin tests | #3393 | CLOSED, `caa14cc5` | `prefix_up_to` + its own boundary test replace both `&arm[..arm.len().min(2000)]` sites. Correct — but see `SF-2026-08-27b-D1-01` for what it left behind. |
| D1-03 hint `0` skipped on a false premise | #3394 | CLOSED, `caa14cc5` | Sweep now starts at `0`; comment corrected to say `checked_chunk_size_usize` *accepts* zero. |
| D3-01 ROADMAP cites closed #2359 | #3395 | CLOSED, `969d81c8` | `ROADMAP.md:1278-1279` now points at **#3398** (OPEN, verified via `gh`), with the repoint reason recorded inline. |
| D6-01 false nif.xml `#GTE# 155` citation | #3396 | **OPEN** | Unfixed and correctly still open. Not re-filed. The sibling docstring defect in the same file is filed separately as `SF-2026-08-27b-D6-01`. |
| D7-01 stale MeshesPatch floor | #3397 | CLOSED, `969d81c8` | `parse_real_nifs.rs:221` now `min_clean: 0.995`; docstring row refreshed to `99.98% actual`. **Independently re-measured at 29 843/29 849 = 99.9799% → 99.98%.** The fix is numerically correct. |

**Not re-filed, per scope**: the `BSFaceGenNiNode` 2-byte under-read
(`crates/nif/src/blocks/mod.rs:314-325`), the corpus-gate coverage gap, and the
CDB `.mat` texture-role items — all owned by concurrent audits this cycle.

---

## Informational Observations (not filed)

Recorded so they are not rediscovered. **Do not file without new evidence.**

- **#3364's parse-side half is settled for Starfield, and it cannot fire.**
  Every retail Starfield `BSLightingShaderProperty` has `shader_type == 0` —
  **406,100 blocks, 100%**, across all five archives (Meshes01 189,801;
  MeshesPatch 153,683; LODMeshes 48,903; FaceMeshes 13,713; Meshes02 0). The
  2026-08-27 report named this histogram "the cheapest way to settle #3364's
  parse-side half"; it is now measured. `parse_fo76_plus`'s ungated
  `parse_shader_type_data_fo76` dispatch (`shader.rs:1291`) can only ever reach
  its `0` arm on vanilla Starfield. Worth appending to #3364, not raising its
  severity.
- **The BSEffectShaderProperty +32 B tail — frequency now measured.** The known,
  deliberately-scoped-out follow-up affects **831 blocks**: Meshes01 665,
  MeshesPatch 118, LODMeshes 48. All are exactly 32 B and byte-identical within
  an archive. Not re-filed (the skill says note frequency only).
- **`material_reference` dominates completely.** 403,562 of 406,100 Starfield
  BLSPs (99.4%) take the `.mat` material-reference short-circuit; only 2,538 are
  full-body. This is the quantitative case for CDB Phase 2 (#3398) being the
  top Starfield fidelity item — essentially all vanilla Starfield material data
  is behind the CDB, not in the NIF.
- **Inline `BSGeometry` geometry does not exist in vanilla.**
  `has_internal_geom_data()` was true for **0** blocks across all five archives,
  confirming `crates/nif/src/blocks/bs_geometry.rs:217-221`'s claim. Consequence:
  `extract_bs_geometry`'s Stage A exit is a bare `?` with no log
  (`bs_geometry.rs:93-102`) while #2357 gave all three Stage B exits one — a
  real diagnostic asymmetry, but on a path vanilla never takes. Not filed;
  worth a one-line `log::debug!` if that code is touched for another reason.
- **Collision dispatch↔resolve parity is clean.** All 16 `bhk*Shape` dispatch
  arms (`crates/nif/src/blocks/mod.rs:1195-1270`) have a matching
  `downcast_ref` arm in `crates/nif/src/import/collision/shape.rs:87-300`,
  including the two Starfield-relevant ones, `BhkMultiSphereShape` (`:110`) and
  `BhkConvexListShape` (`:235`). This discharges the standing dispatch↔resolve
  parity rule for this crate and closes the prior pass's Dim 8 audit gap.
- **#2636's SECH/AOPF capture cannot move the resolve rate.** Both land in the
  new `EsmIndex::sound_echoes` / `audio_occlusion_primitives` maps
  (`index.rs:293-305`), never in `statics`, so the 25,433 / 27,898 = 91.2%
  Cydonia baseline is unchanged **by construction** — the 220 REFRs pointing at
  them were unresolved before and remain unresolved, now with attribution. No
  engine launch was needed to establish this.
- **Repo is not `rustfmt`-clean, and that is not a gate.** `rustfmt --check`
  diffs several untouched files (`records/dispatch_actor.rs`,
  `records/misc/quest.rs`, `tests/common/mod.rs`) as well as the delta's
  `examples/sf_smoke.rs` line-length rewrap. There is **no** `cargo fmt` job in
  `.github/workflows/ci.yml` and no `rustfmt.toml`, so none of this is a gate
  failure. Recorded so a future auditor does not file it as one.

---

## Existing Open Issues Re-confirmed (not re-filed)

`#3398` (CDB Phase 2 — the new tracker, replacing closed #2359) · `#3396`
(BSEffect `#GTE# 155` false citation, still open) · `#3364`
(`BSShaderType155` FO76-only translation — parse-side half now measured, see
above) · `#3230` (CDB gate makes the BGSM/BGEM resolver unreachable) · `#2633`
(CDB duplicate field names last-wins) · `#1576` (model-less STAT/BNDS/ACTI/ARMO
drop). Open-issue baseline: 139 issues, fetched this session to
`/tmp/audit/issues.json`.

---

## CRC32 Flag Table

Unchanged from the 2026-08-27 pass and re-verified present: the complete
32-entry named table lives at `crates/nif/src/shader_flags.rs`
(`pub mod bs_shader_crc32`), pinned against the nif.xml `BSShaderCRC32`
literals by `bs_shader_crc32_matches_nif_xml_literals`. **The hashes are not
opaque.** Rather than reprint 32 rows verbatim, see
[`AUDIT_STARFIELD_2026-08-27.md`](AUDIT_STARFIELD_2026-08-27.md#crc32-flag-table)
— nothing in `shader_flags.rs` changed in this window.

One measurement worth adding to that table's context: because
`parse_fo76_plus` hardcodes `shader_flags_1: 0, shader_flags_2: 0`
(`shader.rs:1297-1298`), the CRC arrays are the **only** flag channel on
Starfield — and with `shader_type == 0` universal (above), CRC flags are also
the only per-material shader signal reaching the importer from the NIF side.

---

## Verification Method

All real-data checks run this session against the on-disk Starfield install.
No engine instance was launched (per the standing no-parallel-launch rule); no
`byro-dbg` attach was needed.

| Check | Method | Result |
|---|---|---|
| NIF parse rate, 5 vanilla mesh archives | purpose-built release probe over every `is_nif_entry` file | Meshes01 **31 058/31 058 = 100.00%** · Meshes02 **7 552/7 552 = 100.00%** · MeshesPatch **29 843/29 849 = 99.98%** · LODMeshes **19 535/19 535 = 100.00%** · FaceMeshes **1 282/1 282 = 100.00%** · aggregate **89 270/89 276 = 99.9933%**, **0 hard failures** — byte-identical to ROADMAP and to both prior passes |
| Residual truncation attribution | per-file header + block-type + recovery-log trace | **6/6 `BSWeakReferenceNode`, all `user_version_2 = 175`**; 5 fail `skip(80)` at `block_size − 10`, 1 fails `skip(1634533376)` inside a material cstring |
| Truncation byte-level differential | hexdump of failing vs clean sibling block tails | 10-byte run `01 00 <u16> cb c0 1a 00 00 00` present in all sampled failures, absent in `cydoniacity.4.-6.2.nif` |
| External `.mesh` name census | all 5 archives, every `BSGeometryMeshKind::External` | **882 072 names · 0 non-ASCII · 13 713 already headed+tailed (all FaceMeshes, 57 chars) · 868 359 bare 41-char stems** |
| Composed-path existence check | BA2 file-table lookup | `geometries\526277e35270101cf88e\9b0d60d3a60db8befad9.mesh` present in FaceMeshes.ba2 (4 832 `geometries\` entries of 6 114) |
| `starfield_tail` distribution | all 5 archives | BLSP: **2 538 tails, every one 30 B**, one distinct value; BESP: **831 tails, every one 32 B** |
| BLSP `shader_type` histogram | all 5 archives | **`{0: 406100}`** — 100% zero |
| Inline-geometry census | all 5 archives | **0** `has_internal_geom_data()` blocks |
| `lz4_flex` default-feature equivalence | read `lz4_flex-0.11.6/Cargo.toml:55-71` | `default = [std, safe-encode, safe-decode, frame, checked-decode]` — the workspace pin matches exactly |
| Workspace clippy (the CI gate) | `cargo clippy --workspace` | **2 warnings → gate red**; `--all-targets` adds only test/example noise |
| Unit tests | `cargo test -p byroredux-bsa -p byroredux-sfmaterial`; `-p byroredux-nif --lib`; `-p byroredux-plugin --lib` | bsa 68+16+6+1 · sfmaterial 1 · nif **1 118** · plugin **814** — **all green, 0 failed** |
| Issue dedup baseline | `gh issue list --limit 400` | 139 open issues → `/tmp/audit/issues.json` |
| Issue state spot-checks | `gh issue view` | #3391–#3395, #3397 CLOSED/COMPLETED · #3396 OPEN · #3398 OPEN · #746/#747/#2105/#2201/#1882 CLOSED/COMPLETED |

Probe binaries were written into `crates/{nif,bsa}/examples/_tmp_sf27b_*.rs`,
run, and **deleted** — the working tree carries no changes from this audit
beyond this report.

---

## Remaining-Work Chain

Per `docs/engine/starfield-esm-roadmap.md` (Phases 0+1 done, 2–4 invalidated by
the 99.9%-parity measurement). **Do NOT frame this as "BGSM parser first / ESM
very far" — both have shipped.**

1. **Per-field CDB extraction (CDB Phase 2)** — now correctly tracked at
   **#3398** (the #3395 fix repointed ROADMAP off the closed #2359). This pass
   quantified why it is first: **99.4% of all Starfield `BSLightingShaderProperty`
   blocks are `material_reference` stubs**, so essentially all vanilla material
   data lives in the CDB and currently reaches the Disney lobe as NIF defaults.
2. **Exterior worldspace tiles** — genuinely unimplemented scope, not a
   regression. Cydonia is an interior CELL and does not depend on it.
3. **Space-cell / planet / GBFM records** — `GBFM`/`GBFT`/`PNDT`/`STDT`/`BIOM`
   remain parser gaps; prior measurement found zero occurrences in Cydonia, so
   not load-bearing for the interior target.
4. **The NIF truncation tail** — **no longer "unexplained"**. All six are
   `BSWeakReferenceNode` at bsver 175 with a characterised 10-byte differential;
   see `SF-2026-08-27b-D7-01` for the byte-level trace and a two-step fix that
   does not require guessing the field's semantics.

---

## Coverage Note

Honest limits of this pass:

- **No windowed engine launch, no rendered-frame validation, no RenderDoc
  capture** (per the no-parallel-engine-launch rule). Every render-path
  conclusion is static analysis plus unit tests. Per the standing "no
  speculative Vulkan fixes" rule, no claim was made that would require one.
- **The ESM resolve rate was not re-run this pass.** The 91.2% Cydonia baseline
  is carried forward from two prior passes that measured it bit-for-bit
  identically; this pass establishes *analytically* (see Informational) that the
  only ESM delta in the window, #2636's SECH/AOPF capture, cannot move it.
  A future pass that can launch the harness should re-measure rather than
  inherit.
- **The 10-byte `BSWeakReferenceNode` differential was not decoded**, only
  characterised. Deriving field names/semantics from three samples would violate
  the no-guessing policy; the finding deliberately stops at the byte level and
  proposes a differential-based decode instead.
- **The 12 DLC/Creation CDBs were still not run through
  `ComponentDatabaseFile::parse` end-to-end** — the prior pass's gap here is
  carried, not closed. Only the base `materialsbeta.cdb` has ever been parsed
  fully.
- **The NIFAL particle slice was not traced on Starfield content**
  (`Starfield - Particles.ba2` was not walked). The collision-slice half of that
  prior gap *was* closed (dispatch↔resolve parity, above).
- **BGSM/BGEM byte-level version-branch offsets were not re-derived** against a
  real `.bgem`; the prior census established Starfield ships zero loose
  `.bgsm`/`.bgem`, so this is low-yield for this game specifically.
- Per `_audit-common.md`'s un-owned-subsystem list, this pass did not exercise
  Starfield content through FaceGen, the Mod Runtime sandbox, the SDK, or the
  Havok packfile reader. Starfield ragdolls remain blocked on the
  `BhkSystemBinary` blob decoder per `docs/engine/physal.md`, unchanged. Note
  that `SF-2026-08-27b-D2-01` touches the *archive* half of the Starfield
  FaceGen path; the morph/blend half in `crates/facegen` is still unowned.

No GitHub issues were created by this audit.
