# Asset Preprocessing Pipeline — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in review
round 1 (interior loads corrected to serial on the caller's thread; the
asset-pipeline / archives timing "disagreement" quoted and re-framed as two
different paths; the `mmap(2)` visibility quote attributed to `MAP_PRIVATE`
and bundle replacement re-grounded in `unlink(2)`; BA2 per-file compression
made a verify item; Windows identity inherited from XPLAT-PORT-007 rev 2 (no stable std API at 1.96.0, `FILE_BASIC_INFO.ChangeTime` the candidate, closing in XPLAT Phase 2); persisted-stamp
stability and external truncation added as risks; the **refused** state given
a manifest; the M72 hand-off in Q5 corrected against the Decentralized Mod
Hosting PRD; Q6 split into Q6 dependency and Q9 mapping; Q2 gains a bundle
checksum alternative; Q4 states the target-size-guard consequence; TXS-SRC-002
and TXS Q2 cited for the texture section; "bake" disambiguated from PGEN-BAKE
and author-time bakes). Revision 1 was the first draft: the M82 promise (a
one-shot, local bake of BSA / BA2 content into a memory-mappable bundle that
the asset provider serves in place of the archive), the measurement that has
to come first, requirements by area, phases and the decisions it needs from
the owner. Nothing below is built as a result of this document yet. Statements about how an original engine, a file format,
a tool or an external specification behaves cite an external source; a
claim that could not be verified is marked **Phase 0: verify** instead of
being stated as fact.

