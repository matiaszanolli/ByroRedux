//! NiSourceTexture — texture file reference.
//! NiPixelData — embedded pixel data (used by some Oblivion NIFs).
//! NiTextureEffect — projected texture effect (env map, gobo, fog).

use super::base::{NiAVObjectData, NiObjectNETData};
use super::traits::{HasAVObject, HasObjectNET};
use super::NiObject;
use crate::stream::NifStream;
use crate::types::{BlockRef, NiMatrix3, NiTransform};
use crate::version::NifVersion;
use std::any::Any;
use std::io;
use std::sync::Arc;

/// Reference to an external texture file or embedded pixel data.
#[derive(Debug)]
pub struct NiSourceTexture {
    pub net: NiObjectNETData,
    pub use_external: bool,
    pub filename: Option<Arc<str>>,
    pub pixel_data_ref: BlockRef,
    pub pixel_layout: u32,
    pub use_mipmaps: u32,
    pub alpha_format: u32,
    pub is_static: bool,
}

impl NiObject for NiSourceTexture {
    fn block_type_name(&self) -> &'static str {
        "NiSourceTexture"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiSourceTexture {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;

        let use_external = stream.read_u8()? != 0;
        let use_string_table = stream.version() >= crate::version::NifVersion::V20_2_0_7;

        // nif.xml line 5117: `Use Internal: byte cond="Use External == 0"
        // until="10.0.1.3"`. On the legacy embedded path Bethesda emits
        // an extra byte distinguishing "embedded NiPixelData" (1) from
        // "neither external nor internal" (0). At 10.0.1.4+ the byte is
        // gone and Pixel Data is always present when Use External == 0.
        // Pre-#715 the byte was unread on pre-Oblivion content with
        // embedded textures, so every block on that path under-read by
        // 1 byte and any following blocks drifted (block_size recovery
        // masked the symptom). See NIF-D1-02 / nif.xml lines 5117/5121.
        // NiSourceTexture Use Internal: nif.xml `until="10.0.1.3"`
        // inclusive per the version.rs doctrine — present at v <=
        // 10.0.1.3. Post-10.0.1.3 content with embedded textures relies
        // on the `Use External == 0` cond alone.
        let use_internal =
            if !use_external && stream.version() <= crate::version::NifVersion(0x0A000103) {
                stream.read_u8()? != 0
            } else {
                true
            };

        let (filename, pixel_data_ref) = if use_external {
            let fname: Option<Arc<str>> = if use_string_table {
                stream.read_string()?
            } else {
                Some(Arc::from(stream.read_sized_string()?))
            };
            if stream.version() >= crate::version::NifVersion(0x0A010000) {
                let _unknown_ref = stream.read_block_ref()?;
            }
            (fname, BlockRef::NULL)
        } else {
            if stream.version() >= crate::version::NifVersion(0x0A010000) {
                if use_string_table {
                    let _unknown = stream.read_string()?;
                } else {
                    let _unknown = stream.read_sized_string()?;
                }
            }
            // nif.xml line 5121: Pixel Data is gated on
            // `Use Internal == 1` for v <= 10.0.1.3. On v >= 10.0.1.4
            // (line 5122) it's unconditional. `use_internal` is `true`
            // on the modern path, so the gate collapses to "always
            // read the ref" — the dual nif.xml lines model a single
            // observable behaviour above 10.0.1.3.
            let pix_ref = if use_internal {
                stream.read_block_ref()?
            } else {
                BlockRef::NULL
            };
            (None, pix_ref)
        };

        let pixel_layout = stream.read_u32_le()?;
        let use_mipmaps = stream.read_u32_le()?;
        let alpha_format = stream.read_u32_le()?;
        // is_static only present in v >= 5.0.0.1 (not in Morrowind-era NIFs).
        let is_static = if stream.version() >= NifVersion(0x05000001) {
            stream.read_u8()? != 0
        } else {
            true
        };

        // nif.xml: Direct Render since 10.1.0.103 (0x0A010067), NOT 10.1.0.6.
        if stream.version() >= crate::version::NifVersion(0x0A010067) {
            let _direct_render = stream.read_byte_bool()?;
        }

        if stream.version() >= crate::version::NifVersion::V20_2_0_7 {
            let _persist_render_data = stream.read_byte_bool()?;
        }

        Ok(Self {
            net,
            use_external,
            filename,
            pixel_data_ref,
            pixel_layout,
            use_mipmaps,
            alpha_format,
            is_static,
        })
    }
}

