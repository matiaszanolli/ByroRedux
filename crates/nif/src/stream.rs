//! Version-aware binary stream reader for NIF files.
//!
//! NifStream wraps a byte cursor and carries the header context so that
//! version-dependent reads (string format, block references, etc.) are
//! handled in one place rather than scattered through block parsers.

use crate::header::NifHeader;
use crate::types::{BlockRef, NiColor, NiMatrix3, NiPoint3, NiQuatTransform, NiTransform};
use crate::version::{NifVariant, NifVersion};
use std::io::{self, Cursor, Read};
use std::sync::Arc;

// NIF format is little-endian by spec. The bulk-array readers below
// (`read_ni_point3_array`, `read_u16_array`, …) cast a typed `Vec<T>`
// into a `&mut [u8]` and pass that to `read_exact`, so the host's
// endianness has to match the file's. Every supported target
// (x86_64, aarch64) is LE; any future big-endian port will need a
// per-element byte swap in those readers and should remove this
// gate. The per-element `from_le_bytes` paths elsewhere in this
// file are host-agnostic and unaffected. See #833.
#[cfg(target_endian = "big")]
compile_error!("NIF parser requires a little-endian host (bulk-array readers cast Vec<T> to &mut [u8])");

/// Binary reader with NIF header context for version-aware parsing.
pub struct NifStream<'a> {
    cursor: Cursor<&'a [u8]>,
    header: &'a NifHeader,
    variant: NifVariant,
}

/// Hard cap on any single file-driven allocation. A corrupt or malicious
/// NIF can claim an arbitrary 32-bit size in a `ByteArray`, `read_bytes`
/// caller, or `vec![0u8; n]` bulk read; without a cap the parser would
/// allocate gigabytes before `read_exact` fails.
///
/// 256 MB is well above any legitimate single-block payload (the fattest
/// Gamebryo shader-map binary or Havok physics blob we've seen on the
/// seven supported games is ~12 MB on FO76 actor NIFs), and well below
/// host RAM pressure on our 16-GB dev target. See #113 / audit NIF-13.
pub const MAX_SINGLE_ALLOC_BYTES: usize = 256 * 1024 * 1024;

impl<'a> NifStream<'a> {
    pub fn new(data: &'a [u8], header: &'a NifHeader) -> Self {
        let variant =
            NifVariant::detect(header.version, header.user_version, header.user_version_2);
        Self {
            cursor: Cursor::new(data),
            header,
            variant,
        }
    }

    pub fn version(&self) -> NifVersion {
        self.header.version
    }

    pub fn user_version(&self) -> u32 {
        self.header.user_version
    }

    pub fn user_version_2(&self) -> u32 {
        self.header.user_version_2
    }

    /// The detected game variant — use this for feature queries instead of raw version numbers.
    pub fn variant(&self) -> NifVariant {
        self.variant
    }

    /// Actual BSVER from the header (user_version_2).
    /// Use this for fine-grained binary format decisions instead of the variant's
    /// hardcoded bsver(), which represents the "typical" value for that game.
    pub fn bsver(&self) -> u32 {
        self.header.user_version_2
    }

    pub fn position(&self) -> u64 {
        self.cursor.position()
    }

    pub fn set_position(&mut self, pos: u64) {
        self.cursor.set_position(pos);
    }

