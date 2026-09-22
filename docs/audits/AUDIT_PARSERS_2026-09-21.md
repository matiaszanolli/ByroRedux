# Parser Discipline Audit: 2026-09-21

**HEAD**: f97775ca8 · **Baseline**: none (first run) · **Audited**: Dims 1–6 (Size Discipline, Error Semantics, Version Gating, Corpus Gates, Decode-Consumer Wiring, I/O & Paths). All in full, because this is the first `/audit-parsers` run · **Unchanged since baseline (skimmed)**: none (first run)

Run as one leg of `/audit-suite --preset comprehensive`. The suite's instruction was no sub-agents, so every dimension was analysed in this session, one at a time. Per-dimension scratch files are at `/tmp/audit/parsers/dim_{1..6}.md`. The skill's Phase-3 `rm -rf /tmp/audit/parsers` was **deliberately not run**, because the suite orchestrator reads those files.

## Executive Summary

**Findings (all NEW):** 0 CRITICAL · **6 HIGH** · 6 MEDIUM · 13 LOW (25 total).

**Existing issues:** none of the 25 matches an open issue. One known open item was touched and not re-filed: CDB presence-only → #3398.

**What the HIGHs are:**
- **Two vanilla-content correctness defects**, silently wrong on shipped data:
  - FaceGen `.egm` morph deltas are decoded as half-floats. The FaceGen SDK and the data both say signed 16-bit integers × per-morph scale. Every FO3/FNV (and Oblivion) runtime-FaceGen NPC head gets wrong deltas.
  - BGSM `specular_enabled = false` is ignored. 466 vanilla FO4 materials (7%) and 612 FO76 materials receive full-strength specular. They are mostly paintings, magazines and signage.
- **Two untrusted-input DoS classes**, reachable from mod or replaced archives:
  - HKX materialises one on-disk string once per pointer to it, so memory is quadratic in file size with no cap.
  - MenuXml `<include>` splices expand exponentially: include nesting never counts toward the depth cap.
- **Two MenuXml load-path panics**: a mid-codepoint `&str` slice after a comment, and `&fnt[12..]` on a short `.fnt`.

**Crates swept:** `byroredux-bsa` (BSA/BA2/CSG/UVD/naming/safety), `byroredux-bgsm`, `byroredux-sfmaterial`, `byroredux-hkx`, `byroredux-facegen`, `byroredux-menuxml` (parse side, plus the load path in `menu.rs`), `byroredux-game-detect` (`vdf`/`steam`). Consumers were traced in `byroredux/src/asset_provider/{archive,texture,animation,material/{provider,merge,mod,cdb}}.rs` and `byroredux/src/npc_spawn/resumable.rs`.

**Unit tests** (Phase 1, `cargo test -j 4 -p byroredux-{bsa,bgsm,sfmaterial,hkx,facegen,menuxml,game-detect}`, isolated target dir): all green.
- 282 passed, 32 ignored, plus 9 doctests.
- Breakdown: bsa 95+1 · bgsm 30 · sfmaterial 26+6 · hkx 22 · facegen 30 · menuxml 22+2+3 · game-detect 45.

**Real-data suites run** (strict lane `BYROREDUX_REQUIRE_GAME_DATA=1`, `--test-threads=1`, one crate at a time): **40 tests, all green.**
- bsa: 24. Oblivion 17 archives / 147,629 files; SSE Meshes0 18,862 NIFs; FO4 Meshes.ba2 34,995 NIFs; Starfield 129/129 archives; 0 errors throughout. The SSE count baselines still match after the 2026-09-02 archive rewrite.
- bgsm: 2 (FO4 6,616 + 283 at 100%).
- sfmaterial: 1 (97 classes / 1,438,780 values).
- facegen: 3.
- hkx: 2 (LE + SE).
- menuxml: 5 (Oblivion 89 XMLs / 1,868 tiles; FO3 HUD).
- byroredux `archive_precedence`: 3.

**Real-data suites skipped:** the byroredux `asset_provider` cart/draugr installs (about 1 GB resident each; memory budget), `facegen_texture_fallback`, `fo4_palette_corpus`, `default_sound_candidates`. All are outside the reader-discipline scope.

**Probes:** every HIGH/MEDIUM decoder claim below was reproduced by a throw-away probe crate outside the repo (`/tmp/audit/parsers/probe`, path deps on the repo crates, debug profile). It builds each crafted input and drives the real public API. Logs are in `/tmp/audit/parsers/probe_*.log`. Corpus measurements (BGSM versions and field usage, EGM/EGT statistics, per-archive duplicate scan, vanilla menu-XML scan) came from the same probe, read-only against the installed games.

**Cross-audit dedup:**
- Checked against `/tmp/audit/issues.json` and all `docs/audits/` reports, including today's `AUDIT_{RENDERER,SAFETY,ECS,CONCURRENCY,PERFORMANCE,NIF,NIFAL,TECH_DEBT}_2026-09-21.md`. No overlap.
- `AUDIT_SAFETY_2026-09-21` states "menuxml … recursion is bounded". That is true for tiles and ops, and not for include chains (PAR-D1-2026-09-21-04).
- NIF-D6-2026-09-21-02's `Vec::with_capacity(num_sections)` pattern is **absent** in `crates/hkx`. `packfile.rs:78-92` bounds `section_count` to 1..=64 and checks the section table fits in the file before reserving.

---

## Findings

### PAR-D1-2026-09-21-01: HKX materialises one on-disk string once per pointer to it: memory is quadratic in file size, with no cap
- **Severity**: HIGH
- **Dimension**: Size Discipline
- **Location**: `crates/hkx/src/packfile.rs:172-193`, `crates/hkx/src/animation.rs:290-307`, `crates/hkx/src/animation.rs:518-577`
- **Status**: NEW
- **Trigger Input**: many pointers to one long string. This can be many virtual-fixup entries naming one long NUL-terminated class name, up to 4,096 bone-name pointers, or up to 65,536 annotation-text pointers aimed at one long NUL-terminated string in `__data__`.
- **Description**:
  - **Virtual fixups.** `Packfile::parse` turns every virtual-fixup entry into `(offset, read_cstr(..).to_owned())`. The entry count is bounded only by the table size, which is file-sized.
  - **Bone names.** `decode_skeleton` copies each bone name (`.to_owned()`).
  - **Annotations.** `read_annotations` copies each annotation text and clones the track name per annotation.
  - **No bound on shared strings.** Strings have no length cap and nothing de-duplicates them. N pointers to one M-byte string cost N·M bytes of real (written) memory, and `read_cstr` also rescans the M bytes for every pointer.
  - The count caps (4,096 bones, 65,536 annotations, 16M samples) bound the number of objects, not the bytes each one owns.
