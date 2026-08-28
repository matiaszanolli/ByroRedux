# Starfield Compatibility Audit — 2026-08-27

**Repo**: `/mnt/data/src/gamebyro-redux` · **Branch**: `main` @ `bbfd742f`
**Type**: Depth/correctness re-audit of the Starfield bring-up surface (9 dimensions, parallel agents)
**Previous pass**: [`AUDIT_STARFIELD_2026-08-24.md`](AUDIT_STARFIELD_2026-08-24.md) — 0 new findings
**Game data**: present (`/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/`) — every dimension had real-data validation available

---

## Executive Summary

Starfield remains a first-class `GameKind`: NIF + BA2 v2/v3 at the compat-matrix
rate, CDB + BGSM/BGEM materials, and a walkable Cydonia interior all ship today.
**Every headline number reproduced its baseline exactly** — NIF parse rate
89,270/89,276 (99.9933%), BA2 corpus 129/129 archives, CDB 97 classes /
1,438,780 instances, Cydonia ESM resolve rate 25,433/27,898 (91.2%). Nothing
regressed in the shipped bring-up surface.

**Total: 7 new findings — 0 CRITICAL · 1 HIGH · 1 MEDIUM · 5 LOW.**

The value of this pass is concentrated entirely in the **delta since
2026-08-24**. The prior pass found zero findings and correctly noted that the
Starfield entry-point files had seen zero commits in its window. That is no
longer true: `crates/bsa/src/ba2.rs`, `crates/nif/src/import/mesh/bs_geometry.rs`,
`crates/nif/src/blocks/shader.rs`, `crates/nif/src/shader_flags.rs`,
`byroredux/src/{material_translate,asset_provider/material}.rs`,
`crates/core/src/ecs/components/material.rs` and the ESM cell walkers all moved.
**Six of the seven findings are in code that landed in the last three days**, and
five of those are in the *fixes themselves* — the audit-driven repairs for
#2360, #2097, #2361, #2359 and #2105's floor each left a smaller defect behind.

### The one thing to fix first

`SF-2026-08-27-D2-01` (**HIGH**) is a hard, uncaught engine panic introduced by
`61520a39` (the #2361 mesh-path fix) two days ago. `canonical_mesh_path` tests
its `.mesh` suffix with a **byte-range slice on a `&str`**
(`bs_geometry.rs:48-49`), which panics on any multibyte UTF-8 boundary. The
input is untrusted archive data decoded *lossily* (`read_sized_string` →
`from_utf8_lossy`), the call site is on the **main thread** outside every
`catch_unwind` guard, and the workspace is `panic = "unwind"`. Reproduced
independently, twice, on both valid non-ASCII (`"модель"`) and lossy-decoded
invalid bytes. Vanilla Starfield is unaffected (all sampled names are bare
20-hex ASCII stems), so the blast radius is mods, authoring-tool output,
localized paths and corrupt archives — but the regression is asymmetric: what
was a silent resolve miss before the fix is now process termination. The repair
is a three-token change to match the byte-slice technique used by the `has_head`
test three lines above it.

### Cross-cutting pattern worth naming

`SF-2026-08-27-D2-01` (HIGH, production) and `SF-2026-08-27-D1-02` (LOW, test
infrastructure) are **the same defect class in two unrelated commits landed the
same week**: indexing a `&str` by a computed byte offset without a char-boundary
guard. Two independent dimension agents found them without knowledge of each
other. Both sites were written as deliberate reimplementations of a
byte-slice-safe technique that deviated at exactly the indexing line. This is
worth a lint (`clippy::string_slice` is available) rather than two point fixes.

### What was verified clean

Dimensions 4, 5, 8 and 9 returned **zero findings** against substantive delta
review, not silence:

- **Dim 4** — the seven ESM commits since the last audit, several touching the
  *shared* `build_static_object_from_subs`, produced **zero** Starfield drift.
  The #1567 LIGH `DAT2` canary still resolves exactly 656 Cydonia lights.
- **Dim 5** — all 13 spawn/cell invariants re-verified with line cites,
  including the structural BLAS exclusion (collider ghosts carry no
  `MeshHandle`) and PDCL's named skip, which is live rather than theoretical:
  `Starfield.esm` ships **706** top-level PDCL records. #3325's new `WMI1` REFR
  arm was falsified against real data — **zero** occurrences in the entire
  master, so it cannot fire.
- **Dim 8** — the glass-optics and soft/rim/back-lighting work
  (`d9d4a6d7`/`b80313f6`/`ceb69d24`) classifies **once** at
  `translate_material` and lowers to canonical `MAT_FLAG_*` bits. No per-game
  branch reached the GPU side. `GpuMaterial`'s growth to 432 B is fully pinned.
- **Dim 9** — all 11 `pack_imported_material_flags` derivations correct;
  `merge_external_material`'s signature still narrow (no NIFAL boundary
  violation); six hypotheses raised and disproved.

**Three dimensions independently converged** on the same disproof: the new
soft/rim/back-lighting extraction is gated `TextureSlotLayout::Skyrim`-only and
therefore unreachable on Starfield — which is **correct**, not a gap, because
nif.xml exposes those three names only as Skyrim SLSF2 bits 25/26/27 and the
32-entry `BSShaderCRC32` enum has no member for any of them. Dim 9 additionally
falsified the tempting "extend it to FO4" repair: FO4's SLSF2 bits 25/26/27 are
`Alpha_Test` / `Gradient_Remap` / `VATS_Target_Draw_All`, so widening the gate
would have *introduced* a misclassification. Recorded here so the next auditor
does not re-open it.