// ── NiPixelData ────────────────────────────────────────────────────

/// Pixel format channel descriptor (4 per NiPixelFormat).
#[derive(Debug, Clone)]
pub struct PixelFormatComponent {
    pub component_type: u32,
    pub convention: u32,
    pub bits_per_channel: u8,
    pub is_signed: bool,
}

/// Mipmap level descriptor.
#[derive(Debug, Clone)]
pub struct MipMapInfo {
    pub width: u32,
    pub height: u32,
    pub offset: u32,
}

/// Embedded pixel data block — inlines texture pixels directly in the NIF.
///
/// Uncommon but occurs in some Oblivion NIFs where textures are baked in.
/// The pixel format fields (NiPixelFormat) are read inline at the start,
/// followed by mipmap descriptors and the raw pixel bytes.
#[derive(Debug)]
pub struct NiPixelData {
    /// Pixel format enum (0=RGB, 1=RGBA, etc.)
    pub pixel_format: u32,
    pub bits_per_pixel: u8,
    pub renderer_hint: u32,
    pub extra_data: u32,
    pub flags: u8,
    pub tiling: u32,
    pub channels: [PixelFormatComponent; 4],
    /// Reference to NiPalette (usually -1/NULL).
    pub palette_ref: BlockRef,
    pub num_mipmaps: u32,
    pub bytes_per_pixel: u32,
    pub mipmaps: Vec<MipMapInfo>,
    pub num_faces: u32,
    /// Raw pixel data (all mipmaps, all faces, contiguous).
    pub pixel_data: Vec<u8>,
}

impl NiObject for NiPixelData {
    fn block_type_name(&self) -> &'static str {
        "NiPixelData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiPixelData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiPixelFormat fields (inline, not inherited).
        let pixel_format = stream.read_u32_le()?;

        // Version split at 10.4.0.2 — Oblivion/FO3+ use the "new" layout.
        let old_layout = stream.version() < NifVersion(0x0A040002);

        if old_layout {
            // Pre-10.4.0.2: color masks, old bits per pixel, fast compare, tiling.
            let _red_mask = stream.read_u32_le()?;
            let _green_mask = stream.read_u32_le()?;
            let _blue_mask = stream.read_u32_le()?;
            let _alpha_mask = stream.read_u32_le()?;
            let bits_per_pixel_u32 = stream.read_u32_le()?;
            let _fast_compare = stream.read_bytes(8)?;

            let tiling = if stream.version() >= NifVersion(0x0A010000) {
                stream.read_u32_le()?
            } else {
                0
            };

            // Old layout NiPixelData fields
            let palette_ref = stream.read_block_ref()?;
            let num_mipmaps = stream.read_u32_le()?;
            let bytes_per_pixel = stream.read_u32_le()?;
            let mut mipmaps: Vec<MipMapInfo> = stream.allocate_vec(num_mipmaps)?;
            for _ in 0..num_mipmaps {
                let width = stream.read_u32_le()?;
                let height = stream.read_u32_le()?;
                let offset = stream.read_u32_le()?;
                mipmaps.push(MipMapInfo {
                    width,
                    height,
                    offset,
                });
            }
            let num_pixels = stream.read_u32_le()? as usize;
            let pixel_data = stream.read_bytes(num_pixels)?;

            let default_channel = PixelFormatComponent {
                component_type: 0,
                convention: 0,
                bits_per_channel: 0,
                is_signed: false,
            };

            return Ok(Self {
                pixel_format,
                bits_per_pixel: bits_per_pixel_u32 as u8,
                renderer_hint: 0,
                extra_data: 0,
                flags: 0,
                tiling,
                channels: [
                    default_channel.clone(),
                    default_channel.clone(),
                    default_channel.clone(),
                    default_channel,
                ],
                palette_ref,
                num_mipmaps,
                bytes_per_pixel,
                mipmaps,
                num_faces: 1,
                pixel_data,
            });
        }