- **Evidence** (probe `hkx-bones` / `hkx-vfix`; the builder follows `packfile::fixtures::PackfileBuilder`'s 64-bit layout):
  ```
  hkaSkeleton file 368,929 B (4096 bones -> one 65,536 B name): decode Ok in 0.80 s;
      owned name bytes 268,435,456; VmHWM 3,916 kB -> 266,544 kB
  packfile 256,561 B (20,000 virtual fixups -> one 16,384 B class name): decode_skeleton -> Err(MissingClass)
      after VmHWM 3,896 kB -> 322,844 kB (the copies happen inside Packfile::parse, before any class lookup)
  ```
  With N = S/24 fixups and an S/2-byte name, a file of S bytes allocates about S²/48. That is roughly 21 GB for a 1 MB file and 83 GB for a 2 MB file. The annotation route reaches 65,536 × M.
- **Impact**: OOM kill or `handle_alloc_error` abort. Neither is interceptable.
  - `decode_skeleton` / `decode_spline_animation` run on the main thread from `byroredux/src/asset_provider/animation.rs`: `skeleton.hkx`, the cart-idle family, `1hm_walkforward.hkx`, and the draugr rig plus three clips.
  - Lookup goes through `TextureProvider::extract_mesh`, where the last-listed archive wins. A mod BSA that overrides `skeleton.hkx` (a very common kind of mod) is enough.
  - Only Skyrim sessions are affected.
- **Related**: #3011 (sample-count bomb, closed; counts only), #4332 (layout gates), PAR-D1-2026-09-21-02
- **Suggested Fix**:
  - Resolve virtual-fixup class names by reference (store the name offset, or compare in place against the few classes the crate looks up) instead of owning a copy per entry.
  - Cap name and annotation string length (Havok names are short, e.g. 256 bytes).
  - Budget total owned string bytes per decode relative to `bytes.len()`.

### PAR-D1-2026-09-21-04: MenuXml `<include>` splices expand exponentially: include nesting never counts toward the depth cap
- **Severity**: HIGH
- **Dimension**: Size Discipline
- **Location**: `crates/menuxml/src/parse.rs:638-679`, `crates/menuxml/src/parse.rs:600-611`
- **Status**: NEW
- **Trigger Input**: prefab fragments `menus\prefabs\l{n}.xml` whose whole body is `<include src="l{n+1}.xml"/><include src="l{n+1}.xml"/>`. Alternatively, one fragment including itself under several spellings.
- **Description**:
  - `splice_include` re-enters `parse_element_content` with the **same** `depth`. The tile cap (48) and op cap (64) therefore never see include nesting.
  - `seen_includes` is an ancestor-path stack. It stops a file from including itself on the current path, but a DAG of distinct fragments with fan-out f and depth d is expanded to f^d splices.
  - Each splice re-fetches the fragment through `MenuFileSource::menu_xml`, which is an archive extract plus inflate in production.
  - Spellings are compared as normalised strings (`x.xml`, `prefabs\x.xml`, `menus\prefabs\x.xml` all resolve to the same archive entry through the candidate list). A fragment that includes itself under K spellings therefore re-enters about K! times.
- **Evidence** (probe `menuxml-bomb`, in-memory source; each extra level doubles both time and memory):
  ```
  depth 10 (11 files,  481 B):     1,025 tiles,     2,047 fetches,  10 ms
  depth 17 (18 files,  817 B):   131,073 tiles,   262,143 fetches, 0.77 s, VmHWM 45.5 MB
  depth 19 (20 files,  913 B):   524,289 tiles, 1,048,575 fetches, 3.09 s, VmHWM 177.5 MB
  ```
  Depth 30 is about 1.4 KB of XML. It extrapolates to about 1.07e9 tiles, roughly 360 GB, and hours of archive fetches.
- **Impact**: hang, then OOM abort, while loading the HUD.
  - The path is `hud.rs` `launch_hud` → `MenuRenderer::load_with_profile` / `graft_fragment`, on the main thread with no `catch_unwind`.
  - Source: the single `Oblivion - Misc.bsa` / `Fallout - Misc.bsa` beside the ESM (`hud.rs:189-214`, opt-in `--hud`).
  - Mods cannot add a second menu archive, so the trigger is a modified or replaced Misc BSA.
  - Vanilla content does not trigger it (`vanilla_corpus`/`fo3_corpus` pass).
- **Related**: PAR-D1-2026-09-21-05; `include_cycles_terminate` (covers cycles only); AUDIT_SAFETY_2026-09-21 "menuxml … recursion is bounded"
- **Suggested Fix**:
  - Pass `depth + 1` (or a separate include depth) into the spliced `parse_element_content`.
  - Keep a per-document budget on total include splices and total tiles, with a warning when truncating.
  - Normalise include keys by the resolved archive key, not the authored spelling.

### PAR-D1-2026-09-21-05: MenuXml scanner panics when a non-ASCII character follows a comment, and silently drops a byte when it is ASCII
- **Severity**: HIGH
- **Dimension**: Size Discipline
- **Location**: `crates/menuxml/src/parse.rs:499-504`, `crates/menuxml/src/parse.rs:595-598`, `crates/menuxml/src/parse.rs:552-554`, `crates/menuxml/src/parse.rs:315-327`
- **Status**: NEW
- **Trigger Input**: valid-UTF-8 menu XML containing `text<!-- c -->é…` inside a trait body or a tile body.
- **Description**:
  - `take_text` stops at the `<` of a comment. The following `skip_trivia` skips the comment and lands on body text. `take_element()` then returns `None` because the text does not start with `<`.
  - The "drop one byte to guarantee progress" fallback (`scanner.pos += 1`) advances one **byte**. The next `self.src[self.pos..]` slice panics when that byte sits inside a multi-byte character.
  - When the character is ASCII, the byte is silently lost, which corrupts the trait value.
- **Evidence** (probe `menuxml-midchar`):
  ```
  <string>abc<!-- note -->été</string>   -> panicked at parse.rs:327:37: start byte index 55 is not a char boundary; it is inside 'é'
  <rect>junk<!-- note --> über<x>1</x>  -> same panic (parse_element_content path), inside 'ü'
  <string>plain<!-- note -->ascii</string> -> Str("plainscii")
  ```
  Vanilla scan (probe `menuxml-scan`) of 309 vanilla menu XMLs (Oblivion 89, FNV 121, FO3 99): 0 `text<!--c-->text` sites. Their only non-ASCII bytes are leading UTF-8 BOMs on 6 FNV and 6 FO3 prefabs, which `take_text` consumes harmlessly. Vanilla is unaffected.
- **Impact**: engine panic at `--hud` launch (main thread, no `catch_unwind`) from a modified or replaced Misc BSA, or silent value corruption with ASCII. Same bug class as #3391 (`&str` byte-slicing of disk-derived text).
- **Related**: #3391, PAR-D1-2026-09-21-04, PAR-D2-2026-09-21-03
- **Suggested Fix**: advance by `self.src[self.pos..].chars().next().map_or(1, char::len_utf8)`, or better, consume the stray text with `take_text` so it is kept rather than dropped. Add the three probe strings as unit tests.

### PAR-D2-2026-09-21-03: MenuXml HUD load panics on a `.fnt` shorter than 12 bytes
- **Severity**: HIGH
- **Dimension**: Error Semantics
- **Location**: `crates/menuxml/src/menu.rs:188`
- **Status**: NEW
- **Trigger Input**: any font slot's `.fnt` under 12 bytes (truncated or corrupt font file in the font archive).
- **Description**: `load_with_profile` computes the atlas name with `String::from_utf8_lossy(&fnt[12..])` before calling `Font::parse`. `Font::parse` has its own `fnt.len() < HEADER_LEN` → `FontError::TruncatedHeader` check (`font.rs:74-76`), but it never runs because the slice panics first.
- **Evidence** (probe `menuxml-short-fnt`, 8-byte font through the public API):
  ```
  thread 'main' panicked at crates/menuxml/src/menu.rs:188:56:
  range start index 12 out of range for slice of length 8
  ```
- **Impact**: engine panic at `--hud` launch (main thread, no `catch_unwind`). Fonts come from the Misc BSA (Oblivion) or the texture BSA (FO3/FNV, `FontArchive::Textures`, user-selectable via `--hud-textures`). The rest of `load_with_profile` already degrades a failed font to `None` with a warning.
- **Related**: PAR-D1-2026-09-21-05. Owner note: `menu.rs` is `/audit-ui` territory; it is filed here because it is a file-byte-reachable panic on the reader load path.
- **Suggested Fix**: use `fnt.get(12..).unwrap_or_default()`, or read the name inside `Font::parse` after its length check, so a short font takes the existing warn-and-`None` path.

### PAR-D5-2026-09-21-01: FaceGen `.egm` morph deltas are decoded as IEEE half-floats, but the format stores per-morph-scaled signed 16-bit integers
- **Severity**: HIGH
- **Dimension**: Decode-Consumer Wiring
- **Location**: `crates/facegen/src/egm.rs:144-151`, `crates/facegen/src/egm.rs:18-28`, `crates/facegen/src/lib.rs:79-118`, `crates/facegen/src/eval.rs:20-30`
- **Status**: NEW
- **Trigger Input**: every vanilla `.egm` (FREGM002).
- **Description**:
  - The parser reads each delta component as `half_to_f32(u16)`.
  - The FaceGen SDK file-format manual specifies, per mode, a `float` scale x, then for each vertex "3 signed short m. The actual morph values should be m * x". PyFFI's EGM format (scale = n/32768.0, integer components) agrees.
  - `i16` and `f16` are both 2 bytes, so the exact-size check and the real-data test pass on wrongly-typed data.
  - The evaluator's rationale that "FaceGen used NaN as a 'no displacement' sentinel" (`eval.rs:20-30`) describes this mis-decode. Those NaNs are small negative integers.
  - `egm.rs:15`'s "num_vertices verified == base head NIF's vertex count" is also false. EGM V = TRI V + K (vanilla `headhuman.tri`: V=1211, K=238 → 1449). The consumer's "best-effort prefix" over the first 1211 is therefore the right base-vertex mapping; the numeric type is the defect.
- **Evidence** (probe `egm-stats`, vanilla FNV `meshes\characters\head\headhuman.egm`, 1449 verts, 50+30 morphs):
  ```
  morphs whose max |int16| >= 30000: 80 of 80   (all exactly 32767: full-range int16 quantisation)
  f16 non-finite: 32,456 (9.33%), of which int16 in [-1024,-1]: 32,319   (the rest are int16 31744..32767)
  int16 sign split: pos 170,754 / neg 170,898 / zero 6,108
  finite components 315,304: mean |true delta| 0.00727, mean |f16 decode - true| 0.01349 (1.86x); 16.4% off by > 2x
  ```
  - Same signature on FNV `cowboyhat.egm` (675 v), `hockeymask.egm` (1424 v) and Oblivion `armor\chainmail\m\helmet.egm` (779 v): 80/80 near-full-range, 7.7–13.0% NaN.
  - Negative deltas in [-32768, -1025] decode in inverted magnitude order: −1025 → −65504·scale, and −32768 → −0.
- **Impact**:
  - Every runtime-FaceGen NPC head, via `npc_spawn/resumable.rs:1193-1239` → `apply_morphs` (FO3/FNV, and Oblivion where a recipe exists), is deformed by wrong deltas. About 9% are dropped as NaN/Inf and the rest have nonlinear, partly inverted magnitudes.
  - `docs/feature-matrix.md:76` lists FaceGen morphs ✓ for FO3/FNV.
  - The real-data test only prints the non-finite count, so nothing catches it.
- **Related**: #3048 (evaluator NaN/overflow guard), #2599 (the facegen `half_to_f32` copy exists only for this decode), #3544, PAR-D5-2026-09-21-04
- **Suggested Fix**:
  - Decode `i16::from_le_bytes` × morph `scale` (`EgmMorph.deltas` can then hold final displacements).
  - Delete the facegen `half_to_f32` and its #2599 pin, and fix the format docs.
  - Make `parse_vanilla_headhuman_egm` assert zero non-finite components and every morph's max |raw| near 32767.

### PAR-D5-2026-09-21-02: BGSM `specular_enabled = false` is never read, so specular is forwarded unconditionally
- **Severity**: HIGH
- **Dimension**: Decode-Consumer Wiring
- **Location**: `byroredux/src/asset_provider/material/merge.rs:792-797`, `byroredux/src/asset_provider/material/merge.rs:562-576`
- **Status**: NEW
- **Trigger Input**: vanilla BGSMs authored with Specular disabled.
- **Description**:
  - `merge_bgsm_arm` copies `specular_color` and `specular_mult` into `ImportedMaterial` for the first BGSM in the chain without checking `bgsm.specular_enabled`. It also derives roughness from `smoothness` regardless.
  - The NIF path honours the same authoring intent: `NiSpecularProperty` disabled zeroes both `specular_strength` and `specular_color` (`crates/nif/src/import/material/walker.rs:188-191`, #696), specifically so the glass-IOR branch cannot re-promote specular.
  - `translate_material` passes both fields straight through (`material_translate.rs:608-614`).
- **Evidence** (probe `bgsm-fields` / `bgsm-specoff` over the vanilla material archives):
  ```
  Fallout4 - Materials.ba2   : 467 BGSM specular_enabled=false, 466 of them with specular_mult > 0 (of 6,616)
  SeventySix - Materials.ba2 : 653 / 612 (of 25,888)
  FO4 by folder: paintingsgeneric 82, comicsandmagazines(+highres) 53, signage 31, buildings 27, grognak 25,
                 interiors\building 20, vault 18, diamondcity 17, ...
  typical values: specular_color [1,1,1], specular_mult 1.0, smoothness 1.0 (defaults left in place)
  ```
  `bgsm_merge.rs` has no test with `specular_enabled: false`.
- **Impact**: matte printed or painted surfaces (paintings, magazines, signage, architecture) render with full-strength white specular, and a smoothness-derived roughness they were authored to ignore. This is a divergent `Material` out of NIFAL translation (HIGH floor) on about 7% of vanilla FO4 BGSMs. Owner overlap: `/audit-nifal`.
- **Related**: #696, #220, #1454, #3639
- **Suggested Fix**:
  - Gate the specular forward (and the `set_specular` sentinel) on `bgsm.specular_enabled` across the chain.
  - When disabled, zero `specular_strength` and `specular_color` exactly as `walker.rs` does, and leave roughness at the matte default rather than `1 - smoothness`.
  - Add a `bgsm_merge.rs` test with `specular_enabled: false`.

### PAR-D1-2026-09-21-02: HKX `MAX_TRANSFORM_SAMPLES` is absolute: a 17 KB clip decodes to 610 MiB and is retained as about 2 GB of keys
- **Severity**: MEDIUM
- **Dimension**: Size Discipline
- **Location**: `crates/hkx/src/animation.rs:52`, `crates/hkx/src/animation.rs:338-358`, `crates/hkx/src/animation.rs:444-452`, `byroredux/src/asset_provider/animation.rs:392-474`
- **Status**: NEW
- **Trigger Input**: an `hkaSplineCompressedAnimation` whose `transform_count × num_frames` sits just under 16,000,000, with all-static tracks. Its mask bytes are 0, so no per-frame data is needed.
- **Description**:
  - #3011 bounded the product to stop allocator aborts, but the bound is absolute rather than tied to the data present. Static tracks cost 0 bytes per frame, so output size is decoupled from file size.
  - `convert_hkx_clip` then expands each sample into `TranslationKey` (56 B), `RotationKey` (48 B) and `ScaleKey` (32 B), about 136 B/sample on the default glam layout.
  - The result is registered in `AnimationClipRegistry` for the session.
- **Evidence** (probe `hkx-samples`):
  ```
  spline clip file 16,929 B: Ok, 4096 tracks x 3906 frames = 15,998,976 samples (40 B each = 610 MB) in 1.37 s;
      VmHWM 3,376 kB -> 651,988 kB
  ```
  99 tracks × 161,616 frames binds fully to the vanilla 99-bone skeleton. That gives about 2.18e9 bytes of converted keys per clip, on top of the transient 610 MiB decode.
- **Impact**: one mod-replaced clip pins about 2 GB for the session. The cart catalogue alone installs up to 16 clips, so a few such files exhaust RAM on a 16 GB machine. Main thread, Skyrim only.
- **Related**: #3011, PAR-D1-2026-09-21-01
- **Suggested Fix**:
  - Require `num_frames <= num_blocks * (max_frames_per_block - 1) + 1`.
  - Cap `max_frames_per_block` and `num_frames` at a measured vanilla ceiling (plus headroom), so output stays proportional to the per-block data the file must actually carry.
  - Optionally budget converted keys per clip.

### PAR-D1-2026-09-21-03: BA2 DX10 header synthesis multiplies file-controlled dimensions in `u32`, panicking in debug builds
- **Severity**: MEDIUM
- **Dimension**: Size Discipline
- **Location**: `crates/bsa/src/ba2.rs:1070-1074`
- **Status**: NEW
- **Trigger Input**: a DX10 record with a 16-byte-block DXGI format (BC2/3/5/6H/7) and `width`, `height` ≥ 65533.
- **Description**: `pitch_or_linear_size_for` computes `bw * bh * bb` in `u32`. With 16384 blocks on each side and 16 bytes per block, that is exactly 2³², so the product overflows. The function runs at extract time (`extract_dx10` → `build_dds_header`), not at open time.
- **Evidence** (probe `ba2-dx10`, a one-chunk synthetic v1 DX10 BA2):
  ```
  65535x65535 BC7 -> panicked at crates/bsa/src/ba2.rs:1073:17: attempt to multiply with overflow
  65533x65533 BC7 -> same panic
  65532x65532 BC7 -> Ok, dwPitchOrLinearSize = 4,294,443,024
  65535x65535 BC1 -> Ok, 2,147,483,648
  ```
- **Impact**:
  - Debug builds (`cargo run`) panic on the main thread (`resolve_texture` → `TextureProvider::extract`, no `catch_unwind`) from a crafted or corrupt mod BA2.
  - Release builds wrap silently. The damage is header-only there, because `crates/renderer/src/vulkan/dds.rs` recomputes mip sizes and ignores the field.
  - Precedent #4155 (a debug-only `u32` overflow panic in a Havok reader) was rated MEDIUM.
- **Related**: #4155, #594, #2628
- **Suggested Fix**: compute in `u64` and saturate to `u32::MAX` (or reject absurd dimensions at record-read time with a named error). Add the 65535² BC7 case as a unit test.

### PAR-D1-2026-09-21-06: The sfmaterial value readers recurse without a depth bound, and an 84-byte CDB aborts the process
- **Severity**: MEDIUM
- **Dimension**: Size Discipline
- **Location**: `crates/sfmaterial/src/reader.rs:846-871`, `crates/sfmaterial/src/reader.rs:922-993`, `crates/sfmaterial/src/reader.rs:1025-1056`, `crates/sfmaterial/src/reader.rs:727-778`
- **Status**: NEW
- **Trigger Input**: a `CLAS` (IS_STRUCT) whose single inline field has its own class as type, plus one `OBJT` of that class. The same happens with a user-flagged self-reference through chunk fields, one chunk per level.
- **Description**:
  - `read_value` ↔ `read_user_class` ↔ `read_primitive_ref`, and `skip_value` ↔ `skip_user_class`, have no depth counter.
  - `ParseLimits::max_instances` counts top-level object chunks only.
  - `parse_with_limits`' doc tells "callers loading untrusted or memory-constrained content" to choose a finite limit. That limit does not protect against this.
- **Evidence** (probe `cdb`):
  ```
  self-referential CDB: 84 bytes; probe_header -> Ok(4)
  parse_with_limits(max_instances: 1_000_000)       -> thread 'main' has overflowed its stack ... aborting (exit 134)
  validate_instances_with_limits(max_instances: 1_000_000) -> same abort
  ```
- **Impact**:
  - Latent today: production uses only `peek_magic` and `probe_header` (`asset_provider/material/cdb.rs:141-149`), which accept this file.
  - `visit_`/`validate_instances_with_limits` are documented as the Phase-2 (#3398) entry points.
  - The skill carried this as "latent — re-check". It is still unbounded, and is now an empirically confirmed abort.
- **Related**: #3398 (OPEN), #2614, #2623, #3055, #4274
- **Suggested Fix**: thread a depth counter through the read and skip recursion (vanilla nesting is shallow; a cap of 64 is ample). Return `Error::NestingTooDeep` and add the 84-byte fixture as a test before #3398 wires the full parse.

### PAR-D2-2026-09-21-01: Every archive-extract consumer discards non-NotFound errors, so a corrupt entry reads as "missing" and silently falls back to a lower-precedence archive
- **Severity**: MEDIUM
- **Dimension**: Error Semantics
- **Location**: `byroredux/src/asset_provider/texture.rs:79-83`, `:108-113`, `:129-133`, `:175-179`, `byroredux/src/asset_provider/material/provider.rs:316-320`, `:357-361`, `:577-590`
- **Status**: NEW
- **Trigger Input**: any entry whose extraction returns `Err`:
  - `InvalidData` from the #3410 decompression-bomb rejection;
  - the #352/#586 size guards;
  - an LZ4/zlib body error;
  - `UnexpectedEof` from a truncated or replaced archive.
- **Description**:
  - All six loops are `if let Ok(data) = archive.extract(..) { return Some(data) }`. The readers build labelled errors, and every one is dropped without a log.
  - The loop then tries the next, earlier-listed archive. A corrupt last-listed override therefore silently resolves to the vanilla copy, inverting #3637 precedence for that entry, or ends as `None`.
  - `None` becomes the checkerboard; for BGEM, the warning "BGEM not found in any loaded archive" (`provider.rs:588`), which is misleading for a present-but-corrupt file; for BGSM templates, `ResolveError::NotFound`.
- **Evidence**:
  ```rust
  for archive in self.texture_archives.iter().rev() {
      if let Ok(data) = archive.extract(normalized.as_ref()) {
          return Some(data);
      }
  }
  ```
- **Impact**: corrupt mod content is invisible in logs and can silently revert to vanilla. `tex.missing` reports "missing" for a present-but-corrupt texture, which is the wrong first diagnostic.
- **Related**: #3637, #3410, #586, AUDIT_STARFIELD_2026-09-05 (quoted this loop for ordering only)
- **Suggested Fix**: `match` the result. Continue silently only on `ErrorKind::NotFound`. Otherwise `warn!` once per (archive, path) with the error and archive path, then choose the fall-through policy explicitly. Mirror this in `MaterialProvider`.

### PAR-D4-2026-09-21-01: No CI lane runs any parser crate's real-data suite: the nightly lane is NIF-only
- **Severity**: MEDIUM
- **Dimension**: Corpus Gates
- **Location**: `.github/workflows/real-data-gates.yml:97-101`
- **Status**: NEW
- **Trigger Input**: n/a (process gap).
- **Description**:
  - `ci.yml` runs `cargo test --workspace`, which skips `#[ignore]`.
  - `real-data-gates.yml` runs only `-p byroredux-nif --test parse_real_nifs --test per_block_baselines --test block_coverage_baselines -- --ignored`.
  - None of the bsa, bgsm, sfmaterial, hkx, facegen or menuxml real-data suites is scheduled. This re-verifies the skill's 2026-09-19 note; it is unchanged.
- **Evidence**: this audit ran them by hand: 37 tests plus 3 `archive_precedence` tests, all green (see the Executive Summary). A regression in `Ba2Archive::open`, the BGSM decoder, the HKX decoder or the CDB walker would stay invisible until someone does the same.
- **Impact**: no automated coverage of vanilla archive, material, packfile or FaceGen decoding. #3918-style silent regressions are possible here exactly as they were for NIF before #3919.
- **Related**: #3919, #3850, #1558
- **Suggested Fix**: add per-title steps (or one parsers job) to `real-data-gates.yml` running `cargo test --release -p byroredux-{bsa,bgsm,sfmaterial,hkx,facegen,menuxml} -- --ignored`, together with the lane's no-test-matched guard.

### PAR-D4-2026-09-21-02: Strict-lane (#3850) holes: several harnesses still turn a missing archive, a broken reader or an unset variable into a green skip
- **Severity**: MEDIUM
- **Dimension**: Corpus Gates
- **Location**: `crates/bgsm/tests/parse_all.rs:171-185`, `crates/facegen/tests/parse_real_facegen.rs:149-176`, `crates/bsa/tests/bsa_real.rs:104-108`, `crates/bsa/src/archive/tests.rs:172-244`, `crates/hkx/src/animation.rs:1171-1181`, `crates/menuxml/tests/vanilla_corpus.rs:48-64`
- **Status**: NEW
- **Trigger Input**: a data dir that is present but whose archive is absent or unopenable; or an unset or wrongly named env var.
- **Description**:
  - **bgsm.** `open_materials_archive` returns `None` (test passes) when `Fallout4 - Materials.ba2` is absent **or `Ba2Archive::open` fails**, even under `BYROREDUX_REQUIRE_GAME_DATA=1`.
  - **facegen.** `Game::extract` uses `BsaArchive::open(..).ok()?` / `.extract(..).ok()`, which surfaces as "data not available; skipping". `for_each_game` only `eprintln!`s when zero games ran.
  - **bsa_real / ba2_real / csg_real.** A missing named archive after `require_game_data` passes is "Skipping" plus `return`.
  - **bsa in-crate tests.** The 11 `#[ignore]`d FNV/SSE tests in `crates/bsa/src/archive/tests.rs` never call `require_game_data`.
  - **hkx.** Its real-data tests skip and return under REQUIRE. They read `BYROREDUX_SKYRIM_DATA` while `real-data-gates.yml:62` and `bsa_real` use `BYROREDUX_SKYRIMSE_DATA`. This is the repo-wide split noted in AUDIT_UI_2026-08-27, which is unfiled. A set-but-wrong override is not binding either.
  - **menuxml.** `vanilla_corpus.rs` is a plain `#[test]` with no default path. This run observed 3× "skipping: BYROREDUX_OBLIVION_DATA not set" followed by `ok`. `fo3_corpus.rs` is a plain `#[test]` with no REQUIRE.
- **Evidence**: see the locations. For the bgsm case:
  ```rust
  match Ba2Archive::open(&archive_path) {
      Ok(a) => Some(a),
      Err(e) => { eprintln!("skipping: failed to open {:?}: {}", archive_path, e); None }
  }
  ```
- **Impact**: the dangerous cases are bgsm and facegen. A regression in the archive readers the tests depend on reads as a skip, so the strict lane goes green on exactly the failure it exists to catch.
- **Related**: #3850, #3014, #3741
- **Suggested Fix**:
  - Under REQUIRE, turn every "archive absent" or "open failed" branch into a panic that names the path.
  - Route the hkx and menuxml resolvers through the same `data_dir()` / `require_game_data` shape.
  - Pick one Skyrim SE env-var spelling.

### PAR-D1-2026-09-21-07: Entry-count reservations are absolute rather than file-relative, and BSA's declared `file_count` is never reconciled with its folder records
- **Severity**: LOW
- **Dimension**: Size Discipline
- **Location**: `crates/bsa/src/archive/open.rs:155`, `:200`, `:320`, `crates/bsa/src/ba2.rs:302`, `:323`, `:502`, `:543`, `crates/bsa/src/csg.rs:168`
- **Status**: NEW
- **Trigger Input**: a tiny header declaring up to 10M entries (`MAX_ENTRY_COUNT`).
- **Description**:
  - `Vec::with_capacity(count)` and `HashMap::with_capacity(file_count)` reserve up to 10M entries before the first record read fails.
  - A BSA's header `file_count` is independent of Σ `folder.count` and is never cross-checked. A 36-byte BSA declaring 10M files and 0 folders opens `Ok` with 0 files.
  - CSG already holds `file_len` when it sizes `vec![0u8; num_chunks * 8]`.
- **Evidence** (probes `bsa-reserve` / `ba2-reserve`):
  ```
  36-byte BSA (10M files / 0 folders): open -> Ok (0 files) in 5.5 ms; VmHWM 3,428 kB -> 19,936 kB (+ ~1.2 GB untouched virtual)
  24-byte BA2 (10M files): open -> Err("failed to fill whole buffer") in 50 µs; VmHWM flat
  ```
- **Impact**: harmless on Linux overcommit (only the HashMap control bytes are touched). The reservation is committed charge on Windows. The error carries no field context.
- **Related**: #586, #2614 (the sfmaterial `count.min(bytes.len() / 8)` clamp is the in-repo pattern)
- **Suggested Fix**: clamp each capacity hint to `remaining_file_bytes / record_size`, and warn (or `Err`) when a BSA's `file_count != Σ folder.count`.

### PAR-D2-2026-09-21-02: BA2 short-decode warnings name neither the archive nor the entry, and DX10 never checks a chunk's decoded length
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/bsa/src/ba2.rs:762-766`, `:836-841`, `:848-866`, `:880-913`
- **Status**: NEW
- **Trigger Input**: a BA2 chunk whose stream decodes shorter than its declared `unpacked_size`.
- **Description**:
  - `decompress_chunk` warns "BA2 zlib decompressed N bytes but record declared M" with no path in scope. `extract_general` and `extract_dx10` do not receive the path. The skill asked to "confirm the warning names the path", and it does not; the BSA sibling does (`archive/extract.rs:173-183`).
  - `extract_dx10` concatenates each chunk's actual length with no check against `chunk.unpacked_size`. A short non-final chunk shifts every later mip under the unchanged synthesized header. CSG rejects the equivalent (#1986).
- **Evidence**: see the locations.
- **Impact**: downgraded from MEDIUM after reading the consumer. `crates/renderer/src/vulkan/texture.rs:339-350` (#4511) rejects a payload shorter than the mip chain, so the usual outcome is a checkerboard plus a renderer-side "DDS pixel data too small" error that does not point at the BA2. A silent mip shift remains possible only when another chunk over-delivers.
- **Related**: #2618, #812, #1986, #4511
- **Suggested Fix**: thread the entry path into `extract_general`/`extract_dx10`/`decompress_chunk` for the warning. For DX10, reject (or zero-pad to `unpacked_size`) a short non-final chunk, after measuring vanilla with the `ba2_real` sweep.

### PAR-D2-2026-09-21-04: A stale duplicate test pins "current" LZ4 under-run behaviour on a false premise
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/bsa/src/ba2.rs:1734-1770`
- **Status**: NEW
- **Trigger Input**: n/a.
- **Description**:
  - `decompress_chunk_lz4_undersized_declared_size_currently_truncates_silently` has the same body as `decompress_chunk_lz4_under_run_returns_actual_length_not_declared` (`:1570-1587`).
  - Its doc says "#2618 (MEDIUM, open …) … Once #2618 lands, this assertion should flip". #2618 is CLOSED: the warning landed and nothing flipped.
  - It also calls `min_uncompressed_size` "only a capacity hint (`Vec::with_capacity`)". #3392 and the LZ4 arm's own comment (`:817-823`) correct that: it is a hard output bound under `safe-decode`.
- **Evidence**: see the location.
- **Impact**: misleading documentation about memory-safety-relevant decoder behaviour, and a redundant test.
- **Related**: #2618, #2630, #3392
- **Suggested Fix**: delete the duplicate, or fold its intent into the surviving test's doc.

### PAR-D3-2026-09-21-01: BGSM/BGEM accept any version with the newest layout and never check for unconsumed bytes, so layout drift is silent
- **Severity**: LOW
- **Dimension**: Version Gating
- **Location**: `crates/bgsm/src/base.rs:170-171`, `crates/bgsm/src/bgsm.rs:158-335`, `crates/bgsm/src/bgem.rs:96-190`
- **Status**: NEW
- **Trigger Input**: a BGSM/BGEM with `version > 22`, or any mis-gated field.
- **Description**: there is no version ceiling, and `parse_bgsm`/`parse_bgem` return `Ok` without checking `Reader::remaining()`. An unknown future layout, or a gating bug, decodes to wrong field values with no signal. The reference implementation also has no version ceiling (Material-Editor `BaseMaterialFile.cs:179-234`), so a warning rather than an `Err` fits.
- **Evidence** (probe `bgsm-scan`):
  - Vanilla uses only v2 (FO4: 6,616 BGSM + 283 BGEM) and v22 (FO76: 25,888 + 4,101).
  - 0 of 36,888 vanilla files leave a byte unconsumed; the test was re-parsing each file with its last byte removed.
  - A leftover-bytes warning would therefore be zero-noise on vanilla.
- **Impact**: silent wrong material fields on mod or future content.
- **Related**: PAR-D4-2026-09-21-03
- **Suggested Fix**: after parse, `warn!` (with the path) when `remaining() > 0` or `version > 22`. Add both conditions to the FO4/FO76 sweep.

### PAR-D4-2026-09-21-03: Sweep gaps: no FO76 BGSM or BA2 sweep, no FNV MenuXml corpus, no Oblivion EGM test, and CDB asserts only non-zero counts
- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**: `crates/bgsm/tests/parse_all.rs:255-268`, `crates/bsa/tests/ba2_real.rs`, `crates/menuxml/tests/`, `crates/facegen/tests/parse_real_facegen.rs:178-227`, `crates/sfmaterial/tests/real_cdb.rs:83-90`
- **Status**: NEW
- **Trigger Input**: n/a.
- **Description**:
  - **FO76 BGSM.** Never swept. This audit measured 29,989/29,991 OK. The 2 failures are vanilla `.bgsm` files that are Material-Editor JSON text (`materials\atx\setdressing\atx_plushie_mr.fuzzy_valentinesday\*.bgsm`), rejected with BadMagic. Whether FO76's runtime reads JSON BGSM is unverified.
  - **FO76 BA2.** No parser-crate test; the DX10 path is never exercised on FO76. Data now holds 40 `.ba2` files after the 2026-09-20 rewrite.
  - **FO3 BSA.** The v104 BSAs have no test.
  - **FNV MenuXml.** The HUD profile ships (`hud.rs:148`, 121 menu XMLs) with zero tests.
  - **Oblivion FaceGen.** 141 EGMs, no test. The FNV/FO3 EGM test prints the non-finite count instead of asserting it, which would have caught PAR-D5-2026-09-21-01.
  - **CDB.** `real_cdb.rs` asserts only non-zero counts; the measured 97 classes / 1,438,780 values could be pinned.
- **Evidence**: the Gate Matrix below; probe logs `probe_bgsm_scan.log` and `probe_dup_scan.log`.
- **Impact**: format branches with vanilla content but no regression pin.
- **Related**: PAR-D4-2026-09-21-01, #3466
- **Suggested Fix**: add FO76 BGSM (with a JSON-form allowlist) and FO76 BA2 GNRL/DX10 sweeps, an FNV MenuXml corpus test, and an Oblivion EGM case. Pin the CDB counts.

### PAR-D4-2026-09-21-04: `archive_with_payload` leaks one temp BSA per test per run
- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**: `crates/bsa/src/archive/tests.rs:289-316`
- **Status**: NEW
- **Trigger Input**: every `cargo test -p byroredux-bsa`.
- **Description**: the helper writes `byroredux-bsa-#352-<pid>-<entry>.bsa` into `temp_dir()` and never removes it; the path is not returned. The `write_temp_v105` callers in the same file do call `remove_file`.
- **Evidence**: this audit's own unit run (`TMPDIR=/mnt/data/tmp`, 20:07) added 6 such files. 24 now sit in `/mnt/data/tmp` from 4 runs (Sep 14 ×3, Sep 21).
- **Impact**: temp-dir litter that accumulates on the default tmpfs `/tmp`.
- **Related**: #352
- **Suggested Fix**: return the path (or a guard that deletes on drop) and remove it after `BsaArchive` construction; the open file handle keeps the data readable on Unix.

### PAR-D5-2026-09-21-03: Renderer-relevant BGSM/BGEM fields authored in vanilla are dropped without a documented deferral
- **Severity**: LOW
- **Dimension**: Decode-Consumer Wiring
- **Location**: `byroredux/src/asset_provider/material/merge.rs:913-927`, `byroredux/src/asset_provider/material/mod.rs:82-145`
- **Status**: NEW
- **Trigger Input**: vanilla non-default values.
- **Description**:
  - #2704's ledger documents 11 BGSM scalars as "deferred: no consumer", and #2642 documents distance-field alpha.
  - The following are also dropped but undocumented (non-default counts from probe `bgsm-fields`):
    - FO76 `lum_emittance != 0`: 25,813 BGSM.
    - FO76 `use_adaptive_emissive`: 3,272. BGEM `adaptive_emissive_final_exposure_max`: 4,101. Together these are the v22 emissive model.
    - FO76 `base.depth_bias`: 230; `base.mask_writes != ALL`: 57.
    - FO4 `decal_no_fade`: 356; `dissolve_fade`: 23; `glowmap`: 154 (FO4) / 289 (FO76).
    - BGEM `falloff_color_enabled`: 2 / 107; `envmap_min_lod`: 11 / 19.
    - `cast_shadows=false`: 327 / 778. NIF-side `Cast_Shadows` is also unread.
  - Correctly ignored: `receive_shadows=false` appears on 6,552/6,616 FO4 and 25,888/25,888 FO76 BGSMs, so it cannot mean "unshadowed".
- **Evidence**: `probe_bgsm_fields.log`.
- **Impact**: no per-field visual claim is made (runtime semantics unverified). The gap is that the "not yet wired vs overlooked" ledger #2704 created omits them.
- **Related**: #2704, #2642, #1077, PAR-D5-2026-09-21-02
- **Suggested Fix**: extend the #2704 comment (or a doc table) with these fields and their vanilla counts, or wire the ones with clear renderer sinks (`depth_bias`, `mask_writes`).

### PAR-D5-2026-09-21-04: FaceGen EGT and TRI parsers mis-describe their formats (latent: no consumer)
- **Severity**: LOW
- **Dimension**: Decode-Consumer Wiring
- **Location**: `crates/facegen/src/egt.rs:13-27`, `crates/facegen/src/egt.rs:142-151`, `crates/facegen/src/tri.rs:17-36`, `crates/facegen/src/tri.rs:80-98`
- **Status**: NEW
- **Trigger Input**: vanilla `.egt` / `.tri`.
- **Description**:
  - **EGT.**
    - The FaceGen SDK manual gives each mode as `float s, <image> r, <image> g, <image> b` with `<image> = (signed char * R) * C`: planar and signed.
    - The parser pushes interleaved `[bytes[o], bytes[o+1], bytes[o+2]]` triples, and its docs describe an offset-128 unsigned reading.
    - The SDK header order is R, C, S, A, Texture Basis Version (vanilla 256, 256, 50, 0, 81). The parser calls A and the basis version `unknown_a/unknown_b`, and sizes the file from S only.
  - **TRI.**
    - The SDK header order is V, T, Q, LV, LS, X, ext, Md, Ms, K.
    - `TriHeader` stores X as `num_modifier_vertices`, ext as `num_modifiers`, Md as `num_uv_coords` and Ms as `num_quads`; Q, LV, LS and K become unknown words. Vanilla `headhuman.tri` words: 1211, 2294, 0, 0, 0, 1211, 1, 38, 8, 238.
    - The module doc and the code also disagree with each other on the order.
- **Evidence** (probe `egt-stats`, vanilla `headhuman.egt`, first 10 modes, first third read as `i8`):
  - lag-1 correlation 0.993 > lag-3 0.952, which means planar (interleaving would make lag 3 the same-channel neighbour);
  - lag-256 (next row) 0.972 > lag-768 0.840;
  - 58.7% of bytes are within 16 of 0x00/0xFF versus 0.8% near 0x80, i.e. zero-centred signed chars.
- **Impact**: latent. There is no consumer (#3544), but the future FGTS compositor and lip-sync `.tri` work would inherit wrong decodes.
- **Related**: #3544, PAR-D5-2026-09-21-01
- **Suggested Fix**: decode EGT as planar `i8` planes (R·C each) and rename the TRI header fields to the SDK order. Add the vanilla header words as a real-data assertion.

### PAR-D5-2026-09-21-05: UVD docs disagree, including a stale skill line and a field doc that contradicts its own module
- **Severity**: LOW
- **Dimension**: Decode-Consumer Wiring
- **Location**: `crates/bsa/src/uvd.rs:157-164`
- **Status**: NEW
- **Trigger Input**: n/a.
- **Description**:
  - `UvdHeader::bounds_min`'s doc says the box is "Not quantised … a tight content bound, not a grid-aligned cell volume". The module doc (`:79-101`, 2026-09-15) and `exterior_cell_grid()` establish that exteriors **are** a grid-aligned 3×3 block (1,095/1,095).
  - `byroredux/src/cell_loader/precombined.rs:50-73` says no consumer exists, and `parse_uvd_header` has no non-example caller. Yet `.claude/commands/audit-parsers/SKILL.md` (Dim 5) says "UVD: envelope only, consumed in `cell_loader/precombined.rs`".
- **Evidence**: `grep -rn parse_uvd_header byroredux/src crates` shows only `crates/bsa/examples/probe_uvd_corpus.rs`.
- **Impact**: misleading docs for the future previs consumer and for the next audit run.
- **Related**: #3810
- **Suggested Fix**: fix the field doc to match the module doc (exterior vs interior). Correct the skill bullet to "envelope only, no consumer yet".

### PAR-D6-2026-09-21-01: BA2 never checks `name_table_offset` against the file length, so a truncated mod archive fails with a bare "failed to fill whole buffer"
- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**: `crates/bsa/src/ba2.rs:194`, `crates/bsa/src/ba2.rs:301`
- **Status**: NEW
- **Trigger Input**: `name_table_offset` beyond EOF.
- **Description**: `reader.seek(SeekFrom::Start(name_table_offset))?` is followed by `read_exact` with no `> file_len` check. The BSA sibling names the equivalent field before seeking (`archive/open.rs:110-131`, #3368).
- **Evidence** (probe `dup-scan` over the installed FO4 Data): `cuwp - textures.ba2` (BTDX v1 DX10, 338 files) declares `name_table_offset` 1,250,980,735 in a 214,135,265-byte file, which looks like a truncated download. `Ba2Archive::open` → `Err("failed to fill whole buffer")`, surfaced as "BA2 '<path>': failed to fill whole buffer".
- **Impact**: correct rejection, uninformative error. The operator cannot tell a truncated archive from a reader bug.
- **Related**: #3368, PAR-D1-2026-09-21-07
- **Suggested Fix**: validate `name_table_offset <= file_len` (and record offset plus size) at open, with a named error mirroring #3368.

### PAR-D6-2026-09-21-02: Duplicate names inside one archive overwrite silently, and BSA keys are not separator-normalised at open
- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**: `crates/bsa/src/archive/open.rs:361-369`, `crates/bsa/src/ba2.rs:323-326`, `crates/bsa/src/archive/open.rs:252`
- **Status**: NEW
- **Trigger Input**: two records whose normalised names collide; or a third-party BSA storing `/` in a folder name.
- **Description**:
  - `HashMap::insert` is last-wins with no log. The skill asked whether this is logged, and it is not.
  - BSA keys are only `to_lowercase()`d at open. BA2 keys go through `normalize_path` (`ba2.rs:309`). A `/` in a BSA folder name would produce keys that no normalised query can reach.
- **Evidence** (probe `dup-scan`): 441 installed archives across all eight titles (vanilla plus installed mods): 0 with declared count ≠ distinct keys.
- **Impact**: hygiene only on current content.
- **Related**: #3637 (the shadow-count logging precedent)
- **Suggested Fix**: count overwritten keys at open and log once per archive, and apply `normalize_path` to BSA keys at open.

### PAR-D6-2026-09-21-03: BGSM/BGEM strings decode as strict UTF-8, so one non-UTF-8 byte in any path drops the whole material
- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**: `crates/bgsm/src/reader.rs:94-98`
- **Status**: NEW
- **Trigger Input**: a CP1252 (or otherwise non-UTF-8) byte in any BGSM/BGEM string.
- **Description**: `String::from_utf8(..)` → `Error::InvalidString` fails the whole parse, and the material falls back to NIF defaults with a warning. The reference Material-Editor reads with .NET `BinaryReader.ReadChars` under a replacement-fallback UTF-8 decoder (`BaseMaterialFile.cs:326-336`), which is lossy. This repo's BSA/BA2 name tables decode lossily too, so a lossy path could still match the archive key.
- **Evidence**: vanilla 0 of 36,888 FO4/FO76 materials hit `InvalidString` (probe `bgsm-scan`).
- **Impact**: mod-content only. Every texture slot of an otherwise valid material is lost over one byte.
- **Related**: PAR-D6-2026-09-21-02
- **Suggested Fix**: decode with `from_utf8_lossy` (matching the reference and the archive readers) and warn once per file when replacement occurred.

### PAR-D6-2026-09-21-04: game-detect has unbounded VDF recursion and joins ACF `installdir` without containment, and unreadable manifests vanish silently
- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**: `crates/game-detect/src/vdf.rs:107-159`, `crates/game-detect/src/steam.rs:157-158`, `crates/game-detect/src/steam.rs:82`, `crates/game-detect/src/steam.rs:139`
- **Status**: NEW
- **Trigger Input**: deeply nested `{` in `libraryfolders.vdf`/`appmanifest_*.acf`; an absolute or `..` `installdir`; a non-UTF-8 manifest.
- **Description**:
  - `parse_entries` recurses once per block with no depth cap.
  - `steamapps.join("common").join(install_dir)`: an absolute `installdir` replaces the base entirely, not just escaping it via `..`, and only `is_dir()` gates the result before it is reported as a detected install.
  - `let Ok(text) = read_to_string(..) else { continue }` drops unreadable or non-UTF-8 manifests without a log line, whereas parse errors do warn.
- **Evidence**: see the locations. The skill already records the recursion and join as Steam-written, LOW.
- **Impact**: requires a tampered Steam install. The launcher could report an install outside the library, or crash on a pathological VDF.
- **Related**: `/audit-tooling` Dim 5 (policy owner)
- **Suggested Fix**: add a depth cap (for example 32) to `parse_entries`, and reject `installdir` values that are absolute or contain `..` components. Log a debug line on manifest read failures.

---

## Gate Matrix

| Crate / format | Size caps | Error policy | Version gate | Corpus sweep (strict lane) | CI lane |
|---|---|---|---|---|---|
| bsa: BSA | ✓ `checked_entry_count` / `checked_chunk_size` / `inflate_bounded(_zlib)` (absolute, D1-07) | ✓ labelled `Err`; consumers swallow extract errors (D2-01); duplicates silent (D6-02) | ✓ allowlist 103/104/105, LZ4 frame on v105 only | Oblivion all 17; SSE Meshes0; FNV single file + in-crate; **FO3 none** | ✗ (synthetic fixtures only) |
| bsa: BA2 | ✓ + per-record total (#2356); ✗ DX10 header `u32` overflow (D1-03) | ✓; name-table offset unchecked (D6-01); DX10 short-decode warning without a path (D2-02) | ✓ {1,2,3,7,8}, method 0/3 | FO4 v8 brute force; Starfield 129/129; **FO76 none** | ✗ |
| bsa: CSG | ✓ `read_psg` PSG-space bound (#3758), compressed-read tail clamp | ✓ short interior chunk → `Err` (#1986) | magic | FO4 object 0 (DLC CSGs none) | ✗ |
| bsa: UVD | ✓ relation checks | ✓ | magic | example only | ✗ |
| bgsm | ✓ string ≤ remaining; template depth 16 + visited (#1148/#2701) | ✓ offset-carrying `Err`; strict UTF-8 (D6-03) | per-field gates, **no ceiling / no leftover check** (D3-01) | FO4 100% floor; **FO76 none** (29,989/29,991 measured) | ✗ |
| sfmaterial | ✓ chunk/field clamps (#2614/#2623); **✗ recursion depth** (D1-06) | ✓ typed errors; `probe_header` tolerant (#4273) | ✓ v4, BE reject, vocab pins | non-zero counts only | ✗ |
| hkx | counts capped (4096 bones/tracks, 16M samples); **✗ owned strings** (D1-01); sample cap loose (D1-02) | ✓ labelled; annotation skip (#3018) | ✓ #4332 gates, pointer-derived layout | LE + SE cart family (not strict) | ✗ |
| facegen | ✓ caps + exact size | ✓ (consumer logs at debug) | magics; **EGM numeric type wrong** (D5-01); EGT/TRI mislabelled (D5-04) | FNV/FO3 headhuman only; **Oblivion none** | ✗ |
| menuxml | depth caps 48/64; **✗ include expansion** (D1-04) | **✗ mid-char panic** (D1-05); **✗ `.fnt` panic** (D2-03) | n/a | Oblivion (env-only), FO3; **FNV none** | ~ (plain `#[test]` runs in `ci.yml` but skips without data) |
| game-detect | ✗ VDF recursion (D6-04) | ✓ malformed rejected | n/a | unit only | ✓ unit tests in `ci.yml` |

## Guards confirmed live (not `#[ignore]`d, passing)

- **Size ceilings and decoders:** `entry_count_rejects_attacker_u32_max`, `over_ratio_payload_is_rejected_at_the_ceiling`, `malicious_bsa_folder_count_u32_max_rejected`, `malicious_file_count_u32_max_rejected_before_allocation`, `lz4_flex_is_pinned_to_the_safe_decoder` (workspace `lz4_flex`: `default-features = false` + `safe-decode`), `lz4_decompress_is_panic_guarded`, `oversized_read_len_is_rejected_before_it_is_reserved`, `decode_spline_animation_rejects_a_sample_count_bomb`, `rejects_morph_count_over_cap`, `parse_with_limits_rejects_object_tree_before_materialising_it`.
- **Cycles:** `include_cycles_terminate`, `resolve_breaks_self_reference_cycle`.
- **Short decodes and corrupt streams:** `short_decode_stays_ok_for_the_shipped_padding_deltas`, `decompress_chunk_zlib_short_stream_returns_actual_length`, `decompress_chunk_lz4_under_run_returns_actual_length_not_declared`, `corrupt_adler32_trailer_recovers_via_raw_deflate`, `corrupt_deflate_body_still_errors`, `read_annotations_skips_an_out_of_range_time_and_keeps_the_rest`, `extract_rejects_compressed_payload_too_short`.
- **Version and layout gates:** `synthetic_v105_block_codec_payload_is_rejected_by_frame_reader`, `unknown_version_rejected`, `v3_unknown_compression_method_rejected`, `build_dds_header_is_148_bytes`, `chunk_type_recognized_set_is_pinned`, `builtin_type_recognized_set_is_pinned`, `class_flags_known_mask_is_pinned`, `probe_header_tolerates_an_unrecognized_chunk_type`, `rejects_packfiles_that_are_not_havok_2010_msvc_layout`, `layout_walk_reproduces_the_skyrim_se_offsets`, `rejects_size_count_mismatch`, `convert_hkx_clip_drops_only_the_out_of_range_bound_track`.
- **Paths, precedence and parsing:** `siblings_*` (7), `every_content_provider_resolves_collisions_last_wins`, `malformed_documents_are_rejected_rather_than_half_read`.
- Only one implementation of `MAX_ENTRY_COUNT` / `MAX_CHUNK_BYTES` / `MAX_RECORD_TOTAL_BYTES` exists (`crates/bsa/src/safety.rs`).

## Publishing

`/audit-publish docs/audits/AUDIT_PARSERS_2026-09-21.md`

| Finding | Suggested labels |
|---|---|
| PAR-D1-01 | `high` `bug` `animation` `safety` `game:skyrim` |
| PAR-D1-04, PAR-D1-05, PAR-D2-03 | `high` `bug` `ui` `safety` |
| PAR-D5-01 | `high` `bug` `import-pipeline` `character` `game:fnv` `game:fo3` `game:oblivion` |
| PAR-D5-02 | `high` `bug` `nifal` `game:fo4` `game:fo76` |
| PAR-D1-02 | `medium` `bug` `animation` `memory` `game:skyrim` |
| PAR-D1-03 | `medium` `bug` `import-pipeline` `safety` |
| PAR-D1-06 | `medium` `bug` `nifal` `safety` `game:starfield` |
| PAR-D2-01 | `medium` `bug` `import-pipeline` |
| PAR-D4-01, PAR-D4-02 | `medium` `bug` `test-gap` |
| PAR-D1-07, PAR-D2-02, PAR-D6-01, PAR-D6-02 | `low` `bug` `import-pipeline` |
| PAR-D2-04 | `low` `bug` `import-pipeline` `tech-debt` `test-gap` |
| PAR-D3-01, PAR-D5-03, PAR-D6-03 | `low` `bug` `nifal` (add `game:fo76` where FO76-specific) |
| PAR-D4-03, PAR-D4-04 | `low` `bug` `test-gap` |
| PAR-D5-04 | `low` `bug` `import-pipeline` |
| PAR-D5-05 | `low` `documentation` `doc-rot` |
| PAR-D6-04 | `low` `bug` `tech-debt` |

Label caveats (to flag in the publish summary): BSA/BA2/CSG/FaceGen have no label of their own and map to `import-pipeline`; game-detect maps to `tech-debt`.
