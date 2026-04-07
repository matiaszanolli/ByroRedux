# Archives — BSA and BA2

The `byroredux-bsa` crate exposes two readers covering every Bethesda
archive format from Oblivion through Starfield:

- [`BsaArchive`](../../crates/bsa/src/archive.rs) — BSA versions
  103, 104, and 105 (Oblivion → Skyrim SE)
- [`Ba2Archive`](../../crates/bsa/src/ba2.rs) — BA2 (BTDX) versions
  1, 2, 3, 7, and 8 (Fallout 4, Fallout 76, Starfield), with both `GNRL`
  (general files) and `DX10` (texture) variants

Both expose a unified case-insensitive, slash-agnostic API:

```rust
let archive = BsaArchive::open("Fallout - Meshes.bsa")?;
let bytes = archive.extract("meshes/clutter/food/beerbottle01.nif")?;
```

```rust
let archive = Ba2Archive::open("Fallout4 - Meshes.ba2")?;
let bytes = archive.extract("meshes/interiors/desk01.nif")?;
```

Source: [`crates/bsa/src/`](../../crates/bsa/src/)

## At a glance

| Game | Format | Compression | Reader |
|---|---|---|---|
| Oblivion | BSA v103 | zlib | `BsaArchive` |
| Fallout 3 | BSA v104 | zlib | `BsaArchive` |
| Fallout New Vegas | BSA v104 | zlib | `BsaArchive` |
| Skyrim LE | BSA v104 | zlib | `BsaArchive` |
| Skyrim SE | BSA v105 | LZ4 frame | `BsaArchive` |
| Fallout 4 (original) | BA2 BTDX v1 GNRL / v7 DX10 | zlib | `Ba2Archive` |
| Fallout 4 (Next Gen) | BA2 BTDX v8 GNRL / v7 DX10 | zlib | `Ba2Archive` |
| Fallout 76 | BA2 BTDX v1 GNRL | zlib | `Ba2Archive` |
| Starfield | BA2 BTDX v2 GNRL / v3 DX10 | LZ4 / Zstd | `Ba2Archive` (GNRL works; v3 DX10 deferred) |

The integration test that walks all of these against real game data is
in [`crates/nif/tests/parse_real_nifs.rs`](../../crates/nif/tests/parse_real_nifs.rs)
and exercises the unified `MeshArchive` enum from
[`crates/nif/tests/common/mod.rs`](../../crates/nif/tests/common/mod.rs).

## BSA reader

[`crates/bsa/src/archive.rs`](../../crates/bsa/src/archive.rs)

BSA files are organised as:

```
Header (36 bytes)
├── magic "BSA\0"
├── version u32 (103/104/105)
├── archive flags u32
├── folder count
├── file count
├── total folder name length
├── total file name length
└── file flags

Folder records (16 bytes each, or 24 for v105)
├── name hash u64
├── file count u32
└── offset to file record block (u32 v103/104, u64 v105 + padding)

For each folder:
├── folder name (u8 length + null-terminated string)
└── per-file records (16 bytes each)
    ├── name hash u64
    ├── size u32 (with toggle bit at 0x40000000)
    └── offset u32

File name table (null-terminated strings, in folder order)
```

`BsaArchive::open()` reads the header, walks the folder records, and
populates a `HashMap<String, FileEntry>` keyed by `folder\file` (lowercase,
backslash-separated). Extraction seeks to the entry's offset, optionally
skips an embedded file-name prefix (when archive flag `0x100` is set on
v104+), and decompresses if the entry's compression toggle bit XORs with
the archive's `compressed_by_default` flag.

### Compression by version

- **v103/v104**: zlib (`flate2::read::ZlibDecoder`)
- **v105**: LZ4 frame format (`lz4_flex::frame::FrameDecoder`)

The compressed payload is always prefixed with a u32 of the original
uncompressed size, so the decompressor knows how big to size its output
buffer.

### Embedded file names (v104+ flag 0x100)

When archive flag `0x100` is set, every file's data starts with a bstring
(`u8 length + name`) prefix. The reader skips it during `extract()` and
subtracts those bytes from the entry size before reading the rest. The
flag is **off** for the standard BSA layout used by Bethesda's official
archives, but some mods set it.

## BA2 reader

[`crates/bsa/src/ba2.rs`](../../crates/bsa/src/ba2.rs)

BA2 (BTDX) is the post-BSA format introduced with Fallout 4. There are two
record layouts:

- **`GNRL`** — general files (meshes, sounds, animations). One 36-byte
  record per file with a u64 offset and optional zlib compression.
- **`DX10`** — texture archive. One 24-byte base record per DDS plus per-mip
  chunk records. The DDS header is **not** stored — the reader has to
  reconstruct it from the record fields (`dxgi_format`, dimensions,
  mip count) before returning the bytes.