        // New layout (10.4.0.2+, covers Oblivion and FO3+).
        let bits_per_pixel = stream.read_u8()?;
        let renderer_hint = stream.read_u32_le()?;
        let extra_data = stream.read_u32_le()?;
        let flags = stream.read_u8()?;
        let tiling = stream.read_u32_le()?;

        // sRGB Space — only since 20.3.0.4 (NOT Oblivion, NOT FO3).
        if stream.version() >= NifVersion(0x14030004) {
            let _srgb = stream.read_byte_bool()?;
        }

        // 4 pixel format channels.
        let mut channels = Vec::with_capacity(4);
        for _ in 0..4 {
            let component_type = stream.read_u32_le()?;
            let convention = stream.read_u32_le()?;
            let bits_per_channel = stream.read_u8()?;
            let is_signed = stream.read_byte_bool()?;
            channels.push(PixelFormatComponent {
                component_type,
                convention,
                bits_per_channel,
                is_signed,
            });
        }
        let channels_arr = [
            channels[0].clone(),
            channels[1].clone(),
            channels[2].clone(),
            channels[3].clone(),
        ];

        // NiPixelData fields.
        let palette_ref = stream.read_block_ref()?;
        let num_mipmaps = stream.read_u32_le()?;
        let bytes_per_pixel = stream.read_u32_le()?;

        let mut mipmaps: Vec<MipMapInfo> = stream.allocate_vec(num_mipmaps)?;
        for _ in 0..num_mipmaps {
            let width = stream.read_u32_le()?;
            let height = stream.read_u32_le()?;
            let offset = stream.read_u32_le()?;
            mipmaps.push(MipMapInfo {
                width,
                height,
                offset,
            });
        }

        let num_pixels = stream.read_u32_le()? as usize;
        let num_faces = stream.read_u32_le()?;
        let total_bytes = num_pixels * num_faces as usize;
        let pixel_data = stream.read_bytes(total_bytes)?;

        Ok(Self {
            pixel_format,
            bits_per_pixel,
            renderer_hint,
            extra_data,
            flags,
            tiling,
            channels: channels_arr,
            palette_ref,
            num_mipmaps,
            bytes_per_pixel,
            mipmaps,
            num_faces,
            pixel_data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::NifHeader;
    use crate::stream::NifStream;

    fn make_oblivion_header() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_0_0_5,
            little_endian: true,
            user_version: 0,
            user_version_2: 0,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        }
    }

    /// Build a Morrowind-era (10.0.1.0) NIF header so the parser
    /// hits the legacy embedded-pixel branch (`version <= 10.0.1.3`).
    fn make_pre_oblivion_header(version: NifVersion) -> NifHeader {
        NifHeader {
            version,
            little_endian: true,
            user_version: 0,
            user_version_2: 0,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        }
    }