    /// Advance the cursor by `n` bytes.
    ///
    /// Returns `UnexpectedEof` if the skip would move past the end of
    /// the backing data, or if `pos + n` overflows `u64`. The cursor is
    /// NOT advanced on error, so callers can rely on block_size recovery.
    pub fn skip(&mut self, n: u64) -> io::Result<()> {
        let pos = self.cursor.position();
        let end = pos
            .checked_add(n)
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "skip overflow"))?;
        let len = self.cursor.get_ref().len() as u64;
        if end > len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("skip({n}) at position {pos} would exceed data length {len}"),
            ));
        }
        self.cursor.set_position(end);
        Ok(())
    }

    // ── Primitive reads ────────────────────────────────────────────────

    pub fn read_u8(&mut self) -> io::Result<u8> {
        let mut buf = [0u8; 1];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    pub fn read_u16_le(&mut self) -> io::Result<u16> {
        let mut buf = [0u8; 2];
        self.cursor.read_exact(&mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }

    pub fn read_u32_le(&mut self) -> io::Result<u32> {
        let mut buf = [0u8; 4];
        self.cursor.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    pub fn read_i32_le(&mut self) -> io::Result<i32> {
        let mut buf = [0u8; 4];
        self.cursor.read_exact(&mut buf)?;
        Ok(i32::from_le_bytes(buf))
    }

    pub fn read_u64_le(&mut self) -> io::Result<u64> {
        let mut buf = [0u8; 8];
        self.cursor.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    pub fn read_f32_le(&mut self) -> io::Result<f32> {
        let mut buf = [0u8; 4];
        self.cursor.read_exact(&mut buf)?;
        Ok(f32::from_le_bytes(buf))
    }

    /// Read a NiBool (version-dependent size).
    ///
    /// Per nif.xml's `<basic name="bool">` entry:
    /// > A boolean; 32-bit up to and including 4.0.0.2, 8-bit from 4.1.0.1 on.
    ///
    /// So every game Redux targets (Morrowind 4.0.0.0, Oblivion 20.0.0.5,
    /// FO3/FNV 20.2.0.7, Skyrim+) reads a **single byte**. Only pre-4.1
    /// NetImmerse content uses the 4-byte form.
    ///
    /// A previous version of this function had the comparison inverted
    /// and the test cases documented the wrong behavior; that bug made
    /// `NiTriShape::parse` over-read by 3 bytes on every Oblivion NIF
    /// that had a shader, which in turn made the block walker fail on
    /// every Oblivion static mesh and silently return empty scenes.
    pub fn read_bool(&mut self) -> io::Result<bool> {
        if self.header.version >= NifVersion(0x04010001) {
            // 4.1.0.1+: bool is u8
            Ok(self.read_u8()? != 0)
        } else {
            // Pre-4.1.0.1: bool is u32
            Ok(self.read_u32_le()? != 0)
        }
    }

    /// Read a 1-byte boolean (`bool` type in niftools, NOT `NiBool`).
    /// NiGeometryData and related blocks use 1-byte bools for
    /// has_vertices, has_normals, has_colors, etc. in all versions.
    pub fn read_byte_bool(&mut self) -> io::Result<bool> {
        Ok(self.read_u8()? != 0)
    }

    pub fn read_bytes(&mut self, len: usize) -> io::Result<Vec<u8>> {
        self.check_alloc(len)?;
        let mut buf = vec![0u8; len];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// File-driven pre-allocation for `Vec<T>` of length `count`.
    ///
    /// Bounds `count` against the bytes remaining in the stream — each
    /// on-disk element occupies at least one byte (even an `Option`
    /// reference is a 4-byte BlockRef), so a claimed count larger than
    /// the rest of the file is necessarily corrupt and we reject it
    /// before allocating any capacity.
    ///
    /// Used in place of the raw `Vec::with_capacity(count as usize)`
    /// anywhere `count` is a `u32` / `u16` read straight out of the
    /// stream — otherwise a corrupt NIF can trip a giant allocation
    /// before the subsequent reads discover the truncation.
    ///
    /// The bound is on-disk bytes, **not** `size_of::<T>()`, because
    /// element types like `(f32, String)` carry heap pointers far
    /// larger than their serialized representation; a `size_of`-based
    /// check produces false positives on legitimate small NIFs.
    ///
    /// See #388 / OBL-D5-C1 — every Oblivion content sweep used to
    /// abort the process on a crafted or drifted `NiTextKeyExtraData`.
    ///
    /// `#[must_use]` because the helper exists to *replace* a downstream
    /// `Vec::with_capacity` — calling it just for its bound-check side
    /// effect allocates an empty Vec and immediately drops it. Use
    /// [`Self::check_alloc`] when you only need validation. See #831.
    #[must_use = "allocate_vec returns a sized Vec; bind it or use check_alloc instead"]
    pub fn allocate_vec<T>(&self, count: u32) -> io::Result<Vec<T>> {
        let pos = self.cursor.position() as usize;
        let total = self.cursor.get_ref().len();
        let remaining = total.saturating_sub(pos);
        if (count as usize) > remaining {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "NIF claims {count} elements but only {remaining} bytes remain at position {pos} in {total}-byte stream"
                ),
            ));
        }
        Ok(Vec::with_capacity(count as usize))
    }

    /// Validate a file-driven allocation request before `vec![0u8; n]`.
    ///
    /// Rejects claims that (a) exceed the remaining bytes in the stream
    /// — preventing the parser from allocating gigabytes for a block
    /// that physically can't contain them — and (b) breach the hard
    /// [`MAX_SINGLE_ALLOC_BYTES`] cap. Failure short-circuits BEFORE
    /// the allocation, so a corrupt file can't OOM the process.
    ///
    /// Called by every size-prefixed reader (`read_bytes`,
    /// `read_sized_string`, and the bulk array helpers) that would
    /// otherwise trust an attacker-controlled length. `pub` so that
    /// non-stream call sites (e.g. the header block-size table) can
    /// validate before pre-sizing their own buffers. See #113, #388.
    pub fn check_alloc(&self, bytes: usize) -> io::Result<()> {
        if bytes > MAX_SINGLE_ALLOC_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "NIF requested {bytes}-byte allocation, exceeds hard cap \
                     ({MAX_SINGLE_ALLOC_BYTES})"
                ),
            ));
        }
        let pos = self.cursor.position() as usize;
        let total = self.cursor.get_ref().len();
        let remaining = total.saturating_sub(pos);
        if bytes > remaining {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "NIF requested {bytes}-byte read at position {pos}, \
                     only {remaining} bytes remaining in {total}-byte stream"
                ),
            ));
        }
        Ok(())
    }

    // ── Bulk reads (geometry hot path) ────────────────────────────────
    //
    // Read entire arrays in a single `read_exact` call instead of per-
    // element calls, reducing function call + bounds check overhead from
    // O(N) to O(1) (#291). #833 collapses the previous two-allocation
    // pattern (intermediate `Vec<u8>` byte buffer + final `Vec<T>` typed
    // output via `chunks_exact + map + collect`) into a single allocation
    // by reading directly into a zero-initialized typed `Vec<T>` cast as
    // `&mut [u8]`. `T` must be POD (any byte pattern is a valid value)
    // and have alignment >= 1 — every type we instantiate this for
    // (`u16`, `u32`, `f32`, `[f32; 2]`, `[f32; 4]`, `NiPoint3`) is POD,
    // and the cast direction (typed → bytes) only weakens alignment so
    // the slice fundamentals hold. The compile-error at the top of this
    // module pins the LE-host requirement.

    /// Read `count` POD values directly into a zero-initialized typed
    /// `Vec<T>` via a single `read_exact`, then return the populated
    /// vector. Call site must satisfy: `T` is `Copy + Default` and has
    /// no padding bytes / no validity invariants beyond "any bit pattern
    /// is sound" (true for `u16`, `u32`, `f32`, `[f32; N]`, and
    /// `#[repr(C)]` 3-or-4-float structs). LE host required.
    pub(crate) fn read_pod_vec<T: Copy + Default>(&mut self, count: usize) -> io::Result<Vec<T>> {
        let byte_count = count
            .checked_mul(std::mem::size_of::<T>())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "read_pod_vec: byte count overflow ({count} × {} bytes)",
                        std::mem::size_of::<T>(),
                    ),
                )
            })?;
        self.check_alloc(byte_count)?;
        let mut out: Vec<T> = vec![T::default(); count];
        // SAFETY:
        // - `out.as_mut_ptr()` is non-null and aligned to `align_of::<T>()`,
        //   which is >= 1 (the target alignment for `u8`); casting to
        //   `*mut u8` only weakens alignment requirements, which is sound.
        // - The pointed-to region is exactly `count * size_of::<T>() == byte_count`
        //   bytes (matches `Vec`'s contiguous-storage guarantee).
        // - `read_exact` writes exactly `byte_count` bytes via the slice
        //   and does not read existing contents (Read::read_exact is a
        //   pure writer interface per the trait contract).
        // - `T` is documented to require any-byte-pattern soundness, so
        //   the post-read bytes are valid `T` values. `Vec`'s length is
        //   already `count` from `vec![Default; count]`, so no `set_len`
        //   call is needed.
        // - The `target_endian = "big"` compile-error gate at the top of
        //   the module ensures the on-disk LE bytes match the host's
        //   in-memory layout.
        let byte_slice: &mut [u8] = unsafe {
            std::slice::from_raw_parts_mut(out.as_mut_ptr() as *mut u8, byte_count)
        };
        self.cursor.read_exact(byte_slice)?;
        Ok(out)
    }

    /// Read `count` NiPoint3 values (3×f32 each) in one bulk read.
    pub fn read_ni_point3_array(&mut self, count: usize) -> io::Result<Vec<NiPoint3>> {
        self.read_pod_vec::<NiPoint3>(count)
    }

    /// Read `count` RGBA color values (4×f32 each) in one bulk read.
    pub fn read_ni_color4_array(&mut self, count: usize) -> io::Result<Vec<[f32; 4]>> {
        self.read_pod_vec::<[f32; 4]>(count)
    }

    /// Read `count` generic 2D vectors (2×f32 each) in one bulk read.
    /// Alias for `read_uv_array`; use whichever name reads better at the call site.
    pub fn read_vec2_array(&mut self, count: usize) -> io::Result<Vec<[f32; 2]>> {
        self.read_uv_array(count)
    }

    /// Read `count` UV pairs (2×f32 each) in one bulk read.
    pub fn read_uv_array(&mut self, count: usize) -> io::Result<Vec<[f32; 2]>> {
        self.read_pod_vec::<[f32; 2]>(count)
    }

    /// Read `count` u16 values in one bulk read.
    pub fn read_u16_array(&mut self, count: usize) -> io::Result<Vec<u16>> {
        self.read_pod_vec::<u16>(count)
    }

    /// Read `count` RGBA colour values (4 × u8 each) in one bulk read.
    /// `[u8; 4]` is POD with alignment 1 ≥ 1 and any-bit-pattern
    /// soundness, so it slots into `read_pod_vec` the same way the
    /// `[u16; 3]` / `[f32; 2]` / `[f32; 4]` cases do. Replaces the
    /// `read_u8` × 4 push-loop pattern in BSGeometry color decode
    /// (#873).
    pub fn read_u8_quad_array(&mut self, count: usize) -> io::Result<Vec<[u8; 4]>> {
        self.read_pod_vec::<[u8; 4]>(count)
    }

    /// Read `count` triangles (3×u16 each) in one bulk read.
    /// Saves the `chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect()`
    /// rebuild that the parser used to do after `read_u16_array(count *
    /// 3)` — `[u16; 3]` is POD, alignment 2 ≥ 1, all bit patterns sound,
    /// so the underlying `read_pod_vec` cast is identical to the
    /// existing `[f32; 2]` / `[f32; 4]` / `NiPoint3` cases. #874.
    pub fn read_u16_triple_array(&mut self, count: usize) -> io::Result<Vec<[u16; 3]>> {
        self.read_pod_vec::<[u16; 3]>(count)
    }

    /// Read `count` u32 values in one bulk read.
    pub fn read_u32_array(&mut self, count: usize) -> io::Result<Vec<u32>> {
        self.read_pod_vec::<u32>(count)
    }

    /// Read `count` f32 values in one bulk read.
    pub fn read_f32_array(&mut self, count: usize) -> io::Result<Vec<f32>> {
        self.read_pod_vec::<f32>(count)
    }

    // ── NIF-specific reads ─────────────────────────────────────────────

    /// Read a string. Format depends on version:
    /// - Pre-20.1.0.1: length-prefixed (u32 length + bytes)
    /// - 20.1.0.1+: string table index (u32 → header.strings[index])
    ///
    /// Returns `Arc<str>` so that string-table reads (the common path on
    /// 20.1+ files) are a cheap pointer copy + atomic increment, not a
    /// fresh allocation. The legacy length-prefixed path allocates once.
    ///
    /// NOTE: the threshold `0x14010001` must match the one in
    /// `header.rs` that decides whether to populate `header.strings`.
    /// A mismatch would corrupt reads on 20.1.0.1/20.1.0.2 files.
    /// Read an `NiExtraData.Name` field respecting the nif.xml version
    /// gate. The name was added in v10.0.1.0 (`since="10.0.1.0"`); on
    /// earlier streams the field does not exist on disk and consuming a
    /// phantom string length would misalign every extra-data block.
    ///
    /// All subclass parsers (`BSBound`, `BSFurnitureMarker`,
    /// `BSBehaviorGraphExtraData`, `BSInvMarker`, `BSDecalPlacementVectorExtraData`,
    /// `BSPackedCombinedGeomDataExtra`, `BSConnectPointParents`,
    /// `BSConnectPointChildren`, `BSPackedAdditionalGeometryData`, etc.)
    /// must read the name through this helper so the gate is in one
    /// place. See #329.
    pub fn read_extra_data_name(&mut self) -> io::Result<Option<Arc<str>>> {
        if self.header.version < NifVersion(0x0A000100) {
            return Ok(None);
        }
        self.read_string()
    }

    pub fn read_string(&mut self) -> io::Result<Option<Arc<str>>> {
        if self.header.version >= NifVersion(0x14010001) {
            // String table index — Arc::clone is just a refcount bump.
            let idx = self.read_i32_le()?;
            if idx < 0 {
                Ok(None)
            } else {
                Ok(self.header.strings.get(idx as usize).cloned())
            }
        } else {
            // Length-prefixed inline string (Morrowind / pre-20.1).
            let len = self.read_u32_le()? as usize;
            if len == 0 {
                return Ok(None);
            }
            let bytes = self.read_bytes(len)?;
            let s = String::from_utf8_lossy(&bytes);
            Ok(Some(Arc::from(s.as_ref())))
        }
    }

    /// Read a sized string (always length-prefixed, ignoring version).
    /// Used in headers and certain block fields.
    ///
    /// Tries zero-copy `String::from_utf8` first; falls back to lossy
    /// replacement only when the bytes contain invalid UTF-8. Avoids
    /// the unconditional copy from `from_utf8_lossy().into_owned()` on
    /// the hot path (NIF strings are almost always valid ASCII). #254.
    pub fn read_sized_string(&mut self) -> io::Result<String> {
        let len = self.read_u32_le()? as usize;
        let bytes = self.read_bytes(len)?;
        match String::from_utf8(bytes) {
            Ok(s) => Ok(s),
            Err(e) => Ok(String::from_utf8_lossy(e.as_bytes()).into_owned()),
        }
    }

    /// Read a short string (u8 length prefix + bytes).
    ///
    /// Same zero-copy-first strategy as `read_sized_string`. #254.
    pub fn read_short_string(&mut self) -> io::Result<String> {
        let len = self.read_u8()? as usize;
        let mut bytes = self.read_bytes(len)?;
        // Short strings include a null terminator — pop it before conversion.
        if bytes.last() == Some(&0) {
            bytes.pop();
        }
        match String::from_utf8(bytes) {
            Ok(s) => Ok(s),
            Err(e) => Ok(String::from_utf8_lossy(e.as_bytes()).into_owned()),
        }
    }

    /// Read a block reference (i32, where -1 = null).
    pub fn read_block_ref(&mut self) -> io::Result<BlockRef> {
        let val = self.read_i32_le()?;
        if val < 0 {
            Ok(BlockRef::NULL)
        } else {
            Ok(BlockRef(val as u32))
        }
    }

    /// Read an array of block references.
    ///
    /// Counts go through `allocate_vec` so a corrupt `0xFFFFFFFF` count
    /// can't OOM the process before the inner reads fail. See #764.
    pub fn read_block_ref_list(&mut self) -> io::Result<Vec<BlockRef>> {
        let count = self.read_u32_le()?;
        let mut refs = self.allocate_vec(count)?;
        for _ in 0..count {
            refs.push(self.read_block_ref()?);
        }
        Ok(refs)
    }

    // ── Math type reads ────────────────────────────────────────────────

    pub fn read_ni_point3(&mut self) -> io::Result<NiPoint3> {
        Ok(NiPoint3 {
            x: self.read_f32_le()?,
            y: self.read_f32_le()?,
            z: self.read_f32_le()?,
        })
    }

    pub fn read_ni_color(&mut self) -> io::Result<NiColor> {
        Ok(NiColor {
            r: self.read_f32_le()?,
            g: self.read_f32_le()?,
            b: self.read_f32_le()?,
        })
    }

    pub fn read_ni_matrix3(&mut self) -> io::Result<NiMatrix3> {
        let mut rows = [[0.0f32; 3]; 3];
        for row in &mut rows {
            for val in row.iter_mut() {
                *val = self.read_f32_le()?;
            }
        }
        Ok(NiMatrix3 { rows })
    }

    /// Read an NiQuatTransform: translation (3 floats), rotation (4 floats: w,x,y,z), scale (1 float).
    pub fn read_ni_quat_transform(&mut self) -> io::Result<NiQuatTransform> {
        let translation = self.read_ni_point3()?;
        let w = self.read_f32_le()?;
        let x = self.read_f32_le()?;
        let y = self.read_f32_le()?;
        let z = self.read_f32_le()?;
        let scale = self.read_f32_le()?;
        Ok(NiQuatTransform {
            translation,
            rotation: [w, x, y, z],
            scale,
        })
    }

    pub fn read_ni_transform(&mut self) -> io::Result<NiTransform> {
        // NiAVObject inline-transform field order per nif.xml's
        // `NiAVObject` definition: Translation → Rotation → Scale.
        //
        // ⚠️ This is DIFFERENT from the `NiTransform` STRUCT spec
        // (`<struct name="NiTransform" size="52">` at nif.xml line 1808),
        // which orders Rotation → Translation → Scale. The NiTransform
        // STRUCT is used as a sub-record inside
        // `NiSkinData::skin_transform` (global per-skin) and
        // `NiSkinData::bones[i].skin_transform` (per-bone bind). For
        // those, call [`Self::read_ni_transform_struct`] instead.
        //
        // M41.0 Phase 1b.x — pinpointed via the live debug-protocol
        // probe at byroredux/tests/skinning_e2e.rs. The two layouts
        // share the same Rust `NiTransform` type but the byte order on
        // disk is opposite, and reading NiSkinData per-bone fields
        // through this NiAVObject-ordered helper produces a transform
        // whose `translation` is actually the source rotation's first
        // row and whose `rotation` is the remaining 6 source-rotation
        // values + the source translation cells, scrambled. The
        // resulting bind-inverse misskins every legacy NiSkinData
        // body NIF as a horizontal ribbon (visible since M29 #178
        // shipped without rendered skinned content).
        let translation = self.read_ni_point3()?;
        let rotation = self.read_ni_matrix3()?;
        let scale = self.read_f32_le()?;
        // Sanitize once at parse time so downstream code can treat the
        // rotation as a valid rotation matrix. See #277.
        let rotation = crate::rotation::sanitize_rotation(rotation);
        Ok(NiTransform {
            rotation,
            translation,
            scale,
        })
    }

    /// Read a `NiTransform` struct in nif.xml's documented field order:
    /// **Rotation → Translation → Scale**. Used as a sub-record inside
    /// blocks like `NiSkinData::skin_transform` (global) and
    /// `NiSkinData::bones[i].skin_transform` (per-bone bind-inverse).
    ///
    /// ⚠️ Call this — NOT [`Self::read_ni_transform`] — anywhere
    /// nif.xml writes `type="NiTransform"`. The latter is for
    /// NiAVObject's inline transform fields which use a different
    /// (Translation-first) order. Mixing them up scrambles the matrix
    /// (translation column reads as rotation row 0; rotation column
    /// reads as the next two rotation rows + translation row).
    pub fn read_ni_transform_struct(&mut self) -> io::Result<NiTransform> {
        let rotation = self.read_ni_matrix3()?;
        let translation = self.read_ni_point3()?;
        let scale = self.read_f32_le()?;
        let rotation = crate::rotation::sanitize_rotation(rotation);
        Ok(NiTransform {
            rotation,
            translation,
            scale,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal NifHeader for testing stream reads.
    fn test_header(version: NifVersion) -> NifHeader {
        NifHeader {
            version,
            little_endian: true,
            user_version: 0,
            user_version_2: 0,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from("hello"), Arc::from("world")],
            max_string_length: 5,
            num_groups: 0,
        }
    }

    #[test]
    fn read_primitives() {
        let header = test_header(NifVersion::V20_2_0_7);
        let data: Vec<u8> = vec![
            0x42, // u8: 0x42
            0x34, 0x12, // u16le: 0x1234
            0x78, 0x56, 0x34, 0x12, // u32le: 0x12345678
            0x00, 0x00, 0x80, 0x3F, // f32le: 1.0
        ];
        let mut stream = NifStream::new(&data, &header);

        assert_eq!(stream.read_u8().unwrap(), 0x42);
        assert_eq!(stream.read_u16_le().unwrap(), 0x1234);
        assert_eq!(stream.read_u32_le().unwrap(), 0x12345678);
        assert_eq!(stream.read_f32_le().unwrap(), 1.0);
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn read_block_ref_valid_and_null() {
        let header = test_header(NifVersion::V20_2_0_7);
        let data: Vec<u8> = vec![
            0x05, 0x00, 0x00, 0x00, // i32: 5 (valid ref)
            0xFF, 0xFF, 0xFF, 0xFF, // i32: -1 (null ref)
        ];
        let mut stream = NifStream::new(&data, &header);

        let r1 = stream.read_block_ref().unwrap();
        assert_eq!(r1.index(), Some(5));

        let r2 = stream.read_block_ref().unwrap();
        assert!(r2.is_null());
    }

    #[test]
    fn read_block_ref_list() {
        let header = test_header(NifVersion::V20_2_0_7);
        let data: Vec<u8> = vec![
            0x02, 0x00, 0x00, 0x00, // count: 2
            0x00, 0x00, 0x00, 0x00, // ref 0
            0x03, 0x00, 0x00, 0x00, // ref 3
        ];
        let mut stream = NifStream::new(&data, &header);

        let refs = stream.read_block_ref_list().unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].index(), Some(0));
        assert_eq!(refs[1].index(), Some(3));
    }

    /// Corrupt count must error before allocating capacity. See #764.
    #[test]
    fn read_block_ref_list_corrupt_count_errors_before_alloc() {
        let header = test_header(NifVersion::V20_2_0_7);
        let data: Vec<u8> = vec![
            0xFF, 0xFF, 0xFF, 0xFF, // count: 0xFFFFFFFF (~4 GB request)
            0x00, 0x00, 0x00, 0x00, // a single ref worth of payload
        ];
        let mut stream = NifStream::new(&data, &header);

        let err = stream
            .read_block_ref_list()
            .expect_err("0xFFFFFFFF count must reject before pre-allocating");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn read_string_from_table() {
        // Version >= 20.1.0.1 reads from string table
        let header = test_header(NifVersion::V20_2_0_7);
        let data: Vec<u8> = vec![
            0x01, 0x00, 0x00, 0x00, // string table index 1
            0xFF, 0xFF, 0xFF, 0xFF, // index -1 (null)
        ];
        let mut stream = NifStream::new(&data, &header);

        assert_eq!(stream.read_string().unwrap().as_deref(), Some("world"));
        assert_eq!(stream.read_string().unwrap(), None);
    }

    #[test]
    fn read_string_table_boundary_at_20_1_0_1() {
        // Regression for #172: string-table dispatch must kick in at
        // exactly 20.1.0.1 per nif.xml, not 20.1.0.3 as it used to.
        // At 20.1.0.1 the reader should take the string-table path.
        let header = test_header(NifVersion(0x14010001));
        let data: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x00, // string table index 0
        ];
        let mut stream = NifStream::new(&data, &header);
        // If the threshold is still 0x14010003 this would fall through
        // to the inline path and try to read 0 bytes → return None.
        // With the corrected threshold we read index 0 → "hello".
        assert_eq!(stream.read_string().unwrap().as_deref(), Some("hello"));
    }

    #[test]
    fn read_string_inline_below_20_1_0_1() {
        // Just below the threshold: 20.1.0.0 must still use inline strings.
        let header = test_header(NifVersion(0x14010000));
        let data: Vec<u8> = vec![
            0x03, 0x00, 0x00, 0x00, // length: 3
            b'f', b'o', b'o', //
        ];
        let mut stream = NifStream::new(&data, &header);
        assert_eq!(stream.read_string().unwrap().as_deref(), Some("foo"));
    }

    #[test]
    fn read_string_inline_old_version() {
        // Version < 20.1.0.3 reads length-prefixed inline
        let header = test_header(NifVersion(0x0A000100)); // 10.0.1.0
        let data: Vec<u8> = vec![
            0x04, 0x00, 0x00, 0x00, // length: 4
            b't', b'e', b's', b't', // "test"
            0x00, 0x00, 0x00, 0x00, // length: 0 (null string)
        ];
        let mut stream = NifStream::new(&data, &header);

        assert_eq!(stream.read_string().unwrap().as_deref(), Some("test"));
        assert_eq!(stream.read_string().unwrap(), None);
    }

    #[test]
    fn read_ni_point3() {
        let header = test_header(NifVersion::V20_2_0_7);
        let data: Vec<u8> = [
            1.0f32.to_le_bytes(),
            2.0f32.to_le_bytes(),
            3.0f32.to_le_bytes(),
        ]
        .concat();
        let mut stream = NifStream::new(&data, &header);

        let p = stream.read_ni_point3().unwrap();
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);
        assert_eq!(p.z, 3.0);
    }

    #[test]
    fn read_bool_version_dependent() {
        // Per nif.xml, type `bool` is 8-bit from 4.1.0.1 onward and
        // 32-bit for older content.

        // v20.2.0.7 (FO3/FNV/Skyrim+): bool is u8
        let header_new = test_header(NifVersion::V20_2_0_7);
        let data_new: Vec<u8> = vec![0x01];
        let mut stream = NifStream::new(&data_new, &header_new);
        assert!(stream.read_bool().unwrap());
        assert_eq!(stream.position(), 1);

        // v20.0.0.5 (Oblivion): bool is u8 (>= 4.1.0.1)
        let header_oblivion = test_header(NifVersion::V20_0_0_5);
        let data_oblivion: Vec<u8> = vec![0x01];
        let mut stream = NifStream::new(&data_oblivion, &header_oblivion);
        assert!(stream.read_bool().unwrap());
        assert_eq!(stream.position(), 1);

        // v4.0.0.2 (pre-NetImmerse 4.1): bool is u32
        let header_old = test_header(NifVersion::V4_0_0_2);
        let data_old: Vec<u8> = vec![0x01, 0x00, 0x00, 0x00];
        let mut stream = NifStream::new(&data_old, &header_old);
        assert!(stream.read_bool().unwrap());
        assert_eq!(stream.position(), 4);
    }

    #[test]
    fn skip_advances_position() {
        let header = test_header(NifVersion::V20_2_0_7);
        let data = vec![0u8; 100];
        let mut stream = NifStream::new(&data, &header);

        stream.skip(50);
        assert_eq!(stream.position(), 50);
        stream.skip(25);
        assert_eq!(stream.position(), 75);
    }

    #[test]
    fn read_ni_transform_translation_before_rotation() {
        // Regression: NiTransform serialization order is translation, rotation, scale
        // (matches Gamebryo 2.3 NiAVObject::LoadBinary). A previous bug read rotation first.
        let header = test_header(NifVersion::V20_2_0_7);
        let mut data = Vec::new();
        // Translation: (10.0, 20.0, 30.0)
        data.extend_from_slice(&10.0f32.to_le_bytes());
        data.extend_from_slice(&20.0f32.to_le_bytes());
        data.extend_from_slice(&30.0f32.to_le_bytes());
        // Rotation: identity
        for v in &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0f32] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // Scale: 2.5
        data.extend_from_slice(&2.5f32.to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let t = stream.read_ni_transform().unwrap();

        assert_eq!(t.translation.x, 10.0);
        assert_eq!(t.translation.y, 20.0);
        assert_eq!(t.translation.z, 30.0);
        assert_eq!(t.rotation.rows[0], [1.0, 0.0, 0.0]);
        assert_eq!(t.rotation.rows[1], [0.0, 1.0, 0.0]);
        assert_eq!(t.rotation.rows[2], [0.0, 0.0, 1.0]);
        assert_eq!(t.scale, 2.5);
        // 3 + 9 + 1 = 13 floats = 52 bytes
        assert_eq!(stream.position(), 52);
    }

    #[test]
    fn read_ni_transform_non_identity_rotation() {
        // Regression: ensure a non-trivial rotation doesn't get mixed up with translation.
        let header = test_header(NifVersion::V20_2_0_7);
        let mut data = Vec::new();
        // Translation: (0, 0, 0)
        for _ in 0..3 {
            data.extend_from_slice(&0.0f32.to_le_bytes());
        }
        // Rotation: 90° around Z (in row-major): [[0,-1,0],[1,0,0],[0,0,1]]
        for v in &[0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0f32] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // Scale: 1.0
        data.extend_from_slice(&1.0f32.to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let t = stream.read_ni_transform().unwrap();

        assert_eq!(t.translation.x, 0.0);
        assert_eq!(t.rotation.rows[0], [0.0, -1.0, 0.0]);
        assert_eq!(t.rotation.rows[1], [1.0, 0.0, 0.0]);
        assert_eq!(t.scale, 1.0);
    }

    #[test]
    fn skip_within_bounds_succeeds() {
        let data = [0u8; 16];
        let header = test_header(NifVersion::V20_2_0_7);
        let mut stream = NifStream::new(&data, &header);
        assert!(stream.skip(8).is_ok());
        assert_eq!(stream.position(), 8);
        assert!(stream.skip(8).is_ok());
        assert_eq!(stream.position(), 16);
    }

    #[test]
    fn skip_past_end_returns_error() {
        let data = [0u8; 10];
        let header = test_header(NifVersion::V20_2_0_7);
        let mut stream = NifStream::new(&data, &header);
        let err = stream.skip(11).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        // Cursor must NOT have advanced on error.
        assert_eq!(stream.position(), 0);
    }

    #[test]
    fn skip_overflow_returns_error() {
        let data = [0u8; 10];
        let header = test_header(NifVersion::V20_2_0_7);
        let mut stream = NifStream::new(&data, &header);
        stream.skip(5).unwrap();
        let err = stream.skip(u64::MAX).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        assert_eq!(stream.position(), 5);
    }

    /// Regression: #113 / audit NIF-13 — `read_bytes` with a size larger
    /// than what remains in the stream must fail before allocating, and
    /// fail specifically with `UnexpectedEof` so block-size recovery can
    /// swallow the error.
    #[test]
    fn read_bytes_oversized_request_errors_before_alloc() {
        let data = [0u8; 64];
        let header = test_header(NifVersion::V20_2_0_7);
        let mut stream = NifStream::new(&data, &header);
        let err = stream.read_bytes(1_000_000).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        // Cursor untouched on the early check.
        assert_eq!(stream.position(), 0);
    }

    /// Regression: #113 / audit NIF-13 — requests over the hard cap
    /// fail with `InvalidData` regardless of how much data the stream
    /// actually has. Guards against a corrupt file that claims e.g. a
    /// 1 GB ByteArray.
    #[test]
    fn read_bytes_over_hard_cap_errors_regardless_of_stream_size() {
        // Backing buffer larger than the cap — pretend we mmapped a
        // huge file — so the only remaining safeguard is the cap.
        let data = vec![0u8; MAX_SINGLE_ALLOC_BYTES + 1];
        let header = test_header(NifVersion::V20_2_0_7);
        let mut stream = NifStream::new(&data, &header);
        let err = stream.read_bytes(MAX_SINGLE_ALLOC_BYTES + 1).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(stream.position(), 0);
    }

    /// Regression for #502: each bulk-read helper must produce the
    /// exact same sequence as a per-element loop. The bulk path uses
    /// `chunks_exact` + `from_le_bytes`, so this also implicitly checks
    /// the little-endian decode and that no residual bytes leak in on
    /// hosts where alignment would otherwise matter.
    #[test]
    fn bulk_reads_match_per_element_loops() {
        let header = test_header(NifVersion::V20_2_0_7);

        // u16 array
        let u16_data: Vec<u8> = (0u16..64).flat_map(|v| v.to_le_bytes()).collect();
        let mut s_bulk = NifStream::new(&u16_data, &header);
        let bulk_u16 = s_bulk.read_u16_array(64).unwrap();
        let mut s_loop = NifStream::new(&u16_data, &header);
        let mut loop_u16 = Vec::with_capacity(64);
        for _ in 0..64 {
            loop_u16.push(s_loop.read_u16_le().unwrap());
        }
        assert_eq!(bulk_u16, loop_u16);
        assert_eq!(s_bulk.position(), s_loop.position());

        // u32 array
        let u32_data: Vec<u8> = (0u32..32).flat_map(|v| v.to_le_bytes()).collect();
        let mut s_bulk = NifStream::new(&u32_data, &header);
        let bulk_u32 = s_bulk.read_u32_array(32).unwrap();
        let mut s_loop = NifStream::new(&u32_data, &header);
        let mut loop_u32 = Vec::with_capacity(32);
        for _ in 0..32 {
            loop_u32.push(s_loop.read_u32_le().unwrap());
        }
        assert_eq!(bulk_u32, loop_u32);

        // f32 array (includes negative, subnormal, inf edge cases)
        let floats = [
            0.0f32,
            1.0,
            -1.5,
            f32::MIN_POSITIVE,
            f32::INFINITY,
            -0.0,
            3.1415927,
        ];
        let f32_data: Vec<u8> = floats.iter().flat_map(|v| v.to_le_bytes()).collect();
        let mut s_bulk = NifStream::new(&f32_data, &header);
        let bulk_f32 = s_bulk.read_f32_array(floats.len()).unwrap();
        assert_eq!(bulk_f32.len(), floats.len());
        for (a, b) in bulk_f32.iter().zip(floats.iter()) {
            assert_eq!(a.to_bits(), b.to_bits());
        }

        // vec2 / uv array equivalence
        let uvs: [[f32; 2]; 3] = [[0.25, 0.75], [1.0, 0.0], [-0.5, 2.5]];
        let uv_data: Vec<u8> = uvs
            .iter()
            .flat_map(|uv| uv[0].to_le_bytes().into_iter().chain(uv[1].to_le_bytes()))
            .collect();
        let mut s_uv = NifStream::new(&uv_data, &header);
        let bulk_uv = s_uv.read_uv_array(uvs.len()).unwrap();
        let mut s_vec2 = NifStream::new(&uv_data, &header);
        let bulk_vec2 = s_vec2.read_vec2_array(uvs.len()).unwrap();
        assert_eq!(bulk_uv, bulk_vec2);
        assert_eq!(bulk_uv.len(), uvs.len());
        for (a, b) in bulk_uv.iter().zip(uvs.iter()) {
            assert_eq!(a[0].to_bits(), b[0].to_bits());
            assert_eq!(a[1].to_bits(), b[1].to_bits());
        }

        // NiPoint3 array vs per-element read_ni_point3
        let points: [(f32, f32, f32); 2] = [(1.0, 2.0, 3.0), (-4.0, 5.5, -6.25)];
        let p_data: Vec<u8> = points
            .iter()
            .flat_map(|(x, y, z)| {
                x.to_le_bytes()
                    .into_iter()
                    .chain(y.to_le_bytes())
                    .chain(z.to_le_bytes())
            })
            .collect();
        let mut s_bulk = NifStream::new(&p_data, &header);
        let bulk_p = s_bulk.read_ni_point3_array(points.len()).unwrap();
        let mut s_loop = NifStream::new(&p_data, &header);
        let loop_p: Vec<_> = (0..points.len())
            .map(|_| s_loop.read_ni_point3().unwrap())
            .collect();
        assert_eq!(bulk_p.len(), loop_p.len());
        for (a, b) in bulk_p.iter().zip(loop_p.iter()) {
            assert_eq!(a.x.to_bits(), b.x.to_bits());
            assert_eq!(a.y.to_bits(), b.y.to_bits());
            assert_eq!(a.z.to_bits(), b.z.to_bits());
        }
    }

    /// Zero-count bulk reads must succeed without touching the stream.
    #[test]
    fn bulk_reads_handle_zero_count() {
        let header = test_header(NifVersion::V20_2_0_7);
        let data: Vec<u8> = Vec::new();
        let mut s = NifStream::new(&data, &header);
        assert!(s.read_u16_array(0).unwrap().is_empty());
        assert!(s.read_u32_array(0).unwrap().is_empty());
        assert!(s.read_f32_array(0).unwrap().is_empty());
        assert!(s.read_uv_array(0).unwrap().is_empty());
        assert!(s.read_vec2_array(0).unwrap().is_empty());
        assert!(s.read_ni_point3_array(0).unwrap().is_empty());
        assert!(s.read_ni_color4_array(0).unwrap().is_empty());
        assert_eq!(s.position(), 0);
    }

    /// Bulk reads that exceed remaining stream bytes must fail via the
    /// `check_alloc` gate before any allocation happens.
    #[test]
    fn bulk_reads_reject_oversized_count() {
        let header = test_header(NifVersion::V20_2_0_7);
        let data = [0u8; 8];
        let mut s = NifStream::new(&data, &header);
        // 100 u32 = 400 bytes, only 8 available.
        let err = s.read_u32_array(100).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        assert_eq!(s.position(), 0);
    }

    /// Legitimate use at the exact cap must succeed — the cap is
    /// inclusive at the limit.
    #[test]
    fn read_bytes_at_cap_succeeds() {
        let cap = 16; // miniature "cap" for test speed
        let data = vec![0u8; cap];
        let header = test_header(NifVersion::V20_2_0_7);
        let mut stream = NifStream::new(&data, &header);
        let out = stream.read_bytes(cap).unwrap();
        assert_eq!(out.len(), cap);
        assert_eq!(stream.position() as usize, cap);
    }

    /// Regression for #833: the bulk-array readers must produce the same
    /// values they did before the `read_pod_vec` rewrite — i.e. each
    /// element decoded from the on-disk LE bytes via `from_le_bytes`-equivalent
    /// semantics. Pins the byte-order contract so a future port to a BE
    /// host (which would currently fail at the compile-error gate at the
    /// top of this module) can't silently flip endianness, and so a
    /// future migration to bytemuck / a different POD-cast path can't
    /// silently break existing call sites either.
    #[test]
    fn bulk_readers_decode_le_byte_order() {
        let header = test_header(NifVersion::V20_2_0_7);

        // u16: 0x1234, 0xCAFE
        {
            let data: Vec<u8> = vec![0x34, 0x12, 0xFE, 0xCA];
            let mut s = NifStream::new(&data, &header);
            let v = s.read_u16_array(2).unwrap();
            assert_eq!(v, vec![0x1234, 0xCAFE]);
            assert_eq!(s.position() as usize, data.len());
        }

        // u32: 0xDEADBEEF, 0x01020304
        {
            let data: Vec<u8> = vec![0xEF, 0xBE, 0xAD, 0xDE, 0x04, 0x03, 0x02, 0x01];
            let mut s = NifStream::new(&data, &header);
            let v = s.read_u32_array(2).unwrap();
            assert_eq!(v, vec![0xDEAD_BEEF, 0x0102_0304]);
        }

        // f32: 1.0, -2.0
        {
            let data: Vec<u8> = {
                let mut d = Vec::new();
                d.extend_from_slice(&1.0f32.to_le_bytes());
                d.extend_from_slice(&(-2.0f32).to_le_bytes());
                d
            };
            let mut s = NifStream::new(&data, &header);
            let v = s.read_f32_array(2).unwrap();
            assert_eq!(v, vec![1.0, -2.0]);
        }

        // NiPoint3: { x: 1.0, y: 2.0, z: 3.0 }
        {
            let data: Vec<u8> = {
                let mut d = Vec::new();
                for f in &[1.0f32, 2.0, 3.0] {
                    d.extend_from_slice(&f.to_le_bytes());
                }
                d
            };
            let mut s = NifStream::new(&data, &header);
            let v = s.read_ni_point3_array(1).unwrap();
            assert_eq!(v.len(), 1);
            assert_eq!(v[0].x, 1.0);
            assert_eq!(v[0].y, 2.0);
            assert_eq!(v[0].z, 3.0);
        }

        // [f32; 2] (UV): (0.25, 0.75)
        {
            let data: Vec<u8> = {
                let mut d = Vec::new();
                d.extend_from_slice(&0.25f32.to_le_bytes());
                d.extend_from_slice(&0.75f32.to_le_bytes());
                d
            };
            let mut s = NifStream::new(&data, &header);
            let v = s.read_uv_array(1).unwrap();
            assert_eq!(v, vec![[0.25f32, 0.75]]);
        }

        // [f32; 4] (color): (0.1, 0.2, 0.3, 1.0)
        {
            let data: Vec<u8> = {
                let mut d = Vec::new();
                for f in &[0.1f32, 0.2, 0.3, 1.0] {
                    d.extend_from_slice(&f.to_le_bytes());
                }
                d
            };
            let mut s = NifStream::new(&data, &header);
            let v = s.read_ni_color4_array(1).unwrap();
            assert_eq!(v.len(), 1);
            assert!((v[0][0] - 0.1).abs() < 1e-6);
            assert!((v[0][1] - 0.2).abs() < 1e-6);
            assert!((v[0][2] - 0.3).abs() < 1e-6);
            assert_eq!(v[0][3], 1.0);
        }

        // count == 0 must succeed and consume nothing
        {
            let data: Vec<u8> = vec![];
            let mut s = NifStream::new(&data, &header);
            assert_eq!(s.read_u16_array(0).unwrap(), Vec::<u16>::new());
            assert_eq!(s.position(), 0);
        }
    }
}