### Note on finding IDs

Dimensions used two ID shapes (`SF-D3-2026-08-27-01` vs
`SF-2026-08-27-D1-01`). All findings are normalized below to
**`SF-2026-08-27-D<dim>-<nn>`**.

---

## Dimension Findings

| Dimension | New | Severity | Status |
|---|---|---|---|
| 1 — BA2 v2/v3 LZ4 block decompression | 3 | 1 MEDIUM, 2 LOW | Real-data 129/129 clean; both delta fixes correct, but each left a smaller defect |
| 2 — BSGeometry mesh extraction | 1 | **1 HIGH** | Regression introduced by the #2361 fix, 2026-08-26 |
| 3 — CDB material database correctness | 1 | 1 LOW | Parser clean on real data; tracker doc-rot |
| 4 — Starfield ESM resolve-rate baseline | 0 | — | Baseline held bit-for-bit (91.2%) |
| 5 — ESM + cell bring-up regression surface | 0 | — | 5 delta commits reviewed, none regressed Starfield |
| 6 — NIF shader blocks, BSVER 155+ | 1 | 1 LOW | #1510 + #1606 guards GREEN; false spec citation |
| 7 — Real-data validation | 1 | 1 LOW | Baseline reproduced exactly; stale test floor |
| 8 — NIFAL canonical material translation | 0 | — | No boundary violation in the material delta |
| 9 — BGSM/BGEM external material flow | 0 | — | 6 hypotheses disproved; 3 informational notes |

---

### HIGH

#### SF-2026-08-27-D2-01: `canonical_mesh_path` panics on a non-ASCII `BSGeometry` mesh name (char-boundary slice)