    /// Build the wire bytes for a `NiSourceTexture` block, optionally
    /// emitting the legacy `use_internal` byte (gated on
    /// `version <= 10.0.1.3 && !use_external` per nif.xml line 5117).
    /// Returns the assembled byte vec the parser is expected to consume
    /// in its entirety.
    fn build_legacy_embedded_source_texture(use_internal: bool, with_pixel_ref: bool) -> Vec<u8> {
        let mut data = Vec::new();
        // NiObjectNET (pre-10.0.1.0): no name string at all on the
        // ancient layout, but our parser reads NiObjectNETData::parse
        // which itself version-gates the name. For 10.0.1.0 the name
        // is read inline as a sized string — author an empty string
        // (u32 length=0).
        data.extend_from_slice(&0u32.to_le_bytes()); // name length = 0
                                                     // extra_data_refs and controller_ref are read for v >= 10.0.1.0
                                                     // — author empty list + null ref.
        data.extend_from_slice(&0u32.to_le_bytes()); // extra_data count = 0
        data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                        // use_external = 0 → embedded path.
        data.push(0u8);
        // use_internal byte (legacy gate).
        data.push(if use_internal { 1u8 } else { 0u8 });
        // pix_ref only present when use_internal == 1 per nif.xml 5121.
        if with_pixel_ref {
            data.extend_from_slice(&7i32.to_le_bytes()); // arbitrary block index
        }
        // Format Prefs (3× u32) + is_static + later booleans.
        data.extend_from_slice(&1u32.to_le_bytes()); // pixel_layout
        data.extend_from_slice(&0u32.to_le_bytes()); // use_mipmaps
        data.extend_from_slice(&0u32.to_le_bytes()); // alpha_format
                                                     // is_static — present when v >= 5.0.0.1; 10.0.1.0 satisfies.
        data.push(1u8);
        data
    }

    /// #715 / NIF-D1-02 — pre-Oblivion embedded path (`until="10.0.1.3"`
    /// inclusive per the version.rs doctrine — present at v <= 10.0.1.3)
    /// must consume the `Use Internal` byte after `Use External == 0`
    /// and then read the Pixel Data ref when `Use Internal == 1`.
    /// Pre-fix the byte was unread, so every block on this path
    /// under-read by 1 byte and the parser drifted into the next
    /// field (read what should be the pixel-ref low byte as the
    /// pixel_layout u32's first byte, etc.).
    #[test]
    fn pre_oblivion_embedded_path_consumes_use_internal_byte_and_pixel_ref() {
        // v10.0.1.2 sits just below the v10.0.1.3 boundary; field present.
        let header = make_pre_oblivion_header(NifVersion(0x0A000102));
        let data = build_legacy_embedded_source_texture(/* use_internal = */ true, true);
        let mut stream = NifStream::new(&data, &header);
        let tex = NiSourceTexture::parse(&mut stream).unwrap();

        assert!(!tex.use_external, "embedded path");
        assert!(
            tex.filename.is_none(),
            "no filename on legacy embedded path"
        );
        assert_eq!(
            tex.pixel_data_ref.index(),
            Some(7),
            "pixel-data ref must read after the use_internal byte (off-by-one symptom pre-#715)"
        );
        assert_eq!(tex.pixel_layout, 1, "format prefs must follow the ref");
        assert_eq!(
            stream.position() as usize,
            data.len(),
            "parser must consume the block exactly — no drift"
        );
    }

    /// nif.xml line 5121 gates Pixel Data on `Use External == 0 #AND#
    /// Use Internal == 1`. When `Use Internal == 0` the ref is absent
    /// — the texture is "neither external nor internal pixel data"
    /// (procedural / runtime-generated). Our parser must not read a
    /// phantom 4-byte ref in this case.
    #[test]
    fn pre_oblivion_embedded_path_skips_pixel_ref_when_use_internal_is_zero() {
        // v10.0.1.2: inside the v10.0.1.3 `until=` boundary (inclusive)
        // so the `Use Internal` byte IS serialized.
        let header = make_pre_oblivion_header(NifVersion(0x0A000102));
        let data = build_legacy_embedded_source_texture(/* use_internal = */ false, false);
        let mut stream = NifStream::new(&data, &header);
        let tex = NiSourceTexture::parse(&mut stream).unwrap();

        assert!(!tex.use_external);
        assert_eq!(
            tex.pixel_data_ref,
            BlockRef::NULL,
            "use_internal = 0 → no Pixel Data ref written / read"
        );
        assert_eq!(stream.position() as usize, data.len());
    }

