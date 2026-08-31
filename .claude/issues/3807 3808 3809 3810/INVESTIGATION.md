# Investigation: #3809 (Havok BhkSystemBinary) + #3810 (.uvd) byte-level cracks

Corpus: `Fallout4 - MeshesExtra.ba2` at
`/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/`. All findings
below were derived by extracting real files with the existing
`byroredux-bsa` archive reader and inspecting raw bytes — no Havok SDK
source (leaked or otherwise) was consulted; the user explicitly declined
that path when offered it, in favor of pure corpus-derived analysis per
this project's no-guessing policy.

## #3809 — `BhkSystemBinary` / Havok classic packfile

### Method
`crates/nif/examples/_tmp_a0831_havok_blob.rs` samples `_physics.nif`
files spread across the 4,484-file corpus (stride sampling, not just the
first N — alphabetical order clusters by cell-formid prefix, not size),
parses each with the existing NIF parser, and dumps the raw
`BhkSystemBinary.data` blob (hex + ASCII-run scan + a `--sections` mode
that hex-dumps the header region). 30 samples spanning 2,608 B to
1,629,568 B were inspected.

### Findings, in the order derived

1. **Magic**: bytes `0..8` are `57 E0 E0 57 10 C0 C0 10` in all 30
   samples — the classic Havok packfile magic pair
   (`0x57e0e057, 0x10c0c010`). This magic and the general packfile
   container shape are long-standing public knowledge from independent
   community reverse-engineering of Havok's `.hkx` format (used across
   many "hkxcmd"/`.hkx`-import tools for Skyrim/FO4 modding); it was
   *recognized* as a hypothesis and then verified byte-for-byte against
   this project's own real corpus, not asserted from memory alone.

2. **Header** (bytes `8..0x40`, fixed for this Havok build):
   - `8..12`: `user_tag` (u32) = 0 in every sample.
   - `12..16`: `file_version` (u32) = 11 in every sample.
   - `16..20`: `layout_rules` (4 raw bytes: `08 01 00 01`) — publicly
     documented as `[bytesInPointer, littleEndian,
     reusePaddingOptimization, emptyBaseClassOptimization]` in community
     packfile-reader docs; not independently re-derived here beyond
     capturing the raw bytes.
   - `20..24`: `num_sections` (u32) = 3 in every sample — matches the
     3-entry section table actually present.
   - `24..28`: unconfirmed (u32) = 2 in every sample. Kept as
     `reserved_after_num_sections`, not named.
   - `28..36`: zero padding in every sample.
   - `36..40`: unconfirmed (u32) = `0x4b` (75) in every sample — doesn't
     match any in-header byte offset, kept as
     `reserved_before_version_string`.
   - `40..~55`: null-terminated ASCII `contents_version` =
     `"hk_2014.1.0-r1"` in every sample.
   - `60..64`: unconfirmed (u32) = `0x15` (21) in every sample. Kept as
     `reserved_after_version_string`.

3. **Section table** (bytes `0x40..0x40+3*64`, one 64-byte entry per
   section): derived by comparing three real samples' raw hex side by
   side and solving for a layout that's internally self-consistent
   across all three (different file sizes → different numeric values,
   same field positions):
   - Each entry: 19-byte null-padded ASCII name, then a `0xFF`
     terminator byte at a **fixed relative offset 19** regardless of the
     name's actual length (verified for `"__classnames__"` (14 chars),
     `"__types__"` (9 chars), `"__data__"` (8 chars) — all three land
     the `0xFF` at the same relative offset), then 7×`u32`:
     `absolute_data_start, local_fixups_offset, global_fixups_offset,
     virtual_fixups_offset, exports_offset, imports_offset, end_offset`
     (all but `absolute_data_start` relative to it), then 16 bytes of
     `0xFFFFFFFF` padding — 64 bytes total.
   - Cross-checks that confirmed this layout rather than assumed it:
     - `__classnames__`'s `local_fixups_offset` (relative) lands exactly
       on `__types__`'s `absolute_data_start`.
     - `__types__`'s `end_offset` is `0` in every sample (its data
       region is empty — see finding 5) and its `absolute_data_start`
       equals `__data__`'s `absolute_data_start`, i.e. it consumes zero
       bytes.
     - `__data__`'s `absolute_data_start + end_offset` equals the
       blob's total length (`BhkSystemBinary.data.len()`) in every one
       of the 30 samples, independent of size.
   - This table decode is now implemented as
     `crates/nif/src/blocks/collision/havok_packfile.rs::parse_havok_packfile`,
     validated against real data via the `--pf` example flag (matches
     for all 30 sampled files) and pinned by 5 unit tests against a
     hand-built synthetic fixture.

4. **`__classnames__` content is a repeating `[5-byte prefix][NUL-terminated
   name]` record**, not a flat NUL-separated string list as first
   assumed. Confirmed by offset arithmetic: `"hkClass"`'s `h` lands
   exactly 5 bytes after its record boundary, and the next record's
   5-byte prefix starts exactly `5 + name.len() + 1` bytes later — this
   held across all 9 class-name entries in the sample and reproduced
   identically class-for-class across all 30 sampled files. The 5-byte
   prefix's own semantic (likely a class-signature hash, given it looks
   high-entropy) is unconfirmed and not decoded. Class names found, in
   order, in every sample: `hkClass`, `hkClassMember`, `hkClassEnum`,
   `hkClassEnumItem`, `hknpPhysicsSystemData`, `hknpCompressedMeshShape`,
   `hkRefCountedProperties`, `hknpBSMaterialProperties`,
   `hknpCompressedMeshShapeData`.

