# Parser Discipline Audit: 2026-10-08

**HEAD**: 00f580e09 · **Baseline**: `AUDIT_PARSERS_2026-10-05.md` (HEAD a2c24b16e) · **Audited**: Dims 1–6. Each dimension's Paths had commits since a2c24b16e: #5314, #5316, #5319, #5320, #5323, the `b24cb46b6` clippy migration, the #4277 loose `.mat` resolver (`c2f28e06c`), and consumer-side deltas from #5079, #5095, #5222, #5366 and #5367. The menuxml DDS decoder (`crates/menuxml/src/tex.rs`) was audited in full at the orchestrator's request. · **Unchanged since baseline (skimmed)**: none at dimension level. Within the dimensions, the BSA/BA2/CSG readers, `crates/bgsm/src/`, `crates/facegen/src/` and `crates/game-detect/src/{vdf,steam}.rs` have no commits since baseline; their guards were spot-checked by running the suites.

This run is one leg of `/audit-suite --preset comprehensive`. Per the suite rules, every dimension was analysed in this session with no sub-agents. Per-dimension notes are in `/tmp/audit/parsers/dim_{1..6}.md`. `/tmp/audit/parsers` is left in place for the orchestrator to reconcile. No engine binary was launched.

## Executive Summary

**NEW findings: 0 CRITICAL · 1 HIGH · 2 MEDIUM · 2 LOW.**

**Already tracked and still live (5):** #5009, #5010 and #5011 are unchanged in code. #5326 is partly addressed: the census now asserts samples, but the other gaps remain. #5329 is unchanged: the all-candidate cycle check survives #5314.

- **HIGH: the menuxml DDS decoder panics on L8, A8 and any narrow-mask uncompressed DDS in dev/test builds, and decodes L8 as red in release (PAR-D2-2026-10-08-01).**
  - `scale()` in `decode_uncompressed` computes `2 * bits - 8` in `u32`. `shift_of(0)` reports a zero mask as 1 bit wide. Any uncompressed DDS whose R, G or B mask is zero (L8, A8), or whose mask is under 4 bits (A2R10G10B10), therefore hits "attempt to subtract with overflow".
  - Reachable from HUD menu art, font atlases and `tex.dump`. `tex.dump` decodes any game texture, and Oblivion ships 470 vanilla L8 DDS.
  - It does **not** share REN-D5-2026-10-08-01: this decoder honours all four masks.
- **MEDIUM: refused include fragments are retained (PAR-D1-2026-10-08-01).** #5314's fix bounds bytes spliced, not bytes fetched. A fragment of 64 KiB or less that the byte budget refuses stays in the per-document cache, and the refusal costs no budget. Measured: 10,000 distinct includes fetch 655 MB and raise VmHWM from 3.0 MB to 644.6 MB.
- **MEDIUM: 16-bpp menu art never decodes (PAR-D3-2026-10-08-01).** 49 vanilla Oblivion menu textures are A4R4G4B4 or R5G6B5: 21 magic icons, 20 world-map icons, the book background, map pages, `icon_small_eye` and `healthbar3dbw`. The decoder returns `None` for them, and the HUD logs this as "not found".