    /// Boundary regression for #935 (post-#765/#769 doctrine flip).
    /// nif.xml gates `Use Internal` with `until="10.0.1.3"` which is
    /// **inclusive** per niftools/nifly (see version.rs doctrine).
    /// The byte IS read at v10.0.1.3 exactly. Pre-#935 the parser
    /// used `<` and skipped the byte at this version, drifting every
    /// subsequent field by 1 byte on legacy embedded textures.
    #[test]
    fn pre_oblivion_embedded_path_consumes_use_internal_at_v10_0_1_3_exactly() {
        let header = make_pre_oblivion_header(NifVersion(0x0A000103));
        let data = build_legacy_embedded_source_texture(/* use_internal = */ true, true);
        let mut stream = NifStream::new(&data, &header);
        let tex = NiSourceTexture::parse(&mut stream).unwrap();

        assert!(!tex.use_external, "embedded path");
        assert!(
            tex.filename.is_none(),
            "no filename on legacy embedded path"
        );
        assert_eq!(
            tex.pixel_data_ref.index(),
            Some(7),
            "pixel-data ref must read after the use_internal byte at v10.0.1.3 (until= is inclusive)"
        );
        assert_eq!(tex.pixel_layout, 1);
        assert_eq!(
            stream.position() as usize,
            data.len(),
            "parser must consume the block exactly — boundary must include v10.0.1.3"
        );
    }

    /// Modern path (v >= 10.0.1.4) — the legacy `Use Internal` byte is
    /// absent. Sanity-check that bumping the version one notch above
    /// the inclusive gate triggers the modern flow (no `use_internal`
    /// consumed, pixel ref read unconditionally on the embedded branch).
    #[test]
    fn post_10_0_1_4_embedded_path_skips_use_internal_byte() {
        let header = make_pre_oblivion_header(NifVersion(0x0A000104));
        let mut data = Vec::new();
        // NiObjectNET: empty name + empty extra_data + null controller.
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.push(0u8); // use_external = 0
                        // No use_internal byte at this version.
        data.extend_from_slice(&5i32.to_le_bytes()); // pixel ref
        data.extend_from_slice(&1u32.to_le_bytes()); // pixel_layout
        data.extend_from_slice(&0u32.to_le_bytes()); // use_mipmaps
        data.extend_from_slice(&0u32.to_le_bytes()); // alpha_format
        data.push(1u8); // is_static

        let mut stream = NifStream::new(&data, &header);
        let tex = NiSourceTexture::parse(&mut stream).unwrap();
        assert!(!tex.use_external);
        assert_eq!(tex.pixel_data_ref.index(), Some(5));
        assert_eq!(stream.position() as usize, data.len());
    }