5. **`__types__` is empty (zero-length) in every sampled blob.** This
   means the packfile carries **no embedded reflection metadata** for
   its objects — the loading application is expected to already know
   the binary layout of every referenced class (all of which are
   standard Havok SDK / `hknp` runtime types, resolved purely by name).
   This closes off the hope that the format might be self-describing:
   there is no free schema to mine from the file. The real remaining
   blocker is specifically **`hknpCompressedMeshShapeData`'s internal
   bit-packed encoding**, which is Havok's own proprietary compression
   and genuinely requires either external documentation or painstaking
   clean-room bit-level inference (e.g. cross-referencing against the
   sibling `_oc.nif`'s already-decoded triangle data to reverse a
   quantization scheme) — real further research, out of this session's
   scope.

6. `__data__`'s content (from `absolute_data_start` onward) is
   high-entropy binary starting almost immediately after the class-name
   list ends — consistent with an already-compressed/bit-packed object
   stream, not a plain serialized struct.

## #3810 — `.uvd` previs/occlusion header

### Method
`crates/bsa/examples/_tmp_a0831_uvd_header.rs` samples `.uvd` files
spread across the 966-file corpus, hex-dumps bytes `0..0xb0` (with
float/int reinterpretation) and the `0xb0..0x100` debug-string region,
and (with `--table`) the region starting at the newly-found constant
offset `0x150`. 30 samples spanning 3,472 B to ~2.4 MB were inspected.

### Findings

Bytes `0..0x14` and `0xb0..0x100` reproduce the 2026-08-23 crack
unchanged (magic `0xD6000012`, self-reported total size at `8..12`,
`f32` tile size `512.0` at `12..16`, and the generator-tool debug string
`"T 512.0 SO 128.0 SH 16.000 BF 100 F 0 CS 0.0 - 3.3.17 F 1 0 OG 0"`,
byte-identical across the whole corpus).

New findings from this pass, past `0x14` (previously "not decoded"):

1. **`content_hash`** (bytes `4..8`, previously flagged as "candidate:
   content hash/checksum, or a per-cell coordinate, not conclusive"): 30
   fresh samples all show uniformly high-entropy 32-bit values with no
   small-integer or coordinate-scale structure (e.g. `0xff2c9628`,
   `0xe4ae7b04`, `0xa46b206b`, ...). This is much more consistent with a
   hash/checksum than a coordinate, though still not proven.

2. **`bounds`** (bytes `0x14..0x28`, 5×`f32`): values are multiples of
   512/4096 in every sample — the same scale as known FO4 exterior
   world-space coordinates. In most (not all) samples, `bounds[0]` and
   `bounds[3]` differ by exactly `12288` (`3×4096`), suggestive of a
   3-cell-wide previs cluster tile, but several samples show smaller,
   irregular diffs (`3584`, `1536`, `2560`, ...) — likely
   boundary-of-worldspace or non-standard-shaped clusters, or possibly
   `bounds[0]`/`bounds[3]` aren't actually a matched min/max pair.
   **Not confirmed** — would need cross-referencing against real parsed
   CELL bounds from the ESM to settle definitively; left for follow-up.

3. **`table_offset`** (byte `0x30`, new field, not previously
   identified): **exactly `336` (`0x150`) in all 30 sampled files**,
   independent of total file size (3,472 B to 2,431,616 B in the
   sample). A file-size-invariant constant this consistent is almost
   certainly a fixed header length / first-variable-table start offset,
   not per-cell content.

4. **`entry_count`** (byte `0x38`, new field, not previously
   identified): scales with file complexity across the corpus (`1, 1,
   9, 11, ..., 305`) — small for tiny files, large for big ones. Very
   likely an object/visibility-entry count.

5. Bytes `0x150` onward (at `table_offset`) were inspected manually for
   one large sample: high-entropy binary through roughly `+0x30`,
   followed by a short run of monotonically increasing single bytes
   (`0f 1b 28 33 3b 41 49 51 57 5d 5f`, i.e. `15, 27, 40, 51, 59, 65, 73,
   81, 87, 93, 95` — an index/offset-table shape), then a second block
   of clean floats resembling a per-object bounding-box table,
   terminated by an `FLT_MAX` sentinel (`ff ff 7f 7f` =
   `0x7f7fffff`). This confirms the real payload is itself a
   compressed/bit-packed stream — a research problem of comparable
   shape to #3809's Havok mesh-shape blocker, not a simple flat struct.
   Not decoded further this session.

New decoder: `crates/bsa/src/uvd.rs::parse_uvd_header` — implements
findings 1-4 above (the confirmed/best-effort envelope fields), pinned
by 3 unit tests against a hand-built synthetic fixture. The payload past
`table_offset` is deliberately left unparsed.