**Prior findings (AUDIT_PARSERS_2026-10-05, issues #5314–#5329):**
- **Fixed, with the fix in place in code:** #5314, #5316, #5319, #5320, #5323.
- **Still open:** #5326 (partly addressed) and #5329 (unchanged).

**Crates swept:** `byroredux-bsa`, `-bgsm`, `-sfmaterial`, `-hkx`, `-facegen`, `-menuxml` (parse side, plus `tex.rs`) and `-game-detect`.
- **Consumers traced:**
  - `byroredux/src/asset_provider/material/{cdb,merge,loose_mat,provider}.rs`
  - `asset_provider/{audio,texture,script}.rs`
  - `hud.rs`
  - `commands/assets.rs` (`tex.dump`)
  - `npc_spawn/resumable/`

**Unit tests** were run on toolchain 1.96.0 with an isolated target dir: `cargo test -p byroredux-{bsa,bgsm,sfmaterial,hkx,facegen,menuxml,game-detect}`. **All green: 342 passed, 50 ignored, plus 6 doctests.**

| Crate | Passed | Ignored |
|---|---|---|
| bsa | 111 | 31 |
| bgsm | 33 | 3 |
| sfmaterial | 44 | 3 |
| hkx | 25 | 3 |
| facegen | 31 | 4 |
| menuxml | 42 | 6 |
| game-detect | 56 | 0 |

`cargo test -p byroredux --bin byroredux -- asset_provider convert_hkx_clip loose_mat tex_dump`: 232 passed, 9 ignored.

**Real-data suites** ran in the strict lane: `BYROREDUX_REQUIRE_GAME_DATA=1`, `--release`, `--test-threads=1`, one crate at a time, all seven `BYROREDUX_<GAME>_DATA` variables plus `BYROREDUX_SKYRIMLE_DATA` set, matching the CI `parsers` job. **50 tests, all green.**

| Crate | Tests | Notes |
|---|---|---|
| bsa | 31 | 11 in-crate, 11 `ba2_real`, 8 `bsa_real` (2 new #5367 voice tests), 1 `csg_real` |
| bgsm | 3 | FO4: 6,616/6,616 BGSM and 283/283 BGEM. FO76: 29,989/29,991, plus 2 JSON files in the allowed bucket |
| sfmaterial | 3 | Base CDB: 500,403 keys in 1.6 s, HWM 239 MB. DLC: 500,385 keys. Both pass #5320's `RowInstanceMismatch` check, so the vanilla row/instance join is aligned |
| hkx | 3 | Census: 6,126 clips, 1,573 skipped. Maximums: mfpb 256, frames 1,471, blocks 15, samples 124,821, so the new 1 M cap holds |
| facegen | 4 | |
| menuxml | 6 | 89 documents, 1,868 tiles |

**Real-data suites skipped:** none.

**Probes:** each probe is a throw-away crate in the session scratchpad that uses path dependencies on the repo crates and the isolated target `/mnt/data/tmp/parsers-probe-target`.
- `menuprobe` (`main`, `dds`, `ddscensus`, `bsagrep`) drives the real public API: `parse_document`, `Rgba8::decode_dds`, `BsaArchive`.
- `hkxmut` is a mutated copy of `crates/hkx`. The tree was not modified.

**Cross-audit dedup:**
- Checked against `/tmp/audit/issues.json` (113 open), closed-issue searches ("menuxml DDS 16-bit", "A4R4G4B4", "luminance DDS") and today's `AUDIT_{RENDERER,SAFETY,…}_2026-10-08.md`.
- **REN-D5-2026-10-08-01/02 (renderer `parse_dds`)** were not re-filed. The menuxml decoder is a separate implementation:
  - **Mask gap: not shared.** All four masks are read, so the 32-bpp A8R8G8B8 layout decodes correctly.
  - **Luminance gap: partly shared, with a different symptom.** The renderer rejects L8; menuxml accepts it and then mis-decodes it, or panics in debug builds.
- **SAFE-D2-2026-10-08-01** (Scaleform `DefineSprite` recursion, `crates/ui`) is outside this skill's crates.
- #1542 (closed) fixed 16/24-bpp only in the renderer's `parse_dds`. The menuxml decoder never received it.

---

## Findings

### PAR-D2-2026-10-08-01: menuxml `decode_uncompressed` underflows a `u32` on any zero or narrow channel mask, which panics in dev/test builds; release decodes L8 as red
- **Severity**: HIGH
- **Dimension**: Error Semantics
- **Location**:
  - `crates/menuxml/src/tex.rs:300-319`: `shift_of` returns `(0, 1)` for a zero mask, and `scale` computes `(v >> (2 * bits - 8).min(bits))`.
  - `crates/menuxml/src/tex.rs:332-343`: the luminance test `r_mask == b_mask && g_mask == r_mask`.
  - Production callers:
    - `crates/menuxml/src/menu.rs:459` (menu art)
    - `crates/menuxml/src/font.rs:88` (font-atlas DDS fallback)
    - `byroredux/src/hud.rs:574` (compass strip)
    - `byroredux/src/commands/assets.rs:421` (`tex.dump`, which decodes any archive texture)
- **Status**: NEW
- **Trigger Input**: an uncompressed DDS header (`DDPF_FOURCC` clear) with `RGBBitCount` 8, 24 or 32 where any of R, G or B is 0, or any non-zero mask is under 4 bits wide. Examples:
  - The standard `DDSPF_L8` header: flags `0x20000`, masks `0xFF, 0, 0, 0`.
  - `DDSPF_A8`: masks `0, 0, 0, 0xFF`.
  - A2R10G10B10: the 2-bit alpha mask `0xC0000000`.
- **Description**:
  1. `bits` is a `u32`. For `bits < 4`, `2 * bits - 8` underflows. The dev profile keeps overflow checks on (`[profile.dev]` sets `opt-level = 1` only), so this panics with "attempt to subtract with overflow".
  2. `scale` runs unconditionally for R, G and B. A zero colour mask, reported as `bits = 1`, therefore always reaches it. Only alpha is guarded, by `a_mask == 0`.
  3. In release the subtraction wraps and `.min(bits)` hides it. The result is still wrong:
     - The luminance branch requires all three colour masks to be equal and non-zero. The standard L8 header has G = B = 0, so it never matches, and an L8 texel decodes as `(L, 0, 0)`.
     - A 2-bit alpha of 3 expands to 192 instead of 255.
  4. Neither the HUD driver nor the debug-command path runs under `catch_unwind`, so in a dev build (`cargo run`, the documented default) the panic takes down the engine's main thread.
  5. The strict real-data lane runs `--release`, so overflow checks are off there. No unit fixture covers an 8-bit or zero-mask header.
- **Evidence**:
  - **Probe** (`menuprobe --bin dds`, real `Rgba8::decode_dds`):
    ```
    (dev)     L8 grey 0x80: PANIC "attempt to subtract with overflow"
    (dev)     A8 0x80: PANIC
    (dev)     A2R10G10B10 white opaque: PANIC
    (release) L8 grey 0x80: Some([128, 0, 0, 255])      # expected [128,128,128,255]
    (release) A8 0x80: Some([0, 0, 0, 128])
    (release) A2R10G10B10 white opaque: Some([255, 255, 255, 192])   # expected alpha 255
    ```
  - **Vanilla census** (`menuprobe --bin ddscensus`, every uncompressed `.dds` header in the textures and misc BSAs):
    - Oblivion has **470** L8 files (flags `0x20000`, masks `000000ff,0,0,0`), for example `textures\clutter\voidessence_g.dds`.
    - No vanilla menu path in Oblivion, FO3 or FNV carries an 8-bit DDS. The vanilla HUD is therefore unaffected, and the vanilla trigger is `tex.dump` on any of those 470 files.
- **Impact**:
  - **Dev/test builds:** an engine abort from one texture. It is reachable from:
    - `tex.dump` on 470 vanilla Oblivion textures;
    - any mod menu art, font atlas or compass strip authored as L8, A8 or a narrow-mask format.
  - **Release builds:** silently wrong colours for the same files: red-tinted luminance and under-opaque 2-bit alpha.
  - The skill's severity rule applies: a panic in an untrusted-input reader is HIGH.
- **Related**:
  - REN-D5-2026-10-08-02: the renderer's `parse_dds` rejects L8; this decoder accepts and then mis-decodes it.
  - REN-D5-2026-10-08-01: the mask gap is not shared.
  - #1542, PAR-D3-2026-10-08-01.
- **Suggested Fix**:
  - Treat a zero mask as "channel absent" and skip `scale`, the way alpha is already handled.
  - Compute the bit replication in signed or saturating arithmetic. Bits replicate as `v8 = v << (8 - b)`, then OR in `v8 >> b` repeatedly until 8 bits are filled.
  - Key luminance off `DDPF_LUMINANCE` (`0x20000`), or off `g_mask == 0 && b_mask == 0 && r_mask != 0` when `bit_count == 8`, and replicate L into R, G and B.
  - Check the payload length against `width * height * bpp` before `Rgba8::new`. Today a 128-byte header claiming 8192² reserves 256 MiB before the first bounds check.
  - Add fixtures for L8, A8 and A2R10G10B10 that run in the debug unit lane.

### PAR-D1-2026-10-08-01: #5314's byte budget refuses a fragment but still caches it, so distinct refused includes retain 64 KiB each without bound
- **Severity**: MEDIUM
- **Dimension**: Size Discipline
- **Location**:
  - `crates/menuxml/src/parse.rs:831`: `includes.cache.insert(key.clone(), Some(bytes.clone()))` runs before the budget check.
  - `crates/menuxml/src/parse.rs:844-850`: the byte-budget refusal, which consumes neither `budget` nor `bytes_spent`.
  - `crates/menuxml/src/parse.rs:771-781`: the pre-fetch checks, which only stop once `bytes_spent >= MAX_INCLUDE_BYTES`.
- **Status**: NEW. It is a residual of #5314, whose commit message claims to close "the retained-bytes vector #5007 opened". That holds only for fragments over 64 KiB.
- **Trigger Input**:
  - A menu document whose includes first splice just under 256 KiB, for example three 64 KiB fragments plus one of 65,535 B, giving 262,143 B spent.
  - It then carries K further `<include src="gN.xml"/>` elements naming distinct prefabs, each between 2 B and 64 KiB.
  - Each prefab can be a separate entry in a mod `Misc.bsa`. A 64 KiB repetitive body compresses to roughly 100 B.
- **Description**:
  - Because `bytes_spent` stays one byte under the cap, the early return never fires.
  - Every new distinct path is fetched (a main-thread archive extract and inflate), inserted into the cache as `Some(bytes)`, and only then refused.
  - The refusal does not decrement the 256-splice budget. The number of such fetches is therefore bounded only by the number of include elements in the root document, and every refused fragment stays resident until `parse_document` returns.
- **Evidence** (`menuprobe`, release build, real `parse_document`, where the source answers each `menus\prefabs\` probe with a 64 KiB comment-only fragment):
  ```
  k=0      root  117 B  fetches     4  fetched     262,143 B  parse 0.17 ms  VmHWM 2,660 kB -> 3,064 kB
  k=1000   root   25 KB fetches 1,004  fetched  65,798,143 B  parse  31 ms   VmHWM 2,660 kB -> 66,436 kB
  k=10000  root  259 KB fetches 10,004 fetched 655,622,143 B  parse 240 ms   VmHWM 3,000 kB -> 644,568 kB
  ```
  Growth is exactly 64 KiB of resident memory per distinct refused include.
- **Impact**:
  - Main-thread memory spike and stall at `--hud` load: about 640 MiB per 10,000 distinct entries. With roughly 100 B compressed per entry, a mod archive of about 1.5 MB is enough.
  - The parse output itself stays bounded by #5314.
  - It is less amplifying than the original #5314 vector (about 450:1 against archive bytes, compared with millions to one), and the memory is released at the end of the parse. That is why this is rated MEDIUM, not HIGH.
  - Vanilla is unaffected: the heaviest vanilla document splices 38,378 B.
- **Related**: #5314, #5007, #4650, #5329. The #5329 fix will touch the same resolve block.
- **Suggested Fix**: do either of the following, and pin the probe's k=1000 case with a counting `MenuFileSource`:
  - Run the `bytes_spent + len > MAX_INCLUDE_BYTES` check before caching, and cache a refused fragment as `None` or as a length-only marker.
  - Or charge every fetch against a document-wide **fetched**-bytes budget, for example 2× `MAX_INCLUDE_BYTES`.

### PAR-D3-2026-10-08-01: the menuxml DDS decoder has no 16-bpp support, so 49 vanilla Oblivion menu textures never decode, and the HUD reports them as "not found"
- **Severity**: MEDIUM
- **Dimension**: Version Gating (format coverage)
- **Location**:
  - `crates/menuxml/src/tex.rs:289-297` (`bytes_per_px` accepts only 8, 24 and 32).
  - `crates/menuxml/src/menu.rs:456-463`: `find_map(|p| assets.texture(p)).and_then(decode_dds)`, followed by the "not found in any resolution set" warning.
- **Status**: NEW. #1542 fixed 16/24-bpp only in the renderer's `parse_dds`.
- **Trigger Input**: vanilla `Oblivion - Textures - Compressed.bsa`, uncompressed 16-bpp headers:
  - 48 × A4R4G4B4: flags `0x41`, masks `0f00/00f0/000f/f000`.
  - 1 × R5G6B5: flags `0x40`, `textures\menus\misc\healthbar3dbw.dds`.
- **Description**:
  - The breakdown by folder:

    | Folder under `textures\menus\` | Files |
    |---|---|
    | `icons\magic` | 21 |
    | `map\world` | 20 |
    | `map` (including `main_page.dds`, `main_page_shadow.dds`) | 2 |
    | `map\log` | 2 |
    | `book` (including `book_background.dds`, `book_mark.dds`) | 2 |
    | `icons` (`icon_small_eye.dds`) | 1 |
    | `misc` | 1 |

  - Vanilla XML references some of them: `hud_reticle.xml` names `icon_small_eye.dds`, and `book_menu.xml` and `prefabs\scroll_line.xml` name `book_background.dds` (`menuprobe --bin bsagrep`). The magic-effect and world-map icons are fed by the runtime.
  - For each of these files `decode_dds` returns `None`, with only a debug-level log.
  - `MenuRenderer` then warns "menu texture '…' not found in any resolution set". The file was found but could not be decoded, so an operator is sent looking for a missing archive.
  - `find_map` also stops at the first **present** candidate, so a decodable sibling in another resolution set is never tried.
- **Evidence**: the `ddscensus` output in `/tmp/audit/parsers/dim_3.md`. FO3 and FNV have no 16-bpp DDS under `menus`. Their 16-bpp files are `textures\fonts\glow_*_lod_a.dds`, which the font profiles do not name.
- **Impact**:
  - **Today:** the Oblivion `--hud` driver loads only `hud_main_menu.xml`, so the live HUD is not affected yet. `tex.dump` fails on these files.
  - **When more Oblivion menus are driven:** the sneak eye (reticle), the book, map and log pages, and the magic and world-map icons will render blank. This is vanilla content failing to decode in a parser this skill owns. It is MEDIUM rather than HIGH only because no menu driven today references these files.
- **Related**: #1542, PAR-D2-2026-10-08-01, REN-D5-2026-10-08-02.
- **Suggested Fix**:
  - Accept `bit_count == 16` (2 bytes per pixel) through the same mask path once PAR-D2-2026-10-08-01's mask arithmetic is fixed. A4R4G4B4, R5G6B5, A1R5G5B5 and A8L8 then fall out of the generic masked decode.
  - Make the `menu.rs` warning distinguish "present but undecodable".
  - Add a header census of `textures\menus\` per legacy game to the menuxml corpus lane, asserting zero undecodable headers.

### PAR-D4-2026-10-08-01: three of the four HKX dimension-gate tests cannot fail if their own clause is removed
- **Severity**: LOW
- **Dimension**: Corpus Gates (test strength)
- **Location**: `crates/hkx/src/animation.rs:1538-1604` and `:1655-1711`, the fixtures of:
  - `decode_spline_animation_rejects_a_sample_count_bomb`
  - `decode_spline_animation_rejects_frames_beyond_the_declared_blocks`
  - `decode_spline_animation_rejects_a_block_claiming_its_whole_clip`

  The gate they target is `animation.rs:368-395`.
- **Status**: NEW
- **Trigger Input**: n/a (gate weakness).
- **Description**:
  - Every clause of the dimension gate returns the same `InvalidData("unsupported spline clip dimensions")`.
  - Three fixtures leave `mask_size` (offset `0x44`) at 0, so `mask_size != transform_count * 4 + float_count` rejects them whatever clause each test claims to pin.
  - The #5006 fixture also declares 15,998,976 samples, which the #5316 1 M cap rejects on its own.
  - The test comments claim "ONLY the relative check can reject it" (#4655) and "only the MAX_FRAMES_PER_BLOCK ceiling can reject it" (#5006). Both claims are false.
  - Only the new #5316 test, `rejects_the_279kb_spline_probe`, sets `mask_size` correctly and isolates its clause.
- **Evidence**: a mutation probe. In a scratch copy of `crates/hkx`, both the #4655 clause (`num_frames > num_blocks * (mfpb - 1) + 1`) and the #5006 clause (`mfpb <= MAX_FRAMES_PER_BLOCK`) were deleted. `cargo test --lib decode_spline_animation_rejects` then reports **4 passed, 0 failed**.
- **Impact**: the #4655 and #5006 hardening (one-block bypass, frames-vs-blocks tie) can regress with a green unit lane. The real-data census asserts only vanilla maximums, not rejection of hostile shapes.
- **Related**: #4655, #5006, #5316, #5326 (census weakness, separate).
- **Suggested Fix**:
  - Set `mask_size = transform_count * 4` in each fixture.
  - Size the #5006 fixture under 1 M samples, for example 4 tracks × 250,000 frames in 1 block with mfpb 250,001.
  - Ideally return a distinct error string per clause, so each test asserts the clause it pins.

### PAR-D3-2026-10-08-02: the loose `.mat` decode wraps the JSON slot index with `as u8` and reads the 6-digit hex alpha from the blue byte
- **Severity**: LOW
- **Dimension**: Version Gating (decode correctness)
- **Location**:
  - `byroredux/src/asset_provider/material/loose_mat.rs:69-71`: `get("Index").and_then(|v| v.as_u64()).map(|v| v as u8)`.
  - `loose_mat.rs:173-174`: `channel(6.min(hex.len().saturating_sub(2)))`.
- **Status**: NEW. It was introduced by `c2f28e06c` (#4277).
- **Trigger Input**:
  - A loose `.mat` JSON component with `"Index": 256` (or 257, …). It becomes slot 0 (`SLOT_COLOR`) or slot 1 (`SLOT_NORMAL`).
  - A `TextureReplacement` `"Color": "#RRGGBB"`. Its alpha is read from `BB`.
- **Description**:
  - An out-of-range index silently lands in a real slot, so a texture authored for slot 256 or above becomes the albedo or normal map.
  - The hex branch indexes alpha at `min(6, len - 2)`, which is 4 for a 6-digit string.
  - The alpha is discarded downstream (`[r, g, b, _a]` in `apply_cdb_material`), so the hex half is latent.
  - The module states that both spellings are unsourced: no vanilla loose `.mat` exists.
- **Evidence**: see the code locations above. `apply_cdb_material` maps slots 0 and 1 to the colour and normal roles.
- **Impact**: a malformed mod `.mat` gets a wrong texture role instead of a skipped component. Vanilla is unaffected.
- **Related**: #4277, #3398.
- **Suggested Fix**:
  - Use `u8::try_from(v).ok()` and skip the component, with a debug log, when it is out of range.
  - Default the alpha to `FF` when the hex string has fewer than 8 digits.

---

## Already-tracked findings re-verified

| Issue | State at 00f580e09 |
|---|---|
| #5009 debug-load raw extract loop | Unchanged: `byroredux/src/debug_load.rs:181` `if let Ok(data) = archive.extract(path)` |
| #5010 BGSM diagnostics silent on the template path | Unchanged: `crates/bgsm/src/template.rs:257` calls `parse_bgsm` |
| #5011 Oblivion EGM / BGSM sweep weakness | Unchanged: no commits to `crates/facegen/tests` or `crates/bgsm/tests` |
| #5326 HKX census and DLC CDB gate | Partly addressed. `max_samples` is now asserted (#5316). The census still has no `clips > 0` floor and no decode pass. `real_cdb.rs:167-170` DLC test still runs `if !dlc.exists() { SKIP; return }` under `REQUIRE` |
| #5329 cycle check over-approximation | Unchanged: `parse.rs:799-805` still tests all three candidate keys before resolving |
| #5073 empty compressed BSA payload (FO3 audit) | No commits to `crates/bsa/src/archive/` |

## Prior-finding verification (AUDIT_PARSERS_2026-10-05)

| Prior finding | Issue | State now |
|---|---|---|
| D1-01 MenuXml splice amplification | #5314 (closed) | Fixed. `MAX_INCLUDE_BYTES` 256 KiB, `MAX_INCLUDE_FRAGMENT_BYTES` 64 KiB checked before caching, and `MAX_DOCUMENT_TILES` 16,384. The residual is the refused-fragment retention in PAR-D1-2026-10-08-01 |
| D1-02 HKX absolute sample cap | #5316 (closed) | Fixed: `MAX_TRANSFORM_SAMPLES` 1,000,000 (`animation.rs:61`). The census asserts `max_samples` (124,821 ≤ 1 M) |
| D2-01 `cdb_material_index` `.ok()?` | #5319 (closed) | Fixed: open and extract errors are matched and warned before memoising. The format-string continuations are restored |
| D2-02 `MaterialIndex::build` silent degradation | #5320 (closed) | Fixed: four typed errors plus a trailing-bytes check. Both vanilla CDBs pass the row/instance check |
| D3-01 skip path in declaration order | #5323 (closed) | Fixed: `skip_user_class_body` walks `read_order`, pinned by `skip_and_read_consume_identically_on_a_reordered_class` |
| D4-01 vacuous gates | #5326 (open) | Partly addressed (see above) |
| D6-01 cycle over-approximation | #5329 (open) | Unchanged |

## Gate Matrix

| Crate / format | Size caps | Error policy | Version gate | Corpus sweep (strict lane) | CI lane |
|---|---|---|---|---|---|
| bsa: BSA | ✓ `checked_entry_count` / `checked_chunk_size` / `inflate_bounded(_zlib)`; file-relative `capacity_hint` | ✓ labelled `Err`; consumers via `extract_first`, except `debug_load.rs` (#5009) | ✓ {103,104,105}; LZ4 **frame** on v105 | Oblivion brute force; SSE Meshes0; FNV, including the new Voices1 floor; FO3 open+list; Skyrim `.fuz` shape | ✓ nightly `parsers` |
| bsa: BA2 | ✓ per-chunk and per-record caps | ✓ path-named; `chunk_hdr_len` hard `Err` | ✓ {1,2,3,7,8}; method 0/3 | FO4 v8 brute force; FO76; Starfield | ✓ nightly |
| bsa: CSG / UVD | ✓ | ✓ | magic | FO4 object 0; UVD has no consumer | ✓ nightly / n/a |
| bgsm | ✓ | ✓; **diagnostics silent on the template path** (#5010) | per-field gates | FO4 100%, FO76 100% plus 2 JSON; no diagnostic asserts (#5011) | ✓ nightly |
| sfmaterial (tree) | ✓; nesting 64 | ✓ typed | ✓; skip ≡ read order (#5323) | exact 97 / 1,438,780 | ✓ nightly |
| sfmaterial (`MaterialIndex`) | per-instance decode | ✓ typed join errors (#5320); consumer warns (#5319) | inherits | base: 500,403 keys + 5 slots; DLC: 500,385, **skip-green on absence** (#5326) | ✓ nightly |
| loose `.mat` (byroredux) | archive-bounded; serde_json depth 128 | tolerant decode; **`as u8` slot wrap** (D3-02) | n/a (unsourced dialect) | none (no vanilla sample) | bin unit only |
| hkx | strings ≤ 256 B; mfpb ≤ 256; samples ≤ 1 M | ✓ labelled; annotation skip | ✓ #4332 gates | census asserts mfpb + samples (#5326 remainder) | ✓ nightly; **dimension-gate unit tests vacuous** (D4-01) |
| facegen | ✓ caps + exact size | ✓ | ✓ magics; R/C fixed | FNV/FO3 pinned; Oblivion EGM weak (#5011) | ✓ nightly |
| menuxml (parse) | depth 48 / 64; splices 256; bytes 256 KiB; fragment 64 KiB; tiles 16,384; **refused fragments retained** (D1-01) | ✓ char-safe; warn-once | n/a | Oblivion, FO3, FNV (tile total printed, not pinned) | ✓ nightly |
| menuxml (`tex.rs` DDS) | dims ≤ 8192, but **allocates before the payload check** | **debug panic on zero/narrow masks** (D2-01); 16-bpp → `None`, logged as "not found" (D3-01) | FourCC DXT1/3/5; masked 8/24/32 | none (fo3_corpus decodes a few, in release) | release-only corpus, so overflow checks never run |
| game-detect | ✓ VDF depth 32 | ✓ | n/a | unit only | ✓ `ci.yml` |

## Guards confirmed live (not `#[ignore]`d, passing this run)

- **Size ceilings:**
  - `entry_count_rejects_attacker_u32_max`, `over_ratio_payload_is_rejected_at_the_ceiling`, `malicious_bsa_folder_count_u32_max_rejected`, `malicious_file_count_u32_max_rejected_before_allocation`.
  - `lz4_flex_is_pinned_to_the_safe_decoder`, `lz4_decompress_is_panic_guarded`, `oversized_read_len_is_rejected_before_it_is_reserved`.
  - `decode_spline_animation_rejects_a_sample_count_bomb` and `decode_spline_animation_rejects_a_block_claiming_its_whole_clip`. These pass, but see D4-01: they would also pass without their own clauses.
  - `decode_spline_animation_rejects_the_279kb_spline_probe` (new).
  - `rejects_morph_count_over_cap`, `parse_with_limits_rejects_object_tree_before_materialising_it`.
- **Nesting and includes:** `include_cycles_terminate`, `self_including_fragment_costs_one_fetch`, `budget_exhausted_includes_never_fetch`, the #5314 budget tests, `resolve_breaks_self_reference_cycle`, `self_referential_struct_is_rejected_instead_of_overflowing_the_stack`, `pathological_nesting_hits_the_depth_cap`.
- **Error semantics:**
  - `dx10_open_rejects_a_non_24_chunk_hdr_len_as_invalid_data`, `extract_dx10_rejects_a_short_non_final_chunk_naming_the_entry`, `short_decode_stays_ok_for_the_shipped_padding_deltas`.
  - `corrupt_adler32_trailer_recovers_via_raw_deflate`, `corrupt_deflate_body_still_errors`, `read_annotations_skips_an_out_of_range_time_and_keeps_the_rest`.
  - The five new #5320 `build_rejects_*` tests.
- **Version and layout:**
  - `synthetic_v105_block_codec_payload_is_rejected_by_frame_reader`, `unknown_version_rejected`, `v3_unknown_compression_method_rejected`, `build_dds_header_is_148_bytes`.
  - `chunk_type_recognized_set_is_pinned`, `builtin_type_recognized_set_is_pinned`, `class_flags_known_mask_is_pinned`, `read_order_reorders_xmcolor_shape`, `skip_and_read_consume_identically_on_a_reordered_class`.
  - `rejects_packfiles_that_are_not_havok_2010_msvc_layout`, `layout_walk_reproduces_the_skyrim_se_offsets`, `rejects_size_count_mismatch`, `non_square_header_maps_rows_to_height_and_columns_to_width`.
- **Paths:** `siblings_*` and `every_content_provider_resolves_collisions_last_wins` (within the 232 bin tests), `malformed_documents_are_rejected_rather_than_half_read`, `only_a_bare_directory_name_is_a_valid_installdir`.
- There is exactly one definition each of `MAX_ENTRY_COUNT`, `MAX_CHUNK_BYTES` and `MAX_RECORD_TOTAL_BYTES` (`crates/bsa/src/safety.rs`).

## Publishing

`/audit-publish docs/audits/AUDIT_PARSERS_2026-10-08.md`

| Finding | Suggested labels |
|---|---|
| PAR-D2-2026-10-08-01 | `high` `bug` `ui` `safety` |
| PAR-D1-2026-10-08-01 | `medium` `bug` `ui` `memory` `safety` |
| PAR-D3-2026-10-08-01 | `medium` `bug` `ui` `game:oblivion` |
| PAR-D4-2026-10-08-01 | `low` `bug` `test-gap` `animation` |
| PAR-D3-2026-10-08-02 | `low` `bug` `nifal` `game:starfield` |

## Summary

| Severity | NEW | Already tracked (live) |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 1 | 0 |
| MEDIUM | 2 | 0 |
| LOW | 2 | 5 (#5009, #5010, #5011, #5326, #5329) |