    /// #944 / NIF-D3-NEW-04 — the `Use Internal` byte is gated on
    /// `Use External == 0` per nif.xml line 5117. Pre-existing tests
    /// pin the `Use External == 0` (embedded) path; this one pins
    /// the `Use External == 1` (external file) path at the same
    /// legacy version, so a future refactor that flips the boolean
    /// or reorders the gate (a common mistake) gets caught immediately
    /// instead of silently drifting by 1 byte on every external
    /// texture — which is most of them.
    #[test]
    fn pre_oblivion_external_path_does_not_consume_use_internal_byte() {
        // v10.0.1.2 sits inside the `until="10.0.1.3"` window. The
        // legacy embedded path WOULD read `use_internal` here, so a
        // misgated parser that read it regardless of `use_external`
        // would drift exactly 1 byte on this fixture.
        let header = make_pre_oblivion_header(NifVersion(0x0A000102));
        let mut data = Vec::new();
        // NiObjectNET: empty name + empty extra_data + null controller.
        data.extend_from_slice(&0u32.to_le_bytes()); // name length = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
        data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
        // use_external = 1 → external file path. The `use_internal`
        // byte MUST NOT be read here even though the version satisfies
        // the `until` gate; nif.xml's gate is the AND of both.
        data.push(1u8);
        // Filename: pre-string-table layout (v < 20.2.0.7) reads a
        // sized string (u32 length + bytes).
        let filename = b"foo.dds";
        data.extend_from_slice(&(filename.len() as u32).to_le_bytes());
        data.extend_from_slice(filename);
        // _unknown_ref read at v >= 10.1.0.0 — v10.0.1.2 < that, so
        // no extra ref. (Sibling guard: this branch differs between
        // pre/post-10.1; if the parser ever read the ref at v10.0.1.2
        // it would also drift, so the consumed-bytes check covers it.)
        // Format prefs + is_static.
        data.extend_from_slice(&1u32.to_le_bytes()); // pixel_layout
        data.extend_from_slice(&0u32.to_le_bytes()); // use_mipmaps
        data.extend_from_slice(&0u32.to_le_bytes()); // alpha_format
        data.push(1u8); // is_static

        let mut stream = NifStream::new(&data, &header);
        let tex = NiSourceTexture::parse(&mut stream).unwrap();
        assert!(tex.use_external, "external path");
        assert_eq!(
            tex.filename.as_deref(),
            Some("foo.dds"),
            "filename must read immediately after `use_external == 1`; \
             a flipped Use Internal gate would have eaten the first \
             byte of the sized-string length here"
        );
        assert_eq!(
            tex.pixel_data_ref,
            BlockRef::NULL,
            "external path stores no pixel data ref"
        );
        assert_eq!(
            tex.pixel_layout, 1,
            "format prefs must follow the filename — a misgated parser would shift this"
        );
        assert_eq!(
            stream.position() as usize,
            data.len(),
            "parser must consume the external block exactly — no drift from a phantom use_internal byte"
        );
    }

    #[test]
    fn parse_ni_pixel_data_oblivion() {
        let header = make_oblivion_header();
        let mut data = Vec::new();

        // NiPixelFormat: pixel_format
        data.extend_from_slice(&1u32.to_le_bytes()); // RGBA
                                                     // New layout (v20.0.0.5 >= 10.4.0.2): bits_per_pixel(u8), renderer_hint(u32),
                                                     // extra_data(u32), flags(u8), tiling(u32)
        data.push(32u8); // bits_per_pixel
        data.extend_from_slice(&0u32.to_le_bytes()); // renderer_hint
        data.extend_from_slice(&0u32.to_le_bytes()); // extra_data
        data.push(0u8); // flags
        data.extend_from_slice(&0u32.to_le_bytes()); // tiling
                                                     // No sRGB (v20.0.0.5 < 20.3.0.4)
                                                     // 4 channels: each is (type:u32, convention:u32, bits:u8, signed:bool=u8)
        for _ in 0..4 {
            data.extend_from_slice(&0u32.to_le_bytes()); // component type
            data.extend_from_slice(&0u32.to_le_bytes()); // convention
            data.push(8u8); // bits per channel
            data.push(0u8); // is_signed (bool as u8 via read_byte_bool)
        }
        // NiPixelData fields
        data.extend_from_slice(&(-1i32).to_le_bytes()); // palette_ref (NULL)
        data.extend_from_slice(&1u32.to_le_bytes()); // num_mipmaps
        data.extend_from_slice(&4u32.to_le_bytes()); // bytes_per_pixel
                                                     // MipMap[0]: width, height, offset
        data.extend_from_slice(&2u32.to_le_bytes()); // width
        data.extend_from_slice(&2u32.to_le_bytes()); // height
        data.extend_from_slice(&0u32.to_le_bytes()); // offset
                                                     // num_pixels (total bytes)
        data.extend_from_slice(&16u32.to_le_bytes()); // 2×2×4 = 16 bytes
                                                      // num_faces
        data.extend_from_slice(&1u32.to_le_bytes());
        // pixel_data: 16 bytes of RGBA
        data.extend_from_slice(&[
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 128, 128, 128, 255,
        ]);

        let mut stream = NifStream::new(&data, &header);
        let pix = NiPixelData::parse(&mut stream).unwrap();

        assert_eq!(pix.pixel_format, 1); // RGBA
        assert_eq!(pix.bits_per_pixel, 32);
        assert_eq!(pix.num_mipmaps, 1);
        assert_eq!(pix.bytes_per_pixel, 4);
        assert_eq!(pix.mipmaps.len(), 1);
        assert_eq!(pix.mipmaps[0].width, 2);
        assert_eq!(pix.mipmaps[0].height, 2);
        assert_eq!(pix.num_faces, 1);
        assert_eq!(pix.pixel_data.len(), 16);
        assert_eq!(pix.pixel_data[0], 255); // first pixel R
        assert_eq!(stream.position() as usize, data.len());
    }
}