### Header

```
0x00  4 bytes  magic "BTDX"
0x04  4 bytes  version u32
0x08  4 bytes  type tag "GNRL" or "DX10"
0x0C  4 bytes  file count
0x10  8 bytes  name table offset (absolute, u64)
```

For BTDX **v2 and v3** (Starfield) the header has 8 extra bytes after the
basic 24-byte block — likely a u64 compressed-name-table length. The
reader skips them so the stream lines up with the first file record.

```rust
if version == 2 || version == 3 {
    let mut extra = [0u8; 8];
    reader.read_exact(&mut extra)?;
}
```

The version numbering is **non-monotonic** across games:
- v1 = original FO4, FO76 (24-byte header)
- v2/v3 = Starfield (32-byte header with 8-byte extension)
- v7 = FO4 Next Gen textures (back to 24-byte header)
- v8 = FO4 Next Gen meshes (24-byte header)

This bit me during M26: gating the 8-byte extension on `version >= 2`
broke FO4 v8. The check is now `version == 2 || version == 3` exactly.

### GNRL records (36 bytes)

```
0x00  u32  name hash
0x04  4    extension ("nif\0", "wav\0", ...)
0x08  u32  directory hash
0x0C  u32  flags
0x10  u64  offset (absolute, into the same file)
0x18  u32  packed size  (0 = uncompressed, otherwise zlib stream size)
0x1C  u32  unpacked size
0x20  u32  padding (0xBAADF00D)
```

`extract()` seeks to `offset` and either reads `unpacked_size` bytes
directly (when `packed_size == 0`) or reads `packed_size` bytes and
zlib-decompresses to `unpacked_size`.

### Name table

After every file record comes a flat name table at `name_table_offset`:
one entry per file, in record order, formatted as `(u16 length, length
bytes UTF-8)`. The reader normalizes each name to lowercase with
backslash separators on the way in (matching the BSA convention) so the
public lookup API is uniform across both formats.

Names in FO76 use forward slashes, names in FO4 use backslashes; both end
up backslash-keyed after normalization.

### DX10 records (24 bytes base + 24 bytes per chunk)

DDS textures live in a separate archive variant. Each entry is a fixed
header followed by `num_chunks` mip-chain records:

```
Base (24 bytes):
0x00  u32  name hash
0x04  4    extension ("dds\0")
0x08  u32  directory hash
0x0C  u8   unknown (always 0)
0x0D  u8   num chunks
0x0E  u16  chunk header length (always 24)
0x10  u16  height
0x12  u16  width
0x14  u8   num mips
0x15  u8   DXGI format
0x16  u16  flags (bit 0 = cubemap)

Per chunk (24 bytes):
0x00  u64  offset
0x08  u32  packed size
0x0C  u32  unpacked size
0x10  u16  start mip
0x12  u16  end mip
0x14  u32  padding (0xBAADF00D)
```

When the user calls `extract(...)` on a DX10 entry, the reader assembles
the chunks (decompressing each via zlib if `packed_size > 0`) and
**reconstructs a 148-byte DDS+DX10 header in front of them**, since the
DDS file format isn't actually stored in the archive — only the pixel data
plus the metadata needed to recreate the header.

#### DDS header reconstruction

[`build_dds_header()`](../../crates/bsa/src/ba2.rs) writes:

```
DDS magic ("DDS ")
DDS_HEADER (124 bytes)
├── flags        = CAPS | HEIGHT | WIDTH | PIXELFORMAT | LINEARSIZE
│                  ( | MIPMAPCOUNT if num_mips > 1 )
├── height, width
├── pitchOrLinearSize  = computed for known DXGI formats (BC1/3/5/6/7),
│                        falls back to total pixel data length otherwise
├── depth = 0
├── mip count = max(1, num_mips)
├── 11 reserved u32 = 0
├── pixel format
│   ├── size = 32
│   ├── flags = DDPF_FOURCC
│   ├── fourCC = "DX10"   (always — we use the extended path)
│   └── (5 reserved u32 = 0)
├── caps1 = TEXTURE | MIPMAP | COMPLEX
├── caps2 = CUBEMAP_ALLFACES if cubemap
└── (3 more reserved u32 = 0)
DX10 extension (20 bytes)
├── DXGI format
├── resource dimension = TEXTURE2D
├── misc flag = TEXTURECUBE if cubemap
├── array size = 1
└── miscFlags2 = 0
```

The `linear_size_for()` helper computes a reasonable
`dwPitchOrLinearSize` for block-compressed formats from the BC block size
table; unknown formats fall back to the total pixel-data length, which at
least lets a downstream loader size its output buffer.