- **Severity**: HIGH
- **Dimension**: 2 — BSGeometry mesh extraction
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:48-49` (call site `:134`)
- **Status**: NEW — introduced by `61520a39` (2026-08-26), *after* the 2026-08-24 pass found this dimension clean. No match in the 143 open issues.
- **Description**: The `has_tail` test slices the `&str` by byte range:
  ```rust
  let has_tail = mesh_name.len() > TAIL.len()
      && mesh_name[mesh_name.len() - TAIL.len()..].eq_ignore_ascii_case(TAIL);
  ```
  `mesh_name.len() - 5` is a **byte** index. If the last five bytes straddle a
  multibyte UTF-8 scalar, `Index<Range<usize>> for str` panics. The `has_head`
  test two lines above operates on `mesh_name.as_bytes()` and is safe; the tail
  test silently switched representation. The commit's own doc comment states the
  helper mirrors `normalize_mesh_path`'s technique — and
  `byroredux/src/asset_provider/archive.rs:96-118` is byte-slice-only end-to-end,
  therefore panic-free. The reimplementation deviated at exactly this one line.
- **Evidence**:
  1. `mesh_name` is untrusted archive input decoded **lossily**:
     `BSGeometryMesh::parse` reads it via `stream.read_sized_string()`
     (`crates/nif/src/blocks/bs_geometry.rs:241`), which falls back to
     `String::from_utf8_lossy` (`crates/nif/src/stream.rs:632-641`). Each invalid
     byte becomes a 3-byte U+FFFD. No ASCII validation on this path.
  2. Panic reproduced against a byte-identical copy of the function
     (`rustc -O`, standalone) — by the dimension agent, then **independently
     re-run by the orchestrator**:
     ```
     len=12 boundaries_ok=false
     panicked: start byte index 7 is not a char boundary; inside 'е' (bytes 6..8)   // "модель"
     panicked: start byte index 1 is not a char boundary; inside '\u{fffd}' (0..3)  // from_utf8_lossy(&[0xFF,0xFF])
     ascii: geometries\abc123.mesh                                                  // ASCII path correct
     ```
  3. It does **not** require corrupt data. Valid non-ASCII suffices: `"модель"`
     is 12 bytes with boundaries at 0,2,…,12; `12 - 5 = 7` is mid-char.
  4. **The panic is not caught.** `extract_bs_geometry` is reached from
     `import_nif_scene_with_resolver` (`crates/nif/src/import/walk/mod.rs:554`,
     `:1231`), called on the **main thread** from
     `byroredux/src/scene/nif_loader.rs:257`,
     `cell_loader/placement_lod.rs:489`, `cell_loader/object_lod.rs:279`,
     `cell_loader/terrain_lod_btr.rs:233`. The streaming worker's `catch_unwind`
     guards (`byroredux/src/streaming.rs:1118`, `:1153`) wrap only `parse_nif`
     plus the satellite walkers — mesh extraction is outside both.
     `panic = "unwind"` is workspace-wide (`Cargo.toml:254`).
  5. Reachability is gated only on a resolver being present
     (`bs_geometry.rs:123`), which the engine always supplies via
     `impl MeshResolver for TextureProvider`
     (`byroredux/src/asset_provider/texture.rs:81-85`).
- **Attempts to disprove** (all failed): no upstream ASCII/UTF-8 filter on
  `mesh_name`; no length or charset cap in `read_sized_string`; no
  `catch_unwind` on the main-thread import path; `has_head`'s byte-slice does
  not protect the tail test — they are independent `&&` chains and the tail test
  evaluates for every name ≥ 6 bytes regardless of head.
- **Impact**: Hard engine crash during Starfield cell load or loose-NIF load for
  any `BSGeometry` whose external mesh name is non-ASCII or invalid UTF-8 — the
  latter also covers a truncated/misparsed `.nif` whose `u32` length prefix
  lands on arbitrary bytes. Vanilla Starfield is unaffected, so blast radius is
  mods, authoring-tool output, localized paths and corrupt archives. Asymmetric
  regression: pre-`61520a39` this input produced a silent resolve miss;
  post-fix it terminates the process.
- **Related**: introduced fixing #2361; composition dates to #1292; miss-path
  logging is #2357. Same-class precedent: #854 (worker-thread panic guards), the
  `crates/bsa/src/ba2.rs:775` `catch_unwind` + its presence test at `:1685`.
  **Same defect class as `SF-2026-08-27-D1-02` below.**
- **Suggested Fix**: Make the tail test byte-wise, matching the head test three
  lines above:
  ```rust
  let has_tail = bytes.len() > TAIL.len()
      && bytes[bytes.len() - TAIL.len()..].eq_ignore_ascii_case(TAIL.as_bytes());
  ```
  Behaviour-preserving for all six existing tests (all ASCII). Add a seventh
  asserting a non-ASCII name returns rather than panics.
- **Labels**: `high` · `bug` · `nif-parser` · `import-pipeline` · `game:starfield` · `legacy-compat`

---

### MEDIUM

#### SF-2026-08-27-D1-01: the #2097 LZ4 panic guard is unreachable as built, and powerless in the only configuration where its named panic can occur

- **Severity**: MEDIUM
- **Dimension**: 1 — BA2 v2/v3 LZ4 block decompression (delta review of `1b521305`)
- **Location**: `crates/bsa/src/ba2.rs:755-816` (the `Lz4Block` arm), `crates/bsa/Cargo.toml:9`, `Cargo.toml:140`
- **Status**: NEW (#2097 is closed; #2585 is the adjacent-but-different under-run signalling item)
- **Description**: `1b521305` wraps `lz4_flex::block::decompress` in
  `catch_unwind` on the strength of the upstream *"May panic if
  `min_uncompressed_size` is smaller than the uncompressed data"* doc, attributing
  the observed absence of panics to "a property of one pinned version". That
  attribution is wrong and the mitigation follows the wrong threat. The absence
  of panics is a property of a **Cargo feature**, not of version 0.11.6 — and in
  the build where that feature is off, the failure mode is not an unwind at all
  but an out-of-bounds heap write that `catch_unwind` cannot intercept.
- **Evidence**:
  - `lz4_flex-0.11.6/src/block/mod.rs:21-25` selects the decoder by feature
    (`safe-decode` → `decompress_safe`, else the raw-pointer `decompress`).
  - **Built path** (`safe-decode` is a default feature) —
    `decompress_safe.rs:354-360` uses `vec![0; min_uncompressed_size]` + a
    bounds-checked `SliceSink` under `forbid(unsafe_code)`; an undersized hint
    yields `Err(DecompressError::OutputTooSmall)`. The documented panic is
    **structurally impossible here**, so the `catch_unwind` at `ba2.rs:775` is
    dead code today.
  - **Unsafe path** (`safe-decode` off) — `decompress.rs:508-517` uses
    `Vec::with_capacity` + `PtrSink`, writing through raw pointers with only
    `debug_assert`s. An undersized hint there is a heap buffer overflow — UB, not
    an unwind — on exactly the attacker-controlled modded-archive bytes the
    commit message cites.
  - **Orchestrator-verified**: `cargo tree -p byroredux-bsa -i lz4_flex -e features`
    confirms `safe-decode` and `checked-decode` are reachable **only** via
    `default`, and `Cargo.toml:140` is a bare `lz4_flex = "0.11"` with no
    `default-features`/`features` pin. `byroredux-bsa` is the sole dependent, so
    nothing else re-enables them.
  - Corroborating doc drift in the same function: `ba2.rs:794` and `:1416` both
    describe `unpacked_size` as *"only a capacity hint (`Vec::with_capacity`)"* —
    that is the **unsafe** module's implementation, not the one this workspace
    compiles.
- **Impact**: Today, none at runtime — a dead guard plus two comments describing
  a module that is not compiled. The exposure is that **nothing pins the
  feature**. A single `default-features = false` on the one `lz4_flex` dependency
  silently swaps every Starfield v3 texture decode onto an unchecked
  raw-pointer decoder, and the in-tree defence that exists specifically for that
  scenario would not fire. Nothing in `cargo test`, clippy or CI would flag the
  flip; the `lz4_decompress_is_panic_guarded` source-order pin would still pass.
- **Related**: closes the loop on #2097; #2585; the standing audit-hygiene rule
  "verify the audit premise against current code before proposing a fix".
- **Suggested Fix**: Pin the feature rather than the panic —
  `lz4_flex = { version = "0.11", default-features = false, features = ["std", "safe-encode", "safe-decode", "frame"] }`
  at `Cargo.toml:140` (optionally `checked-decode` as belt-and-braces). Correct
  `ba2.rs:794` / `:1416` to say `vec![0; n]` + bounds-checked `SliceSink`. Keep
  the `catch_unwind` (cheap, still catches residual panics) but re-word its
  comment so it no longer claims to be the undersized-hint mitigation.
- **Labels**: `medium` · `bug` · `import-pipeline` · `safety` · `game:starfield`

---

### LOW

#### SF-2026-08-27-D1-02: both new source-order pin tests slice source text at a fixed byte offset — latent `not a char boundary` panic

- **Severity**: LOW
- **Dimension**: 1 — delta review of `1b521305` + `cceee44d` (test infrastructure)
- **Location**: `crates/bsa/src/ba2.rs:1686`, `crates/bsa/src/ba2.rs:1718`
- **Status**: NEW
- **Description**: Both fixes introduced the same new technique —
  `include_str!("ba2.rs")`, split on a match-arm marker, then
  `let body = &arm[..arm.len().min(2000)];`. Rust `str` indexing is by **byte**,
  and both arms far exceed 2,000 bytes, so `min(2000)` always resolves to a fixed
  byte cut into text containing multi-byte UTF-8.
- **Evidence**: measured against the current file — the
  `Ba2Compression::Lz4Block =>` arm is 41,574 bytes with **3** em dashes (3 bytes
  each) inside its first 2,000; the `BA2_V_STARFIELD_V3 =>` arm is 66,773 bytes
  with **2**. Byte 2,000 currently lands on ASCII in both, so the tests pass —
  that is luck about where comment text ends, not an asserted property. Both
  commits' new comment blocks sit inside the first 2,000 bytes of their arm, so
  any edit shifts the cut.
- **Impact**: A cosmetic comment edit inside either arm can turn a green suite
  red with an opaque panic, in a test whose whole purpose is to make a
  *deliberate* regression legible. CI/test only — no runtime path.
- **Related**: `SF-2026-08-27-D1-01` (same two commits). **Same defect class as
  the HIGH `SF-2026-08-27-D2-01`** — see the cross-cutting note in the Executive
  Summary; a `clippy::string_slice` lint would catch both.
- **Suggested Fix**: `arm.get(..2000).unwrap_or(arm)`, or scope by the next `}` /
  a line count instead of a byte budget.
- **Labels**: `low` · `bug` · `test-gap` · `import-pipeline`

#### SF-2026-08-27-D1-03: `decompress_chunk_lz4_undersized_hint_never_unwinds` skips hint `0` on a false premise

- **Severity**: LOW
- **Dimension**: 1 — delta review of `1b521305` (test coverage + doc accuracy)
- **Location**: `crates/bsa/src/ba2.rs:1643-1667` (comment at `:1647`); premise contradicted by `crates/bsa/src/safety.rs:78-89`
- **Status**: NEW
- **Description**: The new fuzz-lite test justifies starting its hint sweep at
  `1` with *"0 is rejected upstream by `checked_chunk_size_usize`; start at 1"*.
  That helper only rejects values **above** `MAX_CHUNK_BYTES`; `0` passes
  through. `unpacked_size == 0` with a non-zero `packed_size` is fully reachable
  from a malformed archive, and it is precisely the most-undersized hint
  possible — the one case the test was written to probe and the one it excludes.
- **Evidence**: `checked_chunk_size_usize` returns `Ok(0)` for `0`
  (`safety.rs:78-89`). Reachability: `read_dx10_records` calls
  `checked_chunk_size(unpacked_size, …)` (`ba2.rs:633`), and `extract_dx10`
  (`ba2.rs:861`) takes the decompress branch whenever `packed_size != 0`.
  Behaviour is safe (`vec![0; 0]` + `SliceSink` → `Err(OutputTooSmall)`), and an
  independent scan of 19,656 vanilla v3 DX10 records found **0** chunks with
  `unpacked_size == 0` — but the test exists for the hostile case, not the
  vanilla one.
- **Impact**: Documentation factually wrong about a safety helper, and the
  boundary the test most wanted to cover is silently omitted. No runtime defect.
- **Related**: `SF-2026-08-27-D1-01` (same commit); #586 / #2356 (the `safety.rs` cap family).
- **Suggested Fix**: Sweep `[0usize, 1, 2, 8, 32, actual_payload.len()]` and
  correct the comment to say `0` is *accepted* by `checked_chunk_size_usize` and
  reaches the codec, where the safe decoder rejects it as `OutputTooSmall`.
- **Labels**: `low` · `documentation` · `test-gap` · `import-pipeline`

#### SF-2026-08-27-D3-01: ROADMAP still cites #2359 as the live tracker for CDB Phase 2, but #2359 is CLOSED and Phase 2 is unimplemented

- **Severity**: LOW
- **Dimension**: 3 — CDB material database correctness (doc-rot)
- **Location**: `byroredux/src/asset_provider/material.rs:1106-1125` (the `PresenceOnly` return that is the actual state); `byroredux/src/asset_provider/tests/starfield_mat.rs:148-189` (the invariant test #2359 shipped); tracker reference in `ROADMAP.md`
- **Status**: NEW (doc-rot only — the underlying Phase 2 gap is **not** re-filed)
- **Description**: #2359 was closed COMPLETED on 2026-08-19 by `323f0556`
  ("track the CDB Phase 2 deferral and pin its invariant with a test"). Its
  deliverable was the *tracking + invariant test*, not the Phase 2 feature. The
  ROADMAP forward-blocker chain still names #2359 as the live tracker for
  "CDB → `ImportedMaterial` per-field extraction". The single largest Starfield
  material gap therefore has **no open issue tracking it**: a reader following
  the ROADMAP lands on a CLOSED/COMPLETED issue and reasonably concludes the work
  shipped.
- **Evidence**: `merge_external_material`'s Starfield arm sets one routing flag
  and returns `MergeOutcome::PresenceOnly`; the shipped invariant test pins that
  this is still the state (`assert_eq!(mesh.material.textures,
  MaterialTextureSet::default(), "#2359: … Phase 1 forwards zero authored texture
  data from the CDB")`). Corroborating: production never calls
  `ComponentDatabaseFile::parse` at all — `discover_starfield_cdbs`
  (`material.rs:211`) calls only `probe_header`, and
  `register_starfield_cdb_probe` (`material.rs:631-633`) discards the result and
  increments a counter. **Orchestrator-verified** via `gh issue view 2359`
  (`state=CLOSED`, `reason=COMPLETED`, closed by `323f0556`).
- **Impact**: Documentation/tracking only; no runtime change. Blast radius is
  process — the gap is real and correctly test-pinned, but unowned, so it can
  silently drop off the milestone plan.
- **Related**: #2359 (CLOSED), #1289 (Phase 2 origin), #3230, #2709.
- **Suggested Fix**: Open a fresh Phase 2 issue (or reopen #2359) and repoint the
  ROADMAP forward-blocker row at it. Zero code change.
- **Labels**: `low` · `documentation` · `doc-rot` · `game:starfield`

#### SF-2026-08-27-D6-01: `shader.rs` cites nif.xml as gating the BSEffect FO76 tail on `#GTE# 155`; the actual token is `#EQ# 155`

- **Severity**: LOW
- **Dimension**: 6 — NIF shader blocks, BSVER 155+
- **Location**: `crates/nif/src/blocks/shader.rs:1824-1832` (`refraction_power`), `:1867-1892` (reflectance/lighting/emittance/emit-gradient/luminance), `:1645-1646` + `:1675-1683` (field docstrings)
- **Status**: NEW
- **Description**: Three sites in `BSEffectShaderProperty::parse_inner` justify a
  `bsver >= FO76` (Starfield-inclusive) gate by asserting *"nif.xml gates this on
  `BSVER #GTE# 155`"*. That is false. nif.xml gates every one of these fields on
  `#BS_F76#`, and **both** copies in the tree define it as an equality:
  `<verexpr token="#BS_F76#" string="(#BSVER# #EQ# 155)">Fallout 76 stream 155
  only.</verexpr>` (`docs/legacy/nif.xml:29`,
  `/mnt/data/src/reference/nifxml/nif.xml:29`). The claim originates in
  `cf9d3480` (#746/#747), which widened seven sites mechanically on that premise;
  **four have since been re-narrowed for Starfield on corpus evidence** (#1510,
  #2622). The two `BSEffectShaderProperty` sites are the ones nobody
  re-examined, and they still carry the falsified premise as their authority.
- **Evidence**: The premise is contradicted inside the same file — the five field
  docstrings at `:1675-1683` still say `BSVER == 155` while the code at `:1877`
  reads them on Starfield. #746's two regression tests build an **FO76 field body
  under a Starfield header**, assuming the conclusion rather than testing retail
  bytes. No corpus evidence was cited for the BSEffect widening (contrast #2622,
  which cites 4,417 real blocks for the sibling BLSP luminance quad).
- **Disproof attempted — the code could not be disproved, only the citation.** If
  the six fields were absent on Starfield the parser would over-read ~44 B per
  block. Observed drift is the opposite sign (`shader.rs:1688` records a ~32-byte
  tail *beyond* the FO76 fields), and 89,276 NIFs parse with zero
  `BSEffectShaderProperty` failures despite three misalignment-sensitive
  `read_sized_string()` calls. The fields really are present. **The finding is
  filed against the citation, not the gate.**
- **Impact**: No runtime misbehaviour. The cost is epistemic and lands exactly
  where it hurts: the scoped-out "+32 B BSEffect under-read" follow-up is the
  next person's job, and the first thing they read is a comment telling them
  nif.xml already blesses the Starfield-inclusive gate. It does not. The same
  sentence has already produced four reverted changes in this file, and #2625
  (opaque-tail capture disabling drift telemetry) means nothing will contradict
  it automatically.
- **Related**: #746/#747 (`cf9d3480`, origin), #1510, #2616, #2622 (three
  rollbacks of the same premise), #2625, #3364, #1881.
- **Suggested Fix**: Replace the three comments with the truth — nif.xml's
  `#BS_F76#` is `#EQ# 155` and does not document Starfield; the
  Starfield-inclusive gate rests on corpus evidence, not the spec — and bring the
  five `BSVER == 155` docstrings at `:1675-1683` into agreement with the code.
- **Labels**: `low` · `documentation` · `nif-parser` · `game:starfield`

#### SF-2026-08-27-D7-01: MeshesPatch parse-rate floor is 1.1 points stale — a full revert of #2105 would not trip the gate

- **Severity**: LOW
- **Dimension**: 7 — real-data validation
- **Location**: `crates/nif/tests/parse_real_nifs.rs:186-192` (docstring), `:214-216` (the `min_clean` value)
- **Status**: NEW
- **Description**: The per-archive floors carry a documented methodology
  ("measured minus ~0.5%, rounded down to the nearest 0.5%") and a table
  refreshed 2026-07-11 under #1900 reading
  `MeshesPatch.ba2 ≥ 98.0% (29 849 NIFs; 98.91% actual; was 97.0%)`. **98.91% is
  the pre-#2105 figure** (29,849 − 325 truncated); `b7e0318f` took MeshesPatch
  325 → 6 truncations. Measured today: **99.98%**. The "actual" column is stale
  by 1.07 points and the floor was never re-tightened after the fix it predates,
  so `min_clean: 0.980` now tolerates **597** truncated files where reality has 6.
- **Evidence**: A change that fully reverted #2105 — restoring all 325
  truncations, 98.91% — would leave `parse_rate_starfield_all_meshes` **green**.
  That is the exact regression this gate exists to catch, and the same shape as
  #2201, which the Meshes02 floor caught only because that archive's floor
  happened to sit at 99.5%. Per the file's own rule the value should be **0.995**.
- **Skepticism check** (reasons this might not be a finding, and why they fail):
  (a) *MeshesPatch is genuinely noisy across patch levels* — the prior audit
  measured 99.98% on 2026-08-24 and this pass measured the same 6 files three
  days later; stable, not noisy. (b) *the loose floor is deliberate slack* — the
  docstring states a uniform rule and applies 99.5% to the four archives at
  100.00%, so MeshesPatch is the outlier, not the policy. (c) *another test
  covers the tail* — `per_block_baselines` covers Meshes01 only, and
  `block_coverage_baselines` gates `NiUnknown`, not truncation.
- **Impact**: Test-gate only. A silent 1.1-point regression window on the one
  Starfield archive that still has a residual truncation tail.
- **Suggested Fix**: Set `min_clean: 0.995` and refresh the docstring's "actual"
  column to 99.98%.
- **Labels**: `low` · `bug` · `test-gap` · `nif-parser` · `game:starfield`

---

## Informational Observations (not filed)

Recorded so they are not rediscovered. **Do not file without new evidence.**

- **D9-INFO-01 / Dim-8 C1 — the `Starfield` back-lighting slot arm is
  unreachable, and that is correct.** `slot_role.rs:342-350` routes slot 7 to
  `TextureRole::BackLighting` for `Skyrim | Starfield`, but the only producer
  (`dedicated_shader.rs:142-153`) gates on `TextureSlotLayout::Skyrim` alone.
  **Three dimensions independently confirmed this is not a gap**: nif.xml exposes
  Soft/Rim/Back_Lighting only as Skyrim SLSF2 bits 25/26/27, and the 32-entry
  `BSShaderCRC32` enum has no member for any of the three — so no CRC-era
  authority exists the way `MODELSPACENORMALS` has one. Dim 9 also falsified the
  tempting FO4 widening: FO4's SLSF2 bits 25/26/27 are `Alpha_Test` /
  `Gradient_Remap` / `VATS_Target_Draw_All`.
- **D9-INFO-02 — `Material`'s soft/rim/back booleans are write-only duplicates**
  of bits already in `effect_shader_flags`. The GPU reads only the packed word;
  nothing reads the bools. `byroredux/src/commands/scene.rs:1014` assigns
  `effect_shader_flags` wholesale and leaves them stale. Latent drift hazard, not
  a defect — the field doc gives a deliberate rationale for the split.
- **D9-INFO-03 — open question**: BGSM-sourced back/soft lighting activates with
  no mask texture (`asset_provider/material.rs:84-97`); the shader defaults the
  missing masks to `vec3(1.0)` and applies unmasked full-strength transmission.
  Whether FO4's own renderer requires a map is not established by any in-repo
  reference. Per the no-guessing policy this is flagged, not asserted wrong.
- **Dim 5 O-1 — a doc TODO discharged by data**: `probe_npc_perks` on
  `Starfield.esm` returns 194 NPCs / 432 entries / `PRKR` width `[5]` —
  FO4-identical, so `reader.rs:271-272`'s "confirm the width" caveat is now
  discharged for Starfield (FO76 still open). One-line comment correction.
- **#3364 Starfield consequence (characterised, not re-filed)**: the parse-side
  analogue is `shader.rs:1291`, where `parse_fo76_plus` calls
  `parse_shader_type_data_fo76` for Starfield too, dispatching on `shader_type`
  with no BSVER gate. Dim 8 established the blast radius is **narrower than the
  Skyrim framing implies** — the raw value never crosses the NIFAL boundary (ECS
  `Material` has no `shader_type` field), and slot routing is unreachable for
  Starfield `.mat` stubs due to the `material_reference` early return at
  `dedicated_shader.rs:131`. Exposure is confined to inline-shader Starfield
  meshes. Dim 6 adds that `read_starfield_tail`'s `saturating_sub` would silently
  swallow a spurious 12–16 B consumption rather than surfacing it as drift.
  Worth appending to #3364 rather than raising its severity.

---

## Existing Open Issues Re-confirmed (not re-filed)

`#3230` (CDB gate makes BGSM/BGEM resolver unreachable) · `#2642` (BGSM
`distance_field_alpha_texture` has no role) · `#2637` (sf_smoke unresolved-REFR
report overstates ~5×) · `#2636` (SECH/AOPF zero dispatch) · `#2633` (CDB
duplicate field names last-wins) · `#2628` (`pitch_or_linear_size_for` has no
DXGI 10/11/31 arm) · `#2625` (opaque-tail capture disables drift telemetry) ·
`#1576` (model-less STAT/BNDS/ACTI/ARMO drop) · `#3364` (`BSShaderType155`
FO76-only translation) · `#2585`, `#1761`, `#3348`, `#2099`, `#2105` residual.

Dim 4 **independently corroborated #2637** rather than re-filing it: a
FormID→FourCC map over all 3,829,247 records resolved the smoke's printed
unresolved samples as SOUN×8, ASPC×8, ACTI×2, NPC_×1, AOPF×1 — 17 of 20 are
records the reader parses fine into non-`statics` tables, i.e. by-design
exclusions counted as failures. That matches #2637's "~5× overstatement" in
order of magnitude.

---

## CRC32 Flag Table

**The hashes are not opaque.** A complete named table lives at
`crates/nif/src/shader_flags.rs:289-364` (`pub mod bs_shader_crc32`), **32
entries**, pinned against the nif.xml `BSShaderCRC32` literals by
`bs_shader_crc32_matches_nif_xml_literals` (`shader_flags.rs:530`).

Parsing: `parse_skyrim_shader_base` (`shader.rs:414-450`) reads `num_sf1` +
`sf1_crcs` for BSVER ≥ `FO4_CRC_FLAGS` (132) and `num_sf2` + `sf2_crcs` for
BSVER ≥ `FO76_SF2_CRCS` (152); `parse_fo76_plus` (`:1199-1202`) reads both
unconditionally since 155 > 152. Consumption: `bs_shader_crc32::contains_any`
(`:361`) tests the SF1 ∪ SF2 set.

This matters on Starfield because `parse_fo76_plus` hardcodes
`shader_flags_1: 0, shader_flags_2: 0` (`shader.rs:1297-1298`) — **the CRC arrays
are the only flag channel there.**

| Name (nif.xml spelling) | CRC32 (decimal) | CRC32 (hex) |
|---|---|---|
| `Decal` | 3849131744 | `0xE56F55E0` |
| `Dynamic_Decal` | 1576614759 | `0x5DF87B67` |
| `Two_Sided` | 759557230 | `0x2D46F0EE` |
| `Cast_Shadows` | 1563274220 | `0x5D2E266C` |
| `ZBuffer_Test` | 1740048692 | `0x67B455F4` |
| `ZBuffer_Write` | 3166356979 | `0xBCB50533` |
| `Vertex_Colors` | 348504749 | `0x14C8B4AD` |
| `PBR` | 731263983 | `0x2B98D0EF` |
| `Skinned` | 3744563888 | `0xDF291AF0` |
| `EnvMap` (`ENVMAP`) | 2893749418 | `0xAC7EF32A` |
| `Vertex_Alpha` | 2333069810 | `0x8B0F1BF2` |
| `Face` | 314919375 | `0x12C7300F` |
| `Greyscale_To_Palette_Color` | 442246519 | `0x1A5A00B7` |
| `Hairtint` | 1264105798 | `0x4B5A4B86` |
| `Skin_Tint` | 1483897208 | `0x5872C0B8` |
| `Emit_Enabled` | 2262553490 | `0x86D26652` |
| `Glowmap` | 2399422528 | `0x8F04CB40` |
| `Refraction` | 1957349758 | `0x74A2DABE` |
| `Refraction_Falloff` | 902349195 | `0x35C3138B` |
| `NoFade` | 2994043788 | `0xB27F1F0C` |
| `Inverted_Fade_Pattern` | 3030867718 | `0xB4A9EF46` |
| `RGB_Falloff` | 3448946507 | `0xCD9B8ECB` |
| `External_Emittance` | 2150459555 | `0x802381A3` |
| `ModelSpaceNormals` | 2548465567 | `0x97ED331F` |
| `Transform_Changed` | 3196772338 | `0xBE87B532` |
| `Effect_Lighting` | 3473438218 | `0xCF04A0CA` |
| `Falloff` | 3980660124 | `0xED3EAC9C` |
| `Soft_Effect` | 3503164976 | `0xD0C81FB0` |
| `Greyscale_To_Palette_Alpha` | 2901038324 | `0xACEE1874` |
| `Weapon_Blood` | 2078326675 | `0x7BE39653` |
| `LOD_Objects` | 2896726515 | `0xACAC1BF3` |
| `No_Exposure` (Starfield) | 3707406987 | `0xDCF3C60B` |

**Absent by design** — `Soft_Lighting`, `Rim_Lighting` and `Back_Lighting` have
**no** CRC32 entry. In nif.xml they exist only as `SkyrimShaderPropertyFlags2`
bits 25/26/27. This is the authority behind the D9-INFO-01 disproof above.

Starfield-specific consumption is live and tested:
`starfield_decal_crc_flips_is_decal_when_legacy_flags_are_zero`,
`starfield_dynamic_decal_crc_in_sf2_array_flips_is_decal`,
`starfield_two_sided_crc_flips_two_sided_when_legacy_flags_are_zero`,
`starfield_unrelated_crcs_do_not_trigger_decal_or_two_sided`
(`import/material/double_sided_tests.rs`).

---

## Verification Method

All real-data checks run this session against the on-disk Starfield install.

| Check | Command | Result |
|---|---|---|
| BA2 full corpus | `cargo test -p byroredux-bsa --test ba2_real starfield -- --ignored` | **129 archives, 129 OK, 0 failures** — matches baseline |
| BA2 unit | `cargo test -p byroredux-bsa` | 66 + 1 passed, 0 failed, exit 0 |
| NIF parse rate (5 archives) | `cargo test --release -p byroredux-nif --test parse_real_nifs parse_rate_starfield_all_meshes -- --ignored` | Meshes01 **100.00%** (31,058) · Meshes02 **100.00%** (7,552) · MeshesPatch **99.98%** (29,843/29,849) · LODMeshes **100.00%** (19,535) · FaceMeshes **100.00%** (1,282) · aggregate **89,270/89,276 = 99.9933%**, recoverable 100%, 0 hard failures — byte-identical to ROADMAP |
| NiUnknown ceiling | `cargo test --release --test per_block_baselines per_block_baseline_starfield -- --ignored` | **0 unknown blocks** across 770,322 blocks; 28 types matched |
| CDB parse | `cargo test --release -p byroredux-sfmaterial --test real_cdb -- --ignored` | **97 classes / 1,438,780 instances** in 9.09 s — matches baseline exactly |
| ESM resolve rate | `byroredux --esm Starfield.esm --sf-smoke citycydoniamainlevel` (release) | **25,433 / 27,898 = 91.2%**, 2,465 unresolved (all slot 0x00), LIGH 656 — **bit-for-bit identical** to 2026-08-24 |
| ESM record census | full walk of `Starfield.esm` (1.46 GB) | 3,829,247 records, 0 walk errors; `WMI1` **0 occurrences**; PDCL **706** top-level records |
| BA2 v3 chunk census (independent walker) | 6 vanilla v3 archives | 19,656 records / 61,969 chunks; **2,075 mixed raw+LZ4 records**; `chunk_hdr_len == 24` everywhere; 0 chunks with `unpacked_size == 0` |
| Loose-material census | 129 `.ba2` archives | **0 `.bgsm`, 0 `.bgem`, 20 `.mat`, 13 CDBs** (1 base + 12 DLC/Creation) |
| `char_boundary` panic repro | standalone `rustc -O` of `canonical_mesh_path` | **PANIC** on `"модель"` and on `from_utf8_lossy(&[0xFF,0xFF])`; ASCII correct |
| `lz4_flex` feature resolution | `cargo tree -p byroredux-bsa -i lz4_flex -e features` | `safe-decode`/`checked-decode` reachable **only** via `default`; no pin at `Cargo.toml:140` |
| Unit tests | `byroredux-plugin` 816 · `byroredux` 1,606 · `byroredux-nif` shader 192 / starfield 20 / bs_geometry 56 / import::material 212 · `byroredux-renderer` shader_contract 63 · `byroredux-core` material 37 · `byroredux-sfmaterial` 22 · `byroredux-bgsm` 34 · `byroredux` material_translate 44 / bgsm_merge 47 / glass 26 / starfield_mat 11 | **all green, 0 failed** |

---

## Remaining-Work Chain

Per `docs/engine/starfield-esm-roadmap.md` (Phases 0+1 done, 2–4 invalidated by
the 99.9%-parity measurement). **Do NOT frame this as "BGSM parser first / ESM
very far" — both have shipped.**

1. **Per-field CDB extraction** (CDB Phase 2) — `.mat`-resolved materials still
   reach the Disney BSDF lobe with NIF defaults rather than CDB-authored values.
   `merge_external_material`'s `.mat` arm returns `MergeOutcome::PresenceOnly`;
   the observable signal Phase 2 shipped is that arm returning `Merged`. Single
   highest-value remaining Starfield fidelity item. **Now untracked** — see
   `SF-2026-08-27-D3-01`; #2359 is closed and the ROADMAP still points at it.
2. **Exterior worldspace tiles** — Starfield ships worldspaces but no
   exterior-grid render path is exercised for it today. Cydonia is an interior
   CELL and does not depend on this. Genuinely unimplemented scope, not a
   regression.
3. **Space-cell / planet / GBFM records** — `GBFM`/`GBFT`/`PNDT`/`STDT`/`BIOM`/
   `SFBK`/`SUNP` remain parser gaps. Dim 4 measured **zero occurrences of any of
   them in Cydonia**, resolved or unresolved — they are outdoor/system-scope
   records, so this gap is not load-bearing for the interior target.
4. **The NIF truncation tail** — 6 residual MeshesPatch files, unchanged in
   count, family (`meshes\terrain\<world>\objects\*.nif`) and shape across two
   audits three days apart. Distinct unexplained cause from the closed
   `BSWeakReferenceNode` / cloth / `BSShaderType155` tails.

---

## Coverage Note

Honest limits of this pass, aggregated from each dimension's "Not exercised":

- **No windowed engine launch or rendered-frame validation** anywhere in this
  audit (per the no-parallel-engine-launch rule). All render-path conclusions are
  static analysis plus unit tests; no RenderDoc capture was taken, and per the
  standing "no speculative Vulkan fixes" rule no claim was made that would need one.
- **Dim 2** did not re-measure the `.mesh` name distribution across the mesh
  archives. The "all vanilla names are bare 20-hex ASCII stems" premise — which
  is what bounds `SF-2026-08-27-D2-01` to non-vanilla content — is taken from
  `61520a39`'s commit message, not independently verified. **A corpus scan for
  any non-ASCII `External` mesh name would either confirm vanilla safety or
  escalate that finding to CRITICAL.** This is the highest-value follow-up
  measurement in the report.
- **Dim 3** completed the loose-file half of the CDB-vs-BGSM census but not the
  unique-material-handle count inside the parsed CDB tree; and only the base
  105 MB CDB was parsed end-to-end — the 12 DLC/Creation CDBs were extracted and
  byte-counted but not run through `ComponentDatabaseFile::parse`.
- **Dim 4** attributed only the 20 unresolved FormIDs the smoke prints, not all
  2,465 (the full histogram needs the emit change #2637 asks for), and measured
  one cell only.
- **Dim 5** measured #3362's cross-plugin tombstone behaviour on Skyrim only; the
  Starfield load-order equivalent over 3.3 M REFRs is unmeasured.
- **Dim 6** did not measure the `starfield_tail` length distribution (the 38 B /
  ~32 B figures are quoted from in-tree docstrings) nor the `shader_type`
  histogram on retail Starfield BLSPs — the latter is the cheapest way to settle
  #3364's parse-side half.
- **Dim 8** did not trace the NIFAL particle slice at all, and only spot-checked
  the collision slice's dispatch arms without verifying the matching
  `resolve_shape` arms (violating the standing dispatch↔resolve parity rule as an
  audit gap, not a code gap).
- **Dim 9** did not re-derive BGSM/BGEM byte-level version-branch offsets against
  a real `.bgem`.
- Per `_audit-common.md`'s un-owned-subsystem list, this pass did not exercise
  Starfield content through FaceGen, the Mod Runtime sandbox, or the Havok
  packfile reader (Starfield ragdolls remain blocked on the `BhkSystemBinary`
  blob decoder per `docs/engine/physal.md`, unchanged).

No GitHub issues were created by this audit.