**Scope**: M82 in the [ROADMAP Tier 10 table](../../ROADMAP.md#tier-10--ecosystem-unlock-post-tier-4-horizon).
The engine can bake the archives a profile already opens into a local,
derived bundle per source archive, laid out so an entry (and, for a
texture, any mip range of it) is one contiguous, uncompressed, aligned byte
range. The asset provider serves a fresh bundle entry instead of reading
and inflating the archive entry, and falls back to the archive whenever the
bundle is absent, stale, corrupt or switched off. In scope: the Phase 0
measurement of where asset-load time goes, the bundle format and its
invalidation, the provider read path, asset precedence under mods, the bake
command, the disk budget, telemetry, the texture section M39 can read mip
ranges from, the derived sections Phase 0 justifies, and the legal
boundary. Out of scope: replacing the archive readers, baking plugins,
device-specific GPU objects, recompression or upscaling, defining M53's
cluster format, and distributing any bake (§4.2, decision Q5).

**Related**: this document states the *promise*; the designs below own the
*how*. The bundle format and the bake command have no design yet; Phase 0
writes it (`docs/engine/asset-bake.md`, to be registered in
[docs/engine/index.md](../engine/index.md)). The Virtual Geometry PRD that
will own M53's cluster format (`docs/prd/virtual-geometry.md`) is registered
in the index but not written at HEAD on 2026-11-01, so it is named, not
linked.

| Mechanism | Owning design |
|---|---|
| BSA / BA2 readers, compression per version, DX10 chunks and DDS header reconstruction | [Archives](../engine/archives.md) |
| Provider precedence, the loose tier, last-listed-wins | [Archives § Asset provider precedence](../engine/archives.md#asset-provider-precedence-and-the-loose-tier) |
| Worker extraction, texture prefetch, FO4 precombines on the worker, and their measured numbers | [Archives § Exterior streaming overlap](../engine/archives.md#exterior-streaming-overlap) |
| NIF extraction, parse and import at cell load | [Asset Pipeline](../engine/asset-pipeline.md) |
| On-disk cache precedent: `terrain-height/`, `BYRO_CACHE_DIR`, `GENERATOR_VERSION` | [EXAL — Terrain height maps § Disk cache](../engine/exal-terrain-height.md#disk-cache) |
| Starfield CDB material index cost; NIF import cache | [Memory Budget](../engine/memory-budget.md) |
| Single translate boundary for imported data | [NIFAL](../engine/nifal.md) |
| Mip-range reads and residency (M39) | [Texture Streaming — PRD](texture-streaming.md) |
| Asset precedence under mods (`LMC-AST-*`), the legal posture (`LMC-LEGAL-*`) | [Legacy Mod Compatibility — PRD](legacy-mod-compatibility.md) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `BAKE-` prefix.

---

## 1. Product thesis

Every asset the engine loads today is read out of the game's own archives
and, when the archive stored it compressed, inflated on the spot. A BSA
allows per-file compression: an archive flag means files are "compressed by
default", and "if bit 30 is set in the size, the default compression is
inverted for this file" (UESP, *Oblivion Mod:BSA File Format*; *Skyrim
Mod:Archive File Format* has the same rule; local dumps under
`/mnt/data/src/reference/uesp-wiki/`), with zlib for v103 / v104 and LZ4 for
v105 (same pages). For BA2, Archive2 takes a `-compression=<None|Default|XBox>`
setting per run (Fallout 4 CK wiki, *Archive2*); whether a BA2 can mix
compressed and stored records is **Phase 0: verify** (BAKE-MEAS-003). On
exterior streaming the engine already extracts on a pool and prefetches
textures ([Archives § Exterior streaming overlap](../engine/archives.md#exterior-streaming-overlap),
[§ Texture prefetch](../engine/archives.md#texture-prefetch)); an interior
load still extracts, parses and imports serially on the calling thread
(§3.3 row 14). Either way the work is paid again on every launch, for every
unique asset.

The milestone row promises that a one-shot bake makes runtime loads "10×
faster". That figure is an unmeasured ROADMAP claim, to be tested by Phase 0
(BAKE-MEAS-002), not a target this document adopts. The two designs that
describe the cost measure different paths and name different stages. The
asset pipeline's order-of-magnitude table for the synchronous interior load
gives "Per-unique-mesh NIF parse + import" as "the dominant CPU cost" and
"Texture extraction + decode (per unique texture)" as "small fraction of
total", and its next paragraph says "Load wall time is dominated by
per-unique-mesh GPU uploads"
([Asset Pipeline § End-to-end timing reference](../engine/asset-pipeline.md#end-to-end-timing-reference);
the doc was last reconciled 2026-05-28). The FO4 grid-cross measurements
say of the streaming worker, before extraction moved into its tasks,
"Extraction had been ~80% of that wall time, serial on the coordinator", and
that main-thread texture reads were "41% of streaming apply time"
([Archives § Exterior streaming overlap](../engine/archives.md#exterior-streaming-overlap),
[§ Texture prefetch](../engine/archives.md#texture-prefetch)). Neither
separates read from inflate, and neither covers the other's path. A bake of
inflated bytes speeds up only read and inflate; a bake of imported meshes
speeds up parse and import too, at a much higher maintenance cost. Which one
is worth building depends on a number nobody has yet.

The feature is for the player who loads a cell or crosses a grid boundary
on their own machine, and for the developer who restarts the engine many
times a day against the same install. It is not for distribution: the bake
is derived from the user's copy of the game and stays on that machine.

**Corollary that shapes every requirement below: the bake is a cache, never
a source.** It is measured before it is built, it changes nothing about
which file wins, it is invalidated per source archive and never by a load
order change, it is never required for a correct load, and it is never
shared. The engine will not read a bundle whose source is not on the
machine, will not let a bundle outrank a loose file, and will not ship or
upload a bake.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Source** | One archive the provider opens (a BSA or a BA2), identified by its path and its `ArchiveStamp` (`byroredux/src/asset_provider/archive.rs`) |
| **Bake** | The act of producing bundles from sources, and the set of bundles on disk. Not the Procedural Exteriors bake (generated cells written into a plugin, `PGEN-BAKE-*` in [procedural-exteriors.md](procedural-exteriors.md)) and not an author-time bake (FO4 precombines, prebaked `.btr` / `.bto` LOD). IDs here are `BAKE-<AREA>-NNN`; a plain search for `BAKE-` also matches `PGEN-BAKE-NNN`, so anchor it (`(^|[^-])BAKE-`) |
| **Manifest** | One file in the bake directory, written by every bake run, listing each source with its state (written, fresh, refused) and the reason for a refusal (BAKE-DISK-002) |
| **Bundle** | One file holding the baked entries of exactly one source, with a header that names that source and its stamp (§5.2) |
| **Entry** | One archive path inside a bundle, keyed the way the provider folds keys (lowercase, backslash-separated, relative to `Data`; [Archives § Asset provider precedence](../engine/archives.md#asset-provider-precedence-and-the-loose-tier)) |
| **Bake level** | What an entry holds: **inflated** or **derived** (§3.1) |
| **Section** | A group of entries of one level and kind inside a bundle: inflated generic, inflated texture, a derived kind |
| **Fresh / stale** | A bundle whose recorded stamp and format version match the source and reader / one where either differs |
| **Fall-through** | Serving an entry from the source archive's own reader because the bundle cannot serve it |
| **Cache root** | `byroredux_core::paths::cache_root()`: `BYRO_CACHE_DIR` when set, else the executable's directory (`crates/core/src/paths.rs`) |
| **Bake budget** | The byte ceiling on all bundles under the cache root (§5.7) |
| **Bakeable share** | The fraction of a measured load's wall time spent in stages a given bake level removes (§5.1) |
| **Imported / canonical tier** | NIFAL's tiers: the per-game `Imported*` decode and the resolved ECS-facing type produced by `translate()` ([NIFAL § 1](../engine/nifal.md#1-the-three-tier-model)) |

---

## 3. Core model

### 3.1 Bake levels

| Level | Holds | Keyed on | Removes at load | Phase |
|---|---|---|---|---|
| **inflated** | The bytes `Archive::extract` returns for the entry (for a BA2 DX10 record, the reconstructed DDS), stored uncompressed and aligned | The source's stamp and the bundle format version | read of the compressed payload + inflate | 1 |
| **derived** | Engine output computed from one or more source entries: a Starfield `MaterialIndex`, a decoded FO4 precombine CSG, an `Imported*` scene (decision Q1) | A generator version plus every input's identity, as `terrain-height/` keys on `GENERATOR_VERSION` and the raw source bytes ([EXAL § Disk cache](../engine/exal-terrain-height.md#disk-cache)) | the derived computation | 3, only where §5.1 justifies it |

The canonical tier is never baked: `translate()` stays its only producer
([NIFAL § The canonical-type rule](../engine/nifal.md#the-canonical-type-rule)).

### 3.2 Entry states

Every lookup that reaches a source with a bundle ends in exactly one state.
`bake.status` (BAKE-TEL-001) prints the per-source counts; the `bake:` bench
line (BAKE-TEL-002) prints the session totals.

| State | Meaning | Served from |
|---|---|---|
| **hit** | Bundle fresh, entry present and valid | the bundle |
| **absent** | No bundle for the source, or the entry is not in it (an entry stored uncompressed in the source, BAKE-FMT-006) | the source |
| **stale** | The bundle's stamp or format version does not match | the source; the bundle is not opened |
| **corrupt** | The bundle or entry fails validation (BAKE-READ-003) | the source; the bundle is disabled for the session |
| **refused** | The last bake run skipped the source because of the budget or the reserve, as its manifest records (BAKE-DISK-002, BAKE-DISK-003); without the manifest entry the lookup would be indistinguishable from **absent** | the source |
| **off** | The bake switch is off (BAKE-READ-005) | the source |

### 3.3 Inventory at HEAD

The inventory below is the state at HEAD on 2026-11-01 (`66550cf8a`). It is
hand-written; Phase 1 replaces rows 1, 6 and 10 with the output of
`bake.status` and the `bake:` bench line.

| # | Piece | Today |
|---|---|---|
| 1 | Entry read | `ArchiveInner::extract` (`byroredux/src/asset_provider/archive.rs`) returns an owned `Vec<u8>` per call; `BsaArchive::extract` (`crates/bsa/src/archive/extract.rs`) reads the payload and inflates zlib (v103/v104) or an LZ4 frame (v105) through the bounded inflaters in `crates/bsa/src/safety.rs`; `Ba2Archive::extract` (`crates/bsa/src/ba2.rs`) inflates zlib (`inflate_bounded_zlib`) or an LZ4 block (`lz4_flex::block::decompress`) per GNRL record or DX10 chunk. Nothing inflated is kept across a session |
| 2 | Positional reads | `crates/bsa/src/read_at.rs`: `pread` on Unix, so tasks read in parallel; a serialised handle elsewhere (#4999) |
| 3 | Source identity | `ArchiveStamp` (length, mtime and, on Unix, device, inode, ctime) gates reuse of an opened handle in `Archive::open_shared`; neither format carries a checksum (#5663, module doc beside `ArchiveStamp`) |
| 4 | Loose tier | `Backend::Loose` over `LOOSE_ASSET_ROOTS` (four roots), appended after every archive so it wins (#6200) |
| 5 | In-memory caches | `NifImportRegistry` holds imported scenes under an LRU entry cap ([Memory Budget § NIF Import Cache](../engine/memory-budget.md#nif-import-cache) owns the cap and override); `PrefetchStore` stages extracted DDS bytes and is cleared when a cell's apply ends (`asset_provider/texture_prefetch.rs`) |
| 6 | On-disk caches | two, both under the cache root: `terrain-height/` (`asset_provider/terrain_height.rs`, `default_cache_dir`, key = xxHash64 over `GENERATOR_VERSION`, tier, convention and source bytes; no size bound and no eviction) and `pipeline_cache.bin` (`crates/renderer/src/vulkan/context/helpers.rs`, `pipeline_cache_path`) |
| 7 | Atomic publish | `byroredux_core::atomic_file::atomic_write` with a per-write temp name (`atomic_temp_path`, #6290), used by the terrain-height cache |
| 8 | Memory mapping | none in any workspace crate; `memmap2` is in `Cargo.lock` only through winit's Wayland dependencies |
| 9 | Timing instruments | `Cell load phases:` info line from `CellLoadPhaseTimings` (`byroredux/src/cell_loader/load.rs`, #3559): its `references` phase bundles NIF import, texture resolve, collider and BLAS build. The worker's `[stream-worker] … pipeline:` debug line (`byroredux/src/streaming/pre_parse.rs`) reports `extract_task_sum_ms` (read and inflate together) and `parse_task_sum_ms`. The `streaming:` bench line (`byroredux/src/streaming/telemetry.rs`) reports `apply_tex_extract_*` |
| 10 | Starfield CDB | `MaterialIndex::build` (`crates/sfmaterial/src/index.rs`) runs once per discovered CDB per session behind `sf_cdb_index_cache` (`asset_provider/material/cdb.rs`); its cost is in [Memory Budget § Starfield Component Database](../engine/memory-budget.md#starfield-component-database-cpu-side) |
| 11 | FO4 precombine CSG | `decode_precombine_csg` (`byroredux/src/cell_loader/precombined.rs`) decodes on the worker every session ([Archives § FO4 precombines on the worker](../engine/archives.md#fo4-precombines-on-the-worker)) |
| 12 | Imported types | `ImportedMesh` (`crates/nif/src/import/types/mesh.rs`) derives `Debug, Clone` only; no imported type is serialisable. Any derived kind that serialises imported data SHOULD share one schema with the [glTF and USD Asset Export PRD](asset-export.md)'s AEX-MAT-004 |
| 13 | Bake | none: no bundle format, no bake command, no `bake.*` console command, no bake setting |
| 14 | Interior load threading | the synchronous interior entry point "still drives all phases on the caller's thread" (`CellLoadPhaseTimings` doc, `byroredux/src/cell_loader/load.rs:40`); `load_references` extracts and calls `import_nif_with_collision_and_resolver` inline (`cell_loader/references/import.rs:79`). The pool pipeline (`parse_nif_pipeline`, `streaming/pre_parse.rs:512`) and `prefetch_textures` (sole caller `streaming_helpers.rs:977`, on `WorldStreamingState`) serve exterior streaming only |
| 15 | `ArchiveStamp` persistence | private, in-process only (`#[derive(Clone, Copy, PartialEq, Eq)]`, `archive.rs:358`); it holds a `SystemTime` and is never written to disk |

---

## 4. Goals and non-goals

### 4.1 Goals

- **BAKE-G-001 — Measured first.** A decomposition of asset-load time per
  in-scope game (BAKE-MEAS-001) is checked in before any bundle code
  lands, and the milestone's speed-up target is the one derived from it
  (BAKE-MEAS-002, decision Q8).
- **BAKE-G-002 — Faster where measured.** With a fresh bake, the A7 and A8
  scenes reach the Phase 0 target against the same build with the bake
  off, on the same machine and page-cache state.
- **BAKE-G-003 — Same result.** For every key, the bytes served and the
  source that wins are identical with the bake on and off (A3, A5).
- **BAKE-G-004 — Never required.** Absent, stale, corrupt, refused and off
  bundles all load exactly as HEAD does today (A4, A10).
- **BAKE-G-005 — Bounded disk.** Bundle bytes stay at or under the bake
  budget, and the free-space reserve is never crossed (A6).
- **BAKE-G-006 — Local only.** No bake leaves the machine through any
  engine, launcher or tool path (BAKE-LEGAL-001).

### 4.2 Non-goals

- **BAKE-NG-001.** Replacing the BSA / BA2 readers or the archives
  themselves. The archives stay the source of truth and the bundle is a
  cache of what their readers return (§1 corollary). This is consistent
  with the keep list's "Parse data, don't emulate engine"
  ([ROADMAP § The keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation)),
  which the bake does not touch: the parsers stay the only path from
  archive bytes to engine data.
- **BAKE-NG-002.** Baking plugins or the ESM record index. The parse is
  shared in-session by `SharedParseCache`
  ([Memory Budget § ESM Index](../engine/memory-budget.md#esm-index-cpu-side)),
  and the plugin fold belongs to the Legacy Mod Compatibility PRD.
- **BAKE-NG-003.** Device-specific GPU objects: BLAS, serialised
  acceleration structures, pipeline caches. The pipeline cache already has
  its own file (§3.3 row 6); a bundle stays valid across GPUs and drivers.
- **BAKE-NG-004.** Recompression, mip regeneration and upscaling. Those
  belong to `tools/texture-upscale`, as the texture-streaming PRD already
  decided ([TXS-NG-005](texture-streaming.md#42-non-goals)).
- **BAKE-NG-005.** Distributing a bake, and a mod author shipping a bundle
  with a mod. A mod's own archives are sources like any other and are baked
  on the user's machine (BAKE-INV-003, A10). Decision Q5 records the reason
  and the alternative.
- **BAKE-NG-006.** Defining M53's cluster format. The bundle reserves a
  derived section kind for it (BAKE-DER-004); the Virtual Geometry PRD owns
  the layout.
- **BAKE-NG-007.** Changing asset precedence. Every precedence rule is
  owned by the [Archives](../engine/archives.md#asset-provider-precedence-and-the-loose-tier)
  design and the `LMC-AST` requirements; the bake follows them.
- **BAKE-NG-008.** Baking the canonical tier (§3.1).

---

## 5. Requirements by area

### 5.1 Measurement (`BAKE-MEAS`)

- **BAKE-MEAS-001.** Phase 0 MUST check in, under `docs/audits/`, a table
  per in-scope game that splits asset-load wall time into read, inflate,
  NIF parse, import, texture upload and mesh upload, for one pinned
  interior and one grid-cross, each with a warm and a cold page cache, and
  MUST name the method that made the cache cold (for example "echo 1 >
  /proc/sys/vm/drop_caches", which per the Linux kernel's *Documentation
  for /proc/sys/vm* frees the pagecache,
  <https://docs.kernel.org/admin-guide/sysctl/vm.html>). Today no instrument separates read from inflate (§3.3 row 9:
  `extract_task_sum_ms` sums both) or parse from the rest of the
  `references` phase. The split comes from an offline example binary over
  each game's archives (read, inflate, `parse_nif`, import per entry) plus
  the existing in-engine lines; Phase 0 adds no engine code.
- **BAKE-MEAS-002.** The milestone's speed-up target MUST be stated as the
  ratio the MEAS-001 table supports for the chosen level, bounded above by
  `1 / (1 − bakeable share)`. **Phase 0: verify** the ROADMAP row's "runtime
  loads are 10× faster" against that table: it is an unmeasured claim, and
  even with the removed stages costing nothing it holds only if the chosen
  level's bakeable share is at least nine tenths of load time. The verdict
  (holds / does not hold, with the measured bound) is recorded beside the
  table.
- **BAKE-MEAS-003.** Phase 0 MUST check in a census per in-scope game of
  every vanilla archive: entry count, entries stored compressed versus
  uncompressed, stored bytes and inflated bytes (`declared_size`). It sizes
  the bake budget (BAKE-DISK-001) and settles which entries the inflated
  level helps at all. **Phase 0: verify** how many vanilla FO4 and
  Starfield archives were built uncompressed, and whether any BA2 mixes
  compressed and stored records; Archive2 offers
  `-compression=<None|Default|XBox>` per run (Fallout 4 CK wiki, *Archive2*,
  local dump `falloutck-uesp-wiki/(main)/Archive2.wiki`), and the census
  answers the per-record question. The census MAY share its walk of the
  texture BA2s with the texture-streaming chunk census
  ([TXS A4](texture-streaming.md#72-acceptance), `ba2_mip_chunk_census`).
- **BAKE-MEAS-004.** The MEAS-001 table MUST state the storage class it was
  taken on (NVMe, SATA SSD, HDD). An inflated bundle is larger than its
  source, so on slow storage a cold read can cost more than the inflate it
  removes; the table MUST include one cold row on the slowest storage
  class available to the owner.
- **BAKE-MEAS-005.** Once MEAS-001 lands, the timing rows in
  [Asset Pipeline § End-to-end timing reference](../engine/asset-pipeline.md#end-to-end-timing-reference)
  MUST be replaced by a link to it, so the two designs stop disagreeing.

### 5.2 Bundle format (`BAKE-FMT`)

- **BAKE-FMT-001.** A bundle MUST hold the entries of exactly one source,
  so replacing, adding or removing one archive invalidates one bundle.
- **BAKE-FMT-002.** A bundle header MUST carry a magic, the bundle format
  version, the source path, the `ArchiveStamp` fields BAKE-INV-007 keeps,
  recorded at bake time, and the entry count. A reader MUST treat an
  unknown format version as **stale**.
- **BAKE-FMT-003.** An inflated entry's bytes MUST equal what
  `Archive::extract` returns for that key from that source. Today: no
  bundle exists (§3.3 row 13).
- **BAKE-FMT-004.** Every entry MUST start at an offset aligned to 4096
  bytes and be stored uncompressed, so an entry can be mapped and handed to
  a parser or a staging copy as one slice.
- **BAKE-FMT-005.** A texture entry for a single-layer 2D DDS MUST let any
  mip range `[first, mip_count)` be read as one contiguous byte range plus
  the header. A DDS stores its main image first and each smaller level
  after it (Microsoft, *DDS Texture Example*,
  <https://learn.microsoft.com/en-us/windows/win32/direct3ddds/dds-file-layout-for-textures>),
  so the shipped order already makes every such range a contiguous tail
  once the entry is stored uncompressed; whether reversing the order pays
  is a Phase 2 measurement, not a requirement. Cubemaps are **whole**
  ([Texture Streaming § 3.2](texture-streaming.md#32-texture-classes)), and
  any texture that is not a 2D, single-layer DDS is non-streamable by the
  texture-streaming PRD's definition of a streamable texture
  ([Texture Streaming § 2](texture-streaming.md#2-terminology)); neither
  needs a range rule.
- **BAKE-FMT-006.** An entry the source already stores uncompressed SHOULD
  be left out of the bundle and served from the source (state **absent**),
  since the inflated level removes nothing for it.
- **BAKE-FMT-007.** A published bundle MUST NOT be truncated or written in
  place. A rebake writes a new file through
  `byroredux_core::atomic_file::atomic_write`, which renames it over the old
  name, and an orphaned bundle is removed only by `unlink`. A session that
  already opened the old bundle keeps reading it: when the last name of a
  file is removed while a process holds it open, "the file will remain in
  existence until the last file descriptor referring to it is closed"
  (Linux man-pages, *unlink(2)*,
  <https://man7.org/linux/man-pages/man2/unlink.2.html>). The reader
  therefore MUST hold each bundle's descriptor open for as long as it
  serves from it (BAKE-READ-004).
- **BAKE-FMT-008.** Derived sections MUST carry their own generator version
  and the identities of all their inputs, and MUST be skipped as **stale**
  when either differs, independently of the inflated sections.

### 5.3 Invalidation (`BAKE-INV`)

- **BAKE-INV-001.** A bundle MUST be served only while its recorded stamp
  equals `ArchiveStamp::of` the source at open time, the same comparison
  `Archive::open_shared` makes before reusing a handle (§3.3 row 3).
- **BAKE-INV-002.** A **stale** bundle MUST NOT serve any entry, MUST be
  counted in `bake.status`, and MUST be deleted or replaced by the next
  bake run.
- **BAKE-INV-003.** A change of load order, a plugin enabled or disabled,
  or a reordered mod root MUST NOT invalidate any bundle. Only a source
  added, removed or rewritten changes which bundles are fresh; which source
  wins is resolved at lookup time, as it is today.
- **BAKE-INV-004.** A same-length rewrite that restores the mtime MUST be
  detected as **stale** on Unix (the inode and ctime fields), as
  `open_shared_rereads_a_same_length_rewrite_that_restores_the_mtime`
  (`archive.rs:1247`) already proves for handles. Off Unix the stamp is
  length and mtime only (§3.3 row 3). The bake reuses `ArchiveStamp` and
  has no Windows identity rule of its own: it inherits
  [XPLAT-PORT-007](cross-platform-builds.md#55-platform-neutral-code-xplat-port)'s
  answer. That item records that the pinned 1.96.0 std has no stable
  Windows file identity (`volume_serial_number` / `file_index` are unstable
  under rust-lang/rust#63010, `change_time` under rust-lang/rust#121478),
  names Win32 `FILE_BASIC_INFO.ChangeTime` as the candidate, and closes its
  verify on the Windows lane in XPLAT Phase 2. Until then a Windows bundle
  misses a same-length rewrite that restores the mtime, exactly as the
  handle cache does; BAKE-INV-005's verify mode is the check there.
- **BAKE-INV-005.** The bake command MUST offer a verify mode that re-reads
  every source entry and compares it with the bundle, reporting each
  mismatch by source and key. Decision Q2 settles whether a per-entry hash
  is also checked at lookup time.
- **BAKE-INV-006.** Loose files MUST NOT be baked at the inflated level:
  they are already uncompressed and change without any stamp the bake
  could record per root. A derived section MAY cover a loose input when it
  keys on that input's bytes, as `terrain-height/` does.
- **BAKE-INV-007.** A bundle whose source is unchanged MUST still report
  **fresh** in `bake.status` after a reboot and a remount. **Phase 0:
  verify** which `ArchiveStamp` fields stay equal across both for the
  storage classes BAKE-MEAS-004 records (`st_dev` on removable drives and
  btrfs subvolumes in particular), and which metadata-only operations change
  `ctime` (a mod manager creating a hard link to the source is the expected
  case); settled by the `stat(2)` man page and a test on the owner's
  machine. The verdict sets which fields BAKE-FMT-002 records; BAKE-INV-004
  needs inode and ctime. A field that changes makes a bundle spuriously
  **stale**, never wrong. Today the stamp has only ever been compared within
  one process (§3.3 row 15).

### 5.4 Precedence under mods (`BAKE-PREC`)

- **BAKE-PREC-001.** With the bake on, the winning source for every key
  MUST be the same as with it off. A bundle stands in for its own source at
  that source's position in the provider list and never forms a tier of
  its own. Today the provider resolves last-listed-wins over archives with
  the loose tier last (#3637, #6200). Because a hit returns the bytes the
  source would (BAKE-FMT-003), content-hash rules keyed on those bytes, such
  as asset errata applying only to the vanilla hash
  ([LMC-AST-006](legacy-mod-compatibility.md#53-assets-lmc-ast)), see no
  difference.
- **BAKE-PREC-002.** Loose files MUST outrank baked entries exactly as
  they outrank archives ([LMC-AST-001](legacy-mod-compatibility.md#53-assets-lmc-ast)).
- **BAKE-PREC-003.** Paired archives
  ([LMC-AST-002](legacy-mod-compatibility.md#53-assets-lmc-ast)) and
  mod-root order (LMC-AST-010), when they land, MUST need no bake change:
  no bake code reads the load order or the root order.
- **BAKE-PREC-004.** Entry keys MUST be folded by the provider's own
  normalisation (`canonical_texture_key`, `normalize_mesh_path` in
  `asset_provider/archive.rs`; LMC-AST-005), never by a second rule.
- **BAKE-PREC-005.** Diagnostics that name a winning file (`tex.missing`,
  the shadow counts of `count_shadowed_entries`, LMC-DX explanations) MUST
  name the source archive, never the bundle.

### 5.5 Read path (`BAKE-READ`)

- **BAKE-READ-001.** A bundle MUST be served through the provider's
  `Archive` handle for its source (a new backend beside `Backend::Bsa`,
  `Backend::Ba2` and `Backend::Loose`), so `extract`, `contains` and
  `declared_size` keep their signatures and no consumer changes. The
  renderer MUST NOT gain a `byroredux-bsa` dependency
  ([TXS-SRC-003](texture-streaming.md#52-source-reads-txs-src)).
- **BAKE-READ-002.** Any state other than **hit** MUST fall through to the
  source's reader inside the same call.
- **BAKE-READ-003.** The reader MUST check every header field, offset and
  length against the bundle's file length before reading, MUST bound sizes
  as the archive readers do (`crates/bsa/src/safety.rs`), and on any failure
  MUST mark the bundle **corrupt**, log one warning and fall through. A
  bundle on disk is untrusted input.
- **BAKE-READ-004.** If the reader maps bundles (decision Q9), the engine's
  own writes MUST never shrink a mapped bundle: on Linux `SIGBUS` is raised
  on an "attempted access to a page of the buffer that lies beyond the end
  of the mapped file" (Linux man-pages, *mmap(2)*,
  <https://man7.org/linux/man-pages/man2/mmap.2.html>), and for a
  `MAP_PRIVATE` mapping "it is unspecified whether changes made to the file
  after the mmap() call are visible in the mapped region" (same page).
  BAKE-FMT-007 is what makes this hold for the engine's own writes; the A4
  truncation test covers a bundle that is already short when opened. A
  bundle truncated by another process while mapped is not covered; that
  residual risk is §6 row 7 and the consequence Q9 weighs.
- **BAKE-READ-005.** One setting registered in `SettingsRegistry` MUST turn
  the bake off for a session; with it off, no bundle is opened. It is the
  control for every live scenario in §7.2.
- **BAKE-READ-006.** For a texture entry, the provider MUST serve a
  requested mip range by reading only that range's bytes, through the
  mip-range interface M39 defines
  ([TXS-SRC-001, TXS-SRC-002](texture-streaming.md#52-source-reads-txs-src)).
  A bundle hit is TXS-SRC-002's raw-file case ("A raw file or a loose file
  MAY be read from the floor's byte offset alone"), so a compressed BSA
  texture is no longer inflated twice for a floor and a later promotion,
  the cost [TXS Q2](texture-streaming.md#10-decisions-required)'s
  recommendation accepts. M39 MUST keep working with no bake present, as
  [TXS-NG-002](texture-streaming.md#42-non-goals) states.
- **BAKE-READ-007.** A hit SHOULD cost no inflate and no copy beyond the
  one the consumer's `Vec<u8>` signature requires. Today every consumer
  receives an owned `Vec<u8>` (§3.3 row 1); a borrowed-slice path is a
  Phase 1 design choice, measured by A7.

### 5.6 Bake command (`BAKE-TOOL`)

- **BAKE-TOOL-001.** A bake MUST be producible by one headless command that
  takes the same game profile or archive list the engine boots with
  (`--game <profile>` or explicit `--bsa` lists) and needs no GPU. Decision
  Q3 settles whether runtime write-through also produces bundles.
- **BAKE-TOOL-002.** The command MUST print one line per source: entries
  baked, entries skipped as uncompressed, bytes read, bytes written,
  seconds, and state (written, fresh, refused); and MUST exit non-zero
  when a write fails.
- **BAKE-TOOL-003.** Re-running the command over unchanged sources MUST
  write no file and report every source **fresh**.
- **BAKE-TOOL-004.** The command MUST bound its in-flight inflated bytes,
  in the manner of the stream worker's input budget
  ([Archives § Exterior streaming overlap](../engine/archives.md#exterior-streaming-overlap)),
  so baking a multi-gigabyte archive does not hold it in RAM.
- **BAKE-TOOL-005.** The launcher MAY offer the command as an action; it
  runs as a separate process, as the launcher's design requires
  ([Launcher § 0](../engine/launcher.md#0-the-architectural-decision-a-separate-process)).

### 5.7 Disk budget (`BAKE-DISK`)

- **BAKE-DISK-001.** The total size of all bundles MUST be bounded by one
  bake budget in bytes, a registered setting readable by the headless
  command. Its default is set from the BAKE-MEAS-003 census (decision Q4).
  Today neither on-disk cache has a size bound (§3.3 row 6).
- **BAKE-DISK-002.** When the next bundle would exceed the budget, the bake
  MUST skip that source, record it as **refused** in the manifest with the
  reason, and continue with the next; the session loads it from the source
  and `bake.status` reports it **refused**.
- **BAKE-DISK-003.** The bake MUST NOT write when the filesystem's free
  space after the write would fall under a reserve, a second registered
  setting, and MUST report the reserve as the reason.
- **BAKE-DISK-004.** Bundles whose source no longer exists MUST be removed
  by the next bake run, and `bake.status` MUST report their bytes until
  then.
- **BAKE-DISK-005.** The cache root resolves to the executable's directory
  unless `BYRO_CACHE_DIR` is set (§2), which for a development build is
  inside the cargo target directory (`target/debug/` for `cargo run`).
  There it counts toward the budget `scripts/target-size-guard.sh`
  measures with `du -sk` over the whole target directory (commit
  `588a923aa`, [contributing.md § Build](../contributing.md#build)), and
  `cargo clean` deletes it. The bake MUST print its directory and total size
  at the start of every run. Decision Q4 settles the default location.
- **BAKE-DISK-006.** Deleting the bake directory MUST be a complete and safe
  reset: the next session loads from the sources, as for `terrain-height/`.

### 5.8 Derived sections (`BAKE-DER`)

- **BAKE-DER-001.** A derived section MUST NOT be built for a kind whose
  share in the MEAS-001 table is below the threshold Phase 0 records with
  it.
- **BAKE-DER-002.** If the Starfield CDB index is baked, a session with an
  unchanged CDB MUST skip `MaterialIndex::build` for it and MUST resolve
  every `.mat` lookup the unbaked index resolves to the same result. Today
  it is built per discovered CDB per session (§3.3 row 10).
- **BAKE-DER-003.** A derived section built from imported NIF data MUST be
  produced by the same import call the runtime makes
  (`import_nif_with_collision_and_resolver`), and `translate()` MUST still
  run on load, so NIFAL keeps one translate site per category
  ([NIFAL § 1](../engine/nifal.md#1-the-three-tier-model)). Its key MUST
  include every external input the import resolved (a Starfield
  `BSGeometry`'s `.mesh` payloads, an FO4 material file).
- **BAKE-DER-004.** The bundle format MUST reserve a derived section kind
  for M53 cluster data, and MUST NOT define its layout; the Virtual
  Geometry PRD owns it.

### 5.9 Telemetry (`BAKE-TEL`)

- **BAKE-TEL-001.** A new console command `bake.status` MUST print the
  bake directory, total bundle bytes, the budget and reserve, and per
  source the state (including **refused**, from the manifest) and the
  session's hit, absent, stale and corrupt counts.
- **BAKE-TEL-002.** A per-session bench line (new, `bake:`) MUST carry
  hits, fall-throughs, bytes served from bundles, bytes served from
  sources, and summed read time for each.
- **BAKE-TEL-003.** The stream worker's pipeline line MUST report read and
  inflate as separate sums instead of `extract_task_sum_ms` alone, so the
  live split confirms the offline MEAS-001 table.

### 5.10 Legal (`BAKE-LEGAL`)

- **BAKE-LEGAL-001.** The engine, the launcher and every tool in the
  workspace MUST NOT export, package, upload or transmit a bundle. Bundles
  are written only under the cache root. This follows the posture that the
  engine ships no mod, patch or fix content and the corpus is local
  ([LMC-LEGAL-001](legacy-mod-compatibility.md#513-legal-lmc-legal)), the
  Decentralized Mod Hosting PRD's exclusion of redistributing "anything
  derived from game data"
  ([DMH-NG-006](decentralized-mod-hosting.md#42-non-goals)), and the
  anti-scope's exclusion of hosted online services
  ([ROADMAP § What we are NOT doing](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).
- **BAKE-LEGAL-002.** A bundle whose source archive is absent from the
  machine, or whose stamp does not match, MUST NOT be read
  (BAKE-INV-001). A copied bake is useless without the copy of the game it
  came from.
- **BAKE-LEGAL-003.** A bundle MUST contain nothing but bytes from the
  user's installed sources and engine output derived from them.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Archive entries are stored compressed by archive default with a per-file inversion bit, and inflated on read (UESP, *Oblivion Mod:BSA File Format*, *Skyrim Mod:Archive File Format*) | A fresh bundle serves the inflated bytes (BAKE-FMT-003) | Repacking BSAs uncompressed for load speed, which community guides advise ("Repack BSAs for with no compression. It is faster", UESP, *Oblivion Mod:BSA Repacking*; community advice, unmeasured) | A stale copy served after a mod update → per-source stamp check (BAKE-INV-001, BAKE-INV-002) |
| 2 | FO4 precombined NIFs are an author-time bake; when a NIF "is changed out from under an existing reference, that reference doesn't know it was changed" (Fallout 4 CK wiki, *Precombined References*) | The bake is user-local, keyed on the source's identity, and never replaces authored precombines | — | The original's stale-underneath class cannot recur: a rewritten source makes its bundle **stale** (BAKE-INV-001, BAKE-INV-004); authored precombines stay the plugin layer's (LMC-REC-012) |
| 3 | The original engines spend no disk beyond the install (**Phase 0: verify**; no source found either way) | Bundles take disk | — | Disk exhaustion → budget and reserve (BAKE-DISK-001…003), one-step reset (BAKE-DISK-006) |
| 4 | Engine-internal: every load reads compressed bytes | Inflated bundles are larger than their sources | — | Slower cold loads on slow storage → cold rows per storage class (BAKE-MEAS-004), the off switch (BAKE-READ-005) |
| 5 | Engine-internal: the archive is the only on-disk asset input | A second on-disk input exists | — | A hostile or corrupt bundle → validation and fall-through (BAKE-READ-003); `SIGBUS` on a shrunk mapping → immutable bundles (BAKE-FMT-007, BAKE-READ-004) |
| 6 | Engine-internal: a load-order change costs nothing on disk | Still costs nothing | — | None: bundles are per source (BAKE-INV-003) |
| 7 | Engine-internal: an archive changed under a running session is re-read by the next `open_shared` (#5663) | A mapped bundle truncated by another process mid-session | — | `SIGBUS` ends the process instead of falling through → engine writes never truncate (BAKE-FMT-007), positional reads remove the risk entirely (decision Q9) |
| 8 | Engine-internal: `ArchiveStamp` is compared within one process only (§3.3 row 15) | The stamp is persisted and compared across sessions | — | A field that is not stable across a reboot makes every bundle **stale** → verified field set (BAKE-INV-007) |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

The inflated level consumes `Archive::extract` output, so per-game and
per-archive differences stay in `crates/bsa` and the archive layer, as
they do today; the bundle format has no per-game field. Derived sections
are per data kind (a Starfield CDB, an FO4 CSG), not per game branch. Waves
follow the instruments that already exist (decision Q7); the tiers are in
the [ROADMAP compatibility matrix](../../ROADMAP.md#compatibility-matrix).

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | BSA v103, zlib. Wave 2 |
| Fallout 3 | yes | BSA v104, zlib. Wave 2 |
| Fallout: New Vegas | yes | BSA v104, zlib; the reference title with the interior bench of record. Wave 1 |
| Skyrim (LE / SE / AE) | yes | LE BSA v104 zlib, SE / AE BSA v105 LZ4 frame ([Archives § At a glance](../engine/archives.md#at-a-glance)). Wave 2 |
| Fallout 4 | yes | BA2 GNRL and DX10; grid-cross streaming instruments and the CSG precombine derived candidate. Wave 1 |
| Fallout 76 | no | No cell loader ([Game Compatibility § Per-game support detail](../engine/game-compatibility.md#per-game-support-detail)); its BA2 reads share FO4's code, so the inflated level serves it unchanged once cells render |
| Starfield | yes | BA2 v2 / v3 with LZ4 block; the CDB index derived candidate. Wave 2 |

### 7.2 Acceptance

Game data stays on the machine; no scenario checks in or implies
redistributing any game file or bundle. Every live scenario follows the
[bench methodology](../../ROADMAP.md#how-to-run-a-bench) (pinned
`--camera-pos` / `--camera-forward`, isolated `BYROREDUX_SETTINGS_PATH`,
same-machine control) with the BAKE-READ-005 switch as its control, an
isolated `BYRO_CACHE_DIR`, and the page-cache state recorded. Release
builds need `BYRO_DEBUG_SERVER=1` for `byro-dbg` to attach.

| # | Scenario | Driven by | Data | Passes when |
|---|---|---|---|---|
| A1 | Entry equality | new `cargo test -p byroredux-bsa` test `bundle_entry_bytes_equal_extract` over the crate's synthetic BSA and BA2 fixtures | none | every key's bundle bytes equal `extract` (BAKE-FMT-003) |
| A2 | Staleness | new `cargo test -p byroredux --bin byroredux` test `bundle_is_stale_after_a_same_length_rewrite`, beside `open_shared_rereads_a_same_length_rewrite_that_restores_the_mtime` | temp files | the rewritten source's bundle reports **stale** and serves nothing (BAKE-INV-001, BAKE-INV-004) |
| A3 | Precedence | existing `loose_texture_overrides_the_archived_copy`, `later_listed_bsa_wins_a_mesh_path_collision` and `every_content_provider_resolves_collisions_last_wins`, run with bundles present; new `baked_provider_resolves_every_key_like_the_unbaked_one` | temp files; the corpus tests' FNV / FO4 data | identical winners and bytes for every key (BAKE-PREC-001, -002) |
| A4 | Corrupt and truncated bundles | new test `truncated_bundle_falls_through_to_the_source` (header, table and payload truncations, bad offsets) | temp files | no panic; state **corrupt**; bytes equal the source (BAKE-READ-003) |
| A5 | Corpus equality | new example `crates/bsa/examples/bake_equivalence.rs` over every vanilla archive of the game | each in-scope game's vanilla archives | 100% of entries equal; uncompressed entries reported **absent** |
| A6 | Budget and reserve | new test `bake_refuses_sources_past_the_budget`, and the command run with a budget below one source's size | temp files | refused sources counted; total ≤ budget; exit 0 (BAKE-DISK-001…003) |
| A7 | Interior load | `--bench-hold` on FNV GSProspectorSaloonInterior and FO4 MedTekResearch01, bake on and off, cold and warm; read the `Cell load phases:` line, then `byro-dbg` `bake.status`, `mesh.cache`, `tex.missing` | FNV, FO4 (Wave 2 adds one interior per game) | phase total meets the Phase 0 target; `tex.missing` and entity counts equal the control |
| A8 | Grid-cross | `docs/smoke-tests/m-exteriors.sh fo4 boundary` (existing script) with the bake on and off | FO4 Commonwealth | passes as today; full-detail ready per crossing on the `streaming:` line meets the Phase 0 target |
| A9 | Soak | `docs/smoke-tests/m-exteriors.sh fo4 soak` (existing) with the bake on | FO4 Commonwealth | passes as today; zero **corrupt** in `bake.status` |
| A10 | Mod change | new test `replacing_one_archive_invalidates_only_its_bundle`; live, replace one mod archive between two `--bench-hold` runs and read `bake.status` | temp files; one user-installed mod archive (never checked in) | one source **stale**, every other **fresh**; the new archive's bytes are served (BAKE-INV-002, -003) |
| A11 | Floor read from a bundle | after M39 Phase 4, the texture-streaming A6 floor-first scenario on FNV with the bake on and off | FNV | floor-ready time with the bake ≤ without (BAKE-READ-006) |
| A12 | Freshness across a reboot | bake FNV and FO4, reboot (and remount the data drive where it is removable), then `--bench-hold` and `byro-dbg` `bake.status` | FNV, FO4 on the owner's machine | every unchanged source **fresh** (BAKE-INV-007) |

---

## 8. Metrics

Definitions and granularity only. The numbers go in a new asset-bake row
set under [ROADMAP § How to run a bench](../../ROADMAP.md#how-to-run-a-bench)
with their repro commands in
[ROADMAP § Repro commands](../../ROADMAP.md#repro-commands-for-every-bench-claim);
the Phase 0 decomposition stays in `docs/audits/` and ROADMAP links it.

| Metric | Definition | Granularity |
|---|---|---|
| Stage share | stage wall time ÷ asset-load wall time, for read, inflate, parse, import, texture upload, mesh upload | per game, per scene, warm / cold, per storage class |
| Bakeable share | sum of the shares a bake level removes | per game, per level |
| Load speed-up | control load time ÷ bake-on load time (`Cell load phases:` total; full-detail ready per crossing) | per scene, warm / cold |
| Hit rate | hits ÷ lookups reaching a source with a bundle | per session |
| Bake size | bundle bytes ÷ source stored bytes | per game |
| Bake time | wall time of the bake command over the vanilla archive set | per game, per storage class |
| Invalidation rate | stale bundles ÷ bundles, after a profile change | per change kind (archive replaced, plugin toggled, order changed) |

---

## 9. Delivery sequence

ROADMAP lists M82 as depending on M39 and M53. This PRD's phases depend on
them only where they consume their interfaces: Phase 2 needs M39's
mip-range reads, Phase 5 needs M53's cluster format (decision Q6 asks
whether ROADMAP's dependency column changes to say so). Phase 1 reserves the
cluster section kind (BAKE-DER-004) without defining it.

**Phase 0 — Measure, census and design (no engine code).** Write the
offline decomposition example and run it with the existing in-engine lines
to produce the BAKE-MEAS-001 table for FNV and FO4, then the other in-scope
games; run the BAKE-MEAS-003 census; record the storage classes
(BAKE-MEAS-004). Compute the bakeable share per level and the target
(BAKE-MEAS-002) and record the verdict on the ROADMAP's "10×". Settle the
persisted stamp fields (BAKE-INV-007) and the BA2 per-record compression
question (BAKE-MEAS-003). Write `docs/engine/asset-bake.md` (bundle layout,
manifest, read path, bake command, the crate each lives in) and register
it. Close every **Phase 0: verify** item and resolve Q1–Q9. Exit: the
decomposition and census tables are in
`docs/audits/`, the ROADMAP M82 row cites the measured target instead of
"10×", and every §10 item carries **Decided**.

**Phase 1 — Wave 1 inflated bundles and the read path.** BAKE-FMT-001…008,
BAKE-INV-001…007, BAKE-PREC-001…005, BAKE-READ-001…005 and 007,
the BAKE-DER-004 reservation,
BAKE-TOOL-001…004, BAKE-DISK-001…006, BAKE-TEL-001…003, BAKE-LEGAL-001…003,
for FNV and FO4. Exit: A1–A6, A10 and A12 pass; A5 is 100% on FNV and FO4; A7
and A8 meet the Phase 0 target and their rows are published in ROADMAP.

**Phase 2 — Texture section through M39.** Depends on M39 Phases 2 and 4
(TXS-SRC-001, TXS-SRC-002). BAKE-READ-006; the ordering measurement named
in BAKE-FMT-005. Exit: A11 passes on FNV, and the floor-ready rows with and
without the bake are published beside M39's.

**Phase 3 — Derived sections.** BAKE-DER-001…003, for the kinds the Phase 0
table puts above its threshold (Q1). Exit: for each kind built, A7 or A8
shows the speed-up Phase 0 predicted for it within the control's run-to-run
noise, and DER-002's lookup-equality test passes where the CDB index is
built.

**Phase 4 — Wave 2.** Oblivion, FO3, Skyrim LE / SE and Starfield. Exit: A5
is 100% on each, A7 meets the Phase 0 target on one interior per game, and
the per-game rows are published.

**Phase 5 — Cluster section.** Depends on M53. BAKE-DER-004 filled with the
Virtual Geometry PRD's format. Exit: that PRD's load acceptance passes with
cluster data served from bundles, with the bake-off control alongside.

---

## 10. Decisions required

1. **Q1 — Which bake level ships first.** Recommended: **inflated** first,
   with derived kinds added in Phase 3 only where the Phase 0 table shows
   their share. It keeps the parser the only path from bytes to data,
   invalidates only on a source change, and needs no serialisation of
   imported types (§3.3 row 12). Alternative: **imported** NIF scenes first.
   It removes parse and import too, which the asset-pipeline design names
   as the dominant cost, but every importer change must bump a generator
   version, every imported type needs a stable serialisation, and the key
   must cover every resolved external input (BAKE-DER-003). Alternative:
   **GPU-ready** vertex and index streams. The fastest load, but it couples
   the bundle to the renderer's vertex layout and the shader struct copies
   and bypasses NIFAL's translate step, which BAKE-NG-008 excludes.
2. **Q2 — Lookup-time integrity.** Recommended: trust the stamp at open
   (BAKE-INV-001) and leave content checks to the verify mode
   (BAKE-INV-005). A hit then costs no extra read, and the stamp already
   catches mtime-restored rewrites on Unix; but nothing then notices a
   bundle whose own payload was damaged after it was written, since
   BAKE-READ-003 checks structure, not content. Alternative: store a
   checksum of each entry's inflated bytes in the bundle and verify it on
   every hit. It catches a damaged bundle at the cost of one hash pass over
   the served bytes per hit, and needs no source read. Alternative: also check a
   per-entry hash of the source's stored bytes at every lookup, as
   `terrain-height/` keys on raw source bytes. Always correct, but every hit
   reads the compressed payload again, so only the inflate is saved.
   Alternative: hash the whole source at open. Correct, and costs a full
   read of every archive at every launch.
3. **Q3 — When bundles are produced.** Recommended: the explicit headless
   command (BAKE-TOOL-001), offered by the launcher; the runtime only
   reads. A session never pays for a write, and the ROADMAP's "ships once
   at install" becomes "run once per install". Alternative: runtime
   write-through, writing an entry's bundle the first time it is
   extracted. No separate step, but the first session pays the writes and
   concurrent sessions need the per-write temp names of #6290.
   Alternative: both.
4. **Q4 — Bake location and default budget.** Recommended: `bake/` under
   the cache root, like every other cache (#6274), with a default budget
   from the BAKE-MEAS-003 census and the start-of-run location line
   (BAKE-DISK-005). For a development build that puts the bake under
   `target/`, so it counts toward `scripts/target-size-guard.sh`'s budget
   and `cargo clean` deletes it; a developer who keeps a bake sets
   `BYRO_CACHE_DIR`. Alternative: the XDG cache directory
   (`$XDG_CACHE_HOME`). It keeps a development bake out of the cargo target
   directory, but breaks the one cache-root rule and the `BYRO_CACHE_DIR`
   override every test isolates with. Alternative: no default budget, and
   the bake refuses to run until one is set.
5. **Q5 — Mod-publish bundles.** The ROADMAP row says the bake ships "at
   install / mod-publish". Recommended: drop "mod-publish" from M82
   (BAKE-NG-005). A mod's archives are sources like the game's, so the
   user's own bake already covers them (BAKE-INV-003, A10) and no author
   step is needed. No other PRD takes the step either: the Decentralized
   Mod Hosting PRD (M72) defines publishing as content-addressed releases of
   a package's files (DMH-PUB-001…005), has no bundle format, and excludes
   redistributing "anything derived from game data"
   ([DMH-NG-006](decentralized-mod-hosting.md#42-non-goals)); a bundle of
   a mod's own archive would be publishable there as an ordinary file only
   if a later revision of that PRD adds it. A bake built from game data
   never leaves the machine (BAKE-LEGAL-001). Alternative: accept bundles
   shipped inside a mod root as a new archive kind with the precedence of
   that root's archives. Mods could ship pre-laid-out data, at the cost of
   a precedence rule the Legacy Mod Compatibility PRD has to add
   (`LMC-AST`), a format version that becomes a public contract (trigger
   T2), a DMH revision, and a check that a mod bundle carries no bytes
   derived from the base game. The Skyrim SE Creation Kit EULA permits
   distributing Game Mods only "to users who have purchased the Product"
   and for "non-commercial use" (UESP, *Skyrim Mod:EULA*, local dump
   `uesp-wiki/Skyrim Mod/EULA.wiki`) but says nothing about re-packaged
   game files; **Phase 0: verify** what each title's Creation Kit and
   Archive2 terms say about distributing archives built from game files
   before choosing this alternative.
6. **Q6 — The dependency column.** Recommended: change ROADMAP's M82
   "Depends on" from "M39, M53" to "M39 (Phase 2), M53 (Phase 5)", so
   Phase 1 is not held behind Tier 11. Alternative: keep M82 blocked until
   M39 and M53 ship; nothing is built before both, and the inflated level,
   which needs neither, waits with them.
7. **Q7 — Wave 1.** Recommended: FNV and FO4, because the interior bench of
   record, the grid-cross scripts and the streaming lines already exist for
   both. Alternative: Starfield first, where the CDB index is a derived
   candidate, but with no grid-cross gate (`m-exteriors.sh` skips it by
   policy, #4507).
8. **Q8 — The ROADMAP row.** Recommended: rewrite the M82 row from Phase 0's
   results: "runtime loads are 10× faster" becomes the per-scene target
   Phase 0 derives (BAKE-MEAS-002), published with its storage class;
   "texture-streaming-aware mip ordering" becomes mip-range reads from the
   shipped order (BAKE-FMT-005), unless the Phase 2 measurement shows a
   reversed order pays; "cluster-aware mesh layout for M53" becomes the
   reserved section (BAKE-DER-004). Alternative: keep 10× as the goal and
   accept that the inflated level may not reach it, which makes Phase 1's
   exit unmeetable if the bakeable share is small.
9. **Q9 — Mapping or positional reads.** Recommended: map bundles
   read-only on Linux, with immutable bundles (BAKE-FMT-007) as the guard
   against `SIGBUS` from the engine's own writes. A hit then needs no read
   syscall and, with a borrowed-slice path (BAKE-READ-007), no copy; the
   residual risk is a bundle truncated by another process while mapped,
   which ends the session (§6 row 7). Alternative: positional reads
   (`pread`) from the bundle, as the archive readers do today. No mapping
   risk at all (a short file is a read error that falls through), at the
   cost of one read and one copy per hit. Alternative: decide after Phase 1
   measures both under A7.

---

## 11. Document map

| Need | Go to |
|---|---|
| Archive formats, compression, DX10 chunks, reconstructed DDS headers | [archives.md](../engine/archives.md) |
| Provider precedence and the loose tier | [archives.md § Asset provider precedence](../engine/archives.md#asset-provider-precedence-and-the-loose-tier) |
| Streaming extraction, prefetch and precombine numbers | [archives.md § Exterior streaming overlap](../engine/archives.md#exterior-streaming-overlap), [§ Texture prefetch](../engine/archives.md#texture-prefetch), [§ FO4 precombines on the worker](../engine/archives.md#fo4-precombines-on-the-worker) |
| Cell-load integration, NIF import cache | [asset-pipeline.md](../engine/asset-pipeline.md), [memory-budget.md § NIF Import Cache](../engine/memory-budget.md#nif-import-cache) |
| The on-disk cache precedent | [exal-terrain-height.md § Disk cache](../engine/exal-terrain-height.md#disk-cache) |
| Starfield CDB index cost | [memory-budget.md § Starfield Component Database](../engine/memory-budget.md#starfield-component-database-cpu-side) |
| Imported / canonical tiers | [nifal.md](../engine/nifal.md) |
| Mip-range reads and texture residency (M39) | [texture-streaming.md](texture-streaming.md) |
| Asset precedence under mods, legal posture | [legacy-mod-compatibility.md § Assets](legacy-mod-compatibility.md#53-assets-lmc-ast), [§ Legal](legacy-mod-compatibility.md#513-legal-lmc-legal) |
| Launcher process boundary | [launcher.md](../engine/launcher.md) |
| Per-game engine tiers | [game-compatibility.md](../engine/game-compatibility.md) |
| Smoke scripts and how to run them | [docs/smoke-tests/README.md](../smoke-tests/README.md) |
| Architecture invariants | [CLAUDE.md](../../CLAUDE.md#architecture-invariants) |
| Live milestone state, bench methodology and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: interior-load HEAD claim, timing-doc quotes, mmap/unlink citations, BA2 compression verify, Windows identity inherited from XPLAT-PORT-007 (no stable std API, `FILE_BASIC_INFO.ChangeTime` candidate), persisted stamp (BAKE-INV-007, A12), refused-state manifest, Q5 against DMH, Q6/Q9 split, Q2 checksum alternative, Q4 guard consequence, Q8 row rewrite, TXS-SRC-002 / TXS Q2 cited, bake terminology; coordinator cross-PRD sync (sibling links and citations) | claude: revise |