The reconstructed bytes are valid for downstream readers like our DDS
parser in [`crates/renderer/src/vulkan/dds.rs`](../../crates/renderer/src/vulkan/dds.rs)
or third-party tools like `dds-tools`.

## Unified `MeshArchive` enum

For tests and tooling that need to walk both BSA and BA2 without branching
on format, [`crates/nif/tests/common/mod.rs`](../../crates/nif/tests/common/mod.rs)
defines a thin wrapper:

```rust
pub enum MeshArchive {
    Bsa(BsaArchive),
    Ba2(Ba2Archive),
}

impl MeshArchive {
    pub fn file_count(&self) -> usize { ... }
    pub fn list_files(&self) -> Vec<String> { ... }
    pub fn extract(&self, path: &str) -> io::Result<Vec<u8>> { ... }
}
```

The `Game` enum next to it tracks each supported game's archive kind, env
var override (`BYROREDUX_FNV_DATA`, etc.), default Steam path, and primary
mesh archive filename. `open_mesh_archive(Game)` picks the right reader
and skips cleanly when the data isn't available.

## Tools

### `nif_stats` example

[`crates/nif/examples/nif_stats.rs`](../../crates/nif/examples/nif_stats.rs)
accepts either a single NIF, a directory, or any BSA / BA2 archive and
walks every `.nif` (or `.dds` for DX10 archives) inside it, reporting
parse stats with a block histogram and grouped failure messages.

```bash
cargo run -p byroredux-nif --example nif_stats --release -- \
    "/path/to/Fallout4 - Meshes.ba2"
```

Output:

```
opened /path/to/Fallout4 - Meshes.ba2 (BA2 v8 General, 42426 files)
  → 34995 .nif entries
  progress: 500/34995
  ...

─── Parse stats ──────────────────────────────────────────────
  total:     34995
  ok:        34995  (100.00%)
  failures:      0

─── Block type histogram (top 20) ────────────────────────────
   xxxxx  NiNode
   ...
```

### `ba2_debug` example

[`crates/bsa/examples/ba2_debug.rs`](../../crates/bsa/examples/ba2_debug.rs)
opens a BA2 (or BSA — accepts either by file extension), pulls out the
first three NIFs (or DDSs for DX10 archives), prints a header preview,
and writes the first one to `/tmp/ba2_probe.nif` for offline inspection.
Useful when bringing up new BA2 versions or debugging extraction
mismatches.

```bash
cargo run -p byroredux-bsa --example ba2_debug --release -- \
    "/path/to/SeventySix - Meshes.ba2"
```

### `oblivion_extract` example

[`crates/bsa/examples/oblivion_extract.rs`](../../crates/bsa/examples/oblivion_extract.rs)
pulls one specific file out of an Oblivion BSA and writes it to disk —
used during the M26+ Oblivion follow-up to investigate parser failures
on real Oblivion NetImmerse-era content.

## Known gaps

### Starfield v3 DX10 (deferred)

Starfield's texture archives use BTDX v3 DX10 with a different per-chunk
layout than FO4 v7 — the chunk padding word doesn't match `0xBAADF00D`
and the record fields likely shift. The archive header and directory
parse correctly, but `extract()` for textures returns errors. **NIF / mesh
extraction is unaffected** — the Starfield meshes archive is BTDX v2 GNRL
and parses cleanly.

This is tracked as a follow-up under the M26 deferred list. It only
matters once we want to render Starfield textures, which itself depends
on rendering pipelines that don't exist yet for Starfield's
material/shader stack.

### Starfield LZ4 / Zstd compression

Starfield uses LZ4 block compression (and Zstd in some streams) for some
file payloads. The current reader assumes zlib for compressed entries
inside GNRL archives. Most Starfield mesh entries appear to be raw
(`packed_size == 0`), so this hasn't blocked the 100% Starfield NIF parse
rate, but it would be needed for any compressed entries we encounter.

### BA2 v3 GNRL header differences (theoretical)

BA2 v3 might add fields beyond the v2 8-byte extension. We currently
treat v2 and v3 identically; if Starfield ships an archive that uses v3-only
fields we'll need to dig into the layout.

## Tests

- **8 unit tests** between BSA and BA2 — `normalize_path`, header rejection
  of non-archive files, DDS header reconstruction layout invariants
  (148 bytes, "DDS " magic, "DX10" FourCC, dxgi_format at offset 128),
  `linear_size_for` block size math
- **7 ignored integration tests** in BSA covering FNV mesh open / list /
  contains / extract / decompress, FNV texture BSA decompression
- **Indirectly covered** by every per-game NIF parse-rate sweep in
  `parse_real_nifs.rs` — those are the real correctness oracles

See [Testing](testing.md) for the full test inventory and
[Game Compatibility](game-compatibility.md) for per-game extraction +
parse rate numbers.