// ── NiTextureEffect ────────────────────────────────────────────────────
//
// Inherits NiDynamicEffect (which in turn inherits NiAVObject). Describes
// a projected texture — sphere/env maps, gobos, fog maps, projected
// shadows. Used by Oblivion magic FX meshes and various projected-shadow
// setups. See issue #163.
//
// Wire layout (up to Skyrim — FO4 removes NiDynamicEffect from the chain):
//
//   NiAVObject base
//   [NiDynamicEffect] switch_state:u8 (since 10.1.0.106, < BSVER 130)
//                     num_affected_nodes:u32 (since 10.1.0.0, < BSVER 130)
//                     affected_nodes:u32[n]
//   model_projection_matrix: Matrix33
//   model_projection_translation: Vector3
//   texture_filtering: u32 (TexFilterMode enum)
//   max_anisotropy: u16 (since 20.5.0.4)
//   texture_clamping: u32 (TexClampMode enum)
//   texture_type: u32 (TextureType enum)
//   coordinate_generation_type: u32 (CoordGenType enum)
//   source_texture_ref: Ref<NiSourceTexture> (since 3.1 — always for us)
//   enable_plane: u8 (byte bool)
//   plane: NiPlane { normal:Vec3, constant:f32 } = 16 bytes
//   ps2_l: i16 (until 10.2.0.0 — present in Oblivion v20.0.0.5... wait,
//              nif.xml says "until 10.2.0.0"; Oblivion is 20.0.0.5 which is
//              AFTER that, so PS2 fields are ABSENT for Oblivion)
//   ps2_k: i16 (until 10.2.0.0 — same)

/// NiTextureEffect — projected texture effect (env map, gobo, fog, etc.).
#[derive(Debug)]
pub struct NiTextureEffect {
    pub av: NiAVObjectData,
    pub switch_state: bool,
    pub affected_nodes: Vec<u32>,
    pub model_projection_matrix: NiMatrix3,
    pub model_projection_translation: [f32; 3],
    pub texture_filtering: u32,
    pub max_anisotropy: u16,
    pub texture_clamping: u32,
    pub texture_type: u32,
    pub coordinate_generation_type: u32,
    pub source_texture_ref: BlockRef,
    pub enable_plane: bool,
    /// Clipping plane: (normal_x, normal_y, normal_z, constant).
    pub plane: [f32; 4],
    pub ps2_l: i16,
    pub ps2_k: i16,
}

impl NiObject for NiTextureEffect {
    fn block_type_name(&self) -> &'static str {
        "NiTextureEffect"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_object_net(&self) -> Option<&dyn HasObjectNET> {
        Some(self)
    }
    fn as_av_object(&self) -> Option<&dyn HasAVObject> {
        Some(self)
    }
}

impl HasObjectNET for NiTextureEffect {
    fn name(&self) -> Option<&str> {
        self.av.net.name.as_deref()
    }
    fn name_arc(&self) -> Option<&std::sync::Arc<str>> {
        self.av.net.name.as_ref()
    }
    fn extra_data_refs(&self) -> &[BlockRef] {
        &self.av.net.extra_data_refs
    }
    fn controller_ref(&self) -> BlockRef {
        self.av.net.controller_ref
    }
}

impl HasAVObject for NiTextureEffect {
    fn flags(&self) -> u32 {
        self.av.flags
    }
    fn transform(&self) -> &NiTransform {
        &self.av.transform
    }
    fn properties(&self) -> &[BlockRef] {
        &self.av.properties
    }
    fn collision_ref(&self) -> BlockRef {
        self.av.collision_ref
    }
}

impl NiTextureEffect {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let av = NiAVObjectData::parse(stream)?;

        // NiDynamicEffect base fields — same version gates as NiLight.
        // See crates/nif/src/blocks/light.rs for the full rationale.
        let switch_state = if stream.version() >= NifVersion(0x0A01006A) {
            stream.read_u8()? != 0
        } else {
            true
        };
        let affected_nodes = if stream.version() >= NifVersion(0x0A010000) {
            let count = stream.read_u32_le()?;
            let mut nodes = stream.allocate_vec(count)?;
            for _ in 0..count {
                nodes.push(stream.read_u32_le()?);
            }
            nodes
        } else {
            Vec::new()
        };

        let model_projection_matrix = stream.read_ni_matrix3()?;
        let p = stream.read_ni_point3()?;
        let model_projection_translation = [p.x, p.y, p.z];

        let texture_filtering = stream.read_u32_le()?;
        let max_anisotropy = if stream.version() >= NifVersion(0x14050004) {
            stream.read_u16_le()?
        } else {
            0
        };
        let texture_clamping = stream.read_u32_le()?;
        let texture_type = stream.read_u32_le()?;
        let coordinate_generation_type = stream.read_u32_le()?;
        let source_texture_ref = stream.read_block_ref()?;

        let enable_plane = stream.read_u8()? != 0;
        // NiPlane: vec3 normal + f32 constant = 16 bytes.
        let pn = stream.read_ni_point3()?;
        let pc = stream.read_f32_le()?;
        let plane = [pn.x, pn.y, pn.z, pc];

        // NiTextureEffect PS2 L/K: nif.xml `until="10.2.0.0"` inclusive
        // per the version.rs doctrine — present at v <= 10.2.0.0.
        // Oblivion (v20.0.0.5) sits well past the boundary.
        let (ps2_l, ps2_k) = if stream.version() <= NifVersion(0x0A020000) {
            // No i16 reader in NifStream; sign-reinterpret the u16.
            let l = stream.read_u16_le()? as i16;
            let k = stream.read_u16_le()? as i16;
            (l, k)
        } else {
            (0, 0)
        };

        // #723 / NIF-D2-04 — pre-4.1 `Unknown Short` field. nif.xml
        // line 5201 gates this on `until="4.1.0.12"` (inclusive per the
        // version.rs doctrine — present at v <= 4.1.0.12). No Bethesda
        // title ships in this band; guards pre-Gamebryo NetImmerse demo
        // / Civ IV / Dark Age of Camelot compat.
        if stream.version() <= NifVersion(0x0401000C) {
            let _unknown_short = stream.read_u16_le()?;
        }

        Ok(Self {
            av,
            switch_state,
            affected_nodes,
            model_projection_matrix,
            model_projection_translation,
            texture_filtering,
            max_anisotropy,
            texture_clamping,
            texture_type,
            coordinate_generation_type,
            source_texture_ref,
            enable_plane,
            plane,
            ps2_l,
            ps2_k,
        })
    }
}
