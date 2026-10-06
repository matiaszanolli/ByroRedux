//! One decode of a Scaleform movie, shared by every load stage (#2968).
//!
//! `SwfPlayer`'s constructors used to hand raw bytes to four independent
//! stages — profile detection, host-object injection, `ImportAssets`
//! extraction, and Ruffle's own `SwfMovie::from_data` — each of which began by
//! inflating the whole compressed stream again, and two of which then walked
//! every tag. On Fallout 4's multi-megabyte `hudmenu.swf` / `pipboymenu.swf`
//! that was four zlib inflates and two full tag walks per menu open, run
//! synchronously on the winit main-loop thread, buying nothing: the stages
//! took bytes only because that was the convenient signature, not because any
//! of them needed a fresh decode.
//!
//! [`prepare_movie`] does the decompress once and the tag parse at most once,
//! then hands each stage what it actually wanted. The final
//! `SwfMovie::from_data` still decompresses — Ruffle exposes no constructor
//! taking an already-decoded `SwfBuf` — so a menu open costs two inflates
//! rather than four, and one tag walk rather than two.
//!
//! #3771 — this is an end-to-end number for `SwfPlayer::from_resource_provider`
//! (the archive route, and the workspace's only production caller), not a
//! crate-internal one: `profile` there is `Option<ScaleformProfile>` and
//! [`prepare_movie`] is trusted for its own single detect rather than a
//! caller pre-extracting the archive entry and re-inflating it just to hand
//! in a value for the mismatch-guard cross-check. A caller that DOES have
//! an independent profile source may still pass `Some(..)` — the guard
//! stays available — but nothing in the workspace needs to today, and doing
//! so purely to answer this module's own detection with itself would spend
//! a second archive decompression and whole-stream inflate to buy a
//! tautology.

use url::Url;

use crate::avm2_host::{inject_into_parsed_movie, ScaleformHostObjectState};

/// #4470 — normalize Starfield's Scaleform SWF dialect before Ruffle sees
/// it. The pinned `swf` reader (`0dde9813`) rejects a `PlaceObject3` whose
/// flags carry neither `MOVE` nor `HAS_CHARACTER` — "Invalid PlaceObject
/// type" — while Scaleform's authoring emits exactly that form as a
/// place-by-class-name record: byte-verified on the shipped
/// `interface\hudmenu.swf`, all four PlaceObject3 records (nested in
/// DefineSprites) carry flags `0x0824` = HAS_MATRIX | HAS_NAME |
/// HAS_CLASS_NAME with both action bits clear.
///
/// The shim sets the `MOVE` bit on those records, the nearest action the
/// stock reader accepts (`PlaceObjectAction::Modify`): length-preserving,
/// so no tag or stream length is rewritten. The four records degrade to
/// Modify-at-empty-depth (inert on the first frames they appear in)
/// instead of failing the whole parse — which is what unblocks menu
/// loading; the class-instance placement itself remains unmodelled until
/// an upstream re-pin (#4470 keeps tracking that).
///
/// Returns `None` when nothing needed patching (including non-CWS/FWS
/// containers), so the no-injection fast path can keep the original bytes.
pub(crate) fn normalize_scaleform_dialect(swf_data: &[u8]) -> Option<Vec<u8>> {
    const DEFINE_SPRITE: u16 = 39;
    const PLACE_OBJECT_3: u16 = 70;
    if swf_data.len() < 8 {
        return None;
    }
    let signature = &swf_data[0..3];
    let (mut file, compressed): (Vec<u8>, bool) = match signature {
        b"CWS" => ( decompress_zlib_after_header(swf_data)?, true),
        b"FWS" => (swf_data.to_vec(), false),
        // ZWS (LZMA) is not something this shim touches; leave it to the
        // stock loader.
        _ => return None,
    };
    let tags_start = tag_stream_start(&file)?;
    let patched = patch_place_object3_stream(&mut file[tags_start..], DEFINE_SPRITE, PLACE_OBJECT_3);
    if patched == 0 {
        return None;
    }
    // Re-emit uncompressed: FWS signature + fixed total length. The tag
    // stream's own bytes never moved.
    file[0..3].copy_from_slice(b"FWS");
    let len = file.len() as u32;
    file[4..8].copy_from_slice(&len.to_le_bytes());
    log::info!(
        "Scaleform dialect: set MOVE on {patched} PlaceObject3 record(s) with neither          MOVE nor HAS_CHARACTER (Starfield hudmenu class-name places, #4470)"
    );
    let _ = compressed;
    Some(file)
}

fn decompress_zlib_after_header(swf_data: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut out = swf_data[0..8].to_vec();
    let mut decoder = flate2::read::ZlibDecoder::new(&swf_data[8..]);
    let mut body = Vec::new();
    decoder.read_to_end(&mut body).ok()?;
    out.extend_from_slice(&body);
    Some(out)
}

/// Byte offset where the tag stream begins in an uncompressed SWF: skip
/// the 8-byte file header, the stage RECT (5-bit count + 4 fields), the
/// frame-rate and frame-count u16s.
fn tag_stream_start(file: &[u8]) -> Option<usize> {
    if file.len() < 9 {
        return None;
    }
    let nbits = file[8] >> 3;
    let total_bits = 5 + nbits as usize * 4;
    let rect_end = 8 + (total_bits + 7) / 8 + 4;
    (rect_end < file.len()).then_some(rect_end)
}

/// Walk a tag stream (and DefineSprite bodies within it), setting MOVE on
/// every PlaceObject3 whose action bits are both clear. Length-preserving;
/// returns the patch count.
fn patch_place_object3_stream(stream: &mut [u8], define_sprite: u16, place_object_3: u16) -> usize {
    let mut patched = 0;
    let mut p = 0usize;
    while p + 2 <= stream.len() {
        let code_and_len = u16::from_le_bytes([stream[p], stream[p + 1]]);
        let code = code_and_len >> 6;
        let mut len = (code_and_len & 0x3F) as usize;
        let mut hdr = 2usize;
        if len == 0x3F {
            if p + 6 > stream.len() {
                break;
            }
            len = u32::from_le_bytes([stream[p + 2], stream[p + 3], stream[p + 4], stream[p + 5]])
                as usize;
            hdr = 6;
        }
        // An END tag (code 0) closes this stream.
        if code == 0 {
            break;
        }
        let body_start = p + hdr;
        let body_end = body_start + len;
        if body_end > stream.len() {
            break;
        }
        if code == place_object_3 && len >= 4 {
            let flags = u16::from_le_bytes([stream[body_start], stream[body_start + 1]]);
            if flags & 0b0000_0000_0000_0011 == 0 {
                let fixed = flags | 0b1; // MOVE
                stream[body_start..body_start + 2].copy_from_slice(&fixed.to_le_bytes());
                patched += 1;
            }
        } else if code == define_sprite && len >= 4 {
            // DefineSprite body: character id u16 + frame count u16, then a
            // nested (END-terminated) tag stream.
            patched += patch_place_object3_stream(
                &mut stream[body_start + 4..body_end],
                define_sprite,
                place_object_3,
            );
        }
        p = body_end;
    }
    patched
}

use crate::navigator::import_asset_paths_from_tags;
use crate::{ScaleformHostCatalog, ScaleformProfile};

/// How many times [`prepare_movie`] decoded the movie. Reported so the
/// property this module exists to hold — "one decompress, at most one tag
/// parse, per menu open" — is assertable rather than merely intended.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SwfDecodeCounts {
    pub decompresses: usize,
    pub tag_parses: usize,
}

/// A movie decoded once and readied for `SwfMovie::from_data`.
pub(crate) struct PreparedMovie {
    pub profile: ScaleformProfile,
    pub host_object_state: ScaleformHostObjectState,
    /// Bytes to hand to Ruffle — the adapter-patched movie when injection
    /// rewrote it, otherwise the caller's original bytes untouched.
    pub data: Vec<u8>,
    /// The root movie's `ImportAssets` targets, resolved to archive paths.
    /// Empty unless the caller passed a `movie_url` (the archive route).
    pub import_asset_paths: Vec<String>,
    /// #3770 — one message per root-movie `ImportAssets` URL that failed
    /// to resolve to an archive path. Pre-#3770 a single such failure
    /// aborted the whole scan (`import_asset_paths` above ending up
    /// empty) AND the movie load itself; now the resolvable siblings
    /// still populate `import_asset_paths` and these messages surface
    /// through `NavigatorState.errors` (`SwfPlayer::from_resource_provider`
    /// pushes them in at `ScaleformNavigatorRuntime::create`), matching
    /// the non-fatal policy fetch-time and depth-≥1 failures already had.
    pub root_import_errors: Vec<String>,
    pub decode_counts: SwfDecodeCounts,
}

/// Decode `swf_data` once and run every load-time stage off that decode.
///
/// `expected_profile` is checked against the movie's own declaration before
/// any further work, preserving the "profile mismatch" error the archive and
/// explicit-profile constructors raise. `movie_url` is `Some` only on the
/// archive route, where the navigator needs the root movie's `ImportAssets`
/// list; on the loose-file routes the tag parse is skipped entirely for an
/// AVM1 movie, which needs no injection either.
pub(crate) fn prepare_movie(
    swf_data: &[u8],
    expected_profile: Option<ScaleformProfile>,
    movie_url: Option<&Url>,
) -> Result<PreparedMovie, String> {
    // #4470 — the dialect shim runs before every stage, so detection,
    // injection and Ruffle's parse all see the same normalized bytes.
    let normalized = normalize_scaleform_dialect(swf_data);
    let swf_data: &[u8] = normalized.as_deref().unwrap_or(swf_data);
    let decompressed =
        swf::decompress_swf(swf_data).map_err(|error| format!("Failed to parse SWF: {error}"))?;
    let profile = ScaleformProfile::from_header(&decompressed.header);
    if let Some(expected) = expected_profile {
        if profile != expected {
            return Err(format!(
                "Scaleform profile mismatch: requested {expected:?}, movie requires {profile:?}"
            ));
        }
    }

    let catalog = ScaleformHostCatalog::for_profile(profile);
    let needs_injection = catalog.host_object().is_some();
    if !needs_injection && movie_url.is_none() {
        return Ok(logged(PreparedMovie {
            profile,
            host_object_state: ScaleformHostObjectState::NotRequired,
            data: swf_data.to_vec(),
            import_asset_paths: Vec::new(),
            root_import_errors: Vec::new(),
            decode_counts: SwfDecodeCounts {
                decompresses: 1,
                tag_parses: 0,
            },
        }));
    }

    let movie = swf::parse_swf(&decompressed).map_err(|error| format!("parsing SWF: {error}"))?;
    // Read before injection, which consumes the tag list. Equivalent to
    // reading it after: injection only replaces and inserts `DoAbc`/`DoAbc2`
    // tags, and never touches `ImportAssets`.
    //
    // #3770 — partitioned, not `?`-short-circuited: a single unresolvable
    // URL must not cost every other resolvable sibling (or the movie load
    // itself). `root_import_errors` surfaces through `NavigatorState.errors`
    // once the navigator exists — see `PreparedMovie::root_import_errors`.
    let (import_asset_paths, root_import_errors) = match movie_url {
        Some(movie_url) => import_asset_paths_from_tags(movie_url, &movie.tags),
        None => (Vec::new(), Vec::new()),
    };
    let (patched, host_object_state) = inject_into_parsed_movie(movie, catalog)?;

    Ok(logged(PreparedMovie {
        profile,
        host_object_state,
        data: patched.unwrap_or_else(|| swf_data.to_vec()),
        import_asset_paths,
        root_import_errors,
        decode_counts: SwfDecodeCounts {
            decompresses: 1,
            tag_parses: 1,
        },
    }))
}

/// Report what the preparation cost, so a regression back toward the
/// four-inflate load path is visible in a log rather than only in a test.
fn logged(prepared: PreparedMovie) -> PreparedMovie {
    log::debug!(
        "Scaleform movie prepared as {:?}: {} decompress, {} tag parse, \
         host object {:?}, {} import(s) (#2968)",
        prepared.profile,
        prepared.decode_counts.decompresses,
        prepared.decode_counts.tag_parses,
        prepared.host_object_state,
        prepared.import_asset_paths.len(),
    );
    prepared
}

#[cfg(test)]
mod tests {
    use super::*;
    use swf::{DoAbc2, DoAbc2Flag, FileAttributes, SwfStr, Tag};

    /// Minimal AVM1 movie — no `FileAttributes`, so `is_action_script_3()` is
    /// false and no host object is required.
    fn avm1_movie() -> Vec<u8> {
        let mut header = swf::Header::default_with_swf_version(8);
        header.num_frames = 1;
        header.frame_rate = swf::Fixed8::from_f32(30.0);
        let mut out = Vec::new();
        swf::write_swf(&header, &[Tag::ShowFrame], &mut out).unwrap();
        out
    }

    /// Minimal AVM2 movie carrying an ABC tag that does not declare Fallout
    /// 4's `BGSCodeObj` contract, so injection reports `NotPresent` and leaves
    /// the bytes alone.
    fn avm2_movie() -> Vec<u8> {
        let mut header = swf::Header::default_with_swf_version(15);
        header.num_frames = 1;
        header.frame_rate = swf::Fixed8::from_f32(30.0);
        let abc = Vec::new();
        let tags = [
            Tag::FileAttributes(FileAttributes::IS_ACTION_SCRIPT_3),
            Tag::DoAbc2(DoAbc2 {
                flags: DoAbc2Flag::empty(),
                name: SwfStr::from_utf8_str("frame"),
                data: &abc,
            }),
            Tag::ShowFrame,
        ];
        let mut out = Vec::new();
        swf::write_swf(&header, &tags, &mut out).unwrap();
        out
    }

    /// #2968 — the property, not just "it still loads". A menu open decodes
    /// the movie ONCE inside preparation; before this the archive route ran
    /// `decompress_swf` three times here (detect, inject, import-scan) plus a
    /// fourth inside `SwfMovie::from_data`, and walked every tag twice.
    #[test]
    fn an_archive_menu_open_decompresses_and_parses_once() {
        let url = Url::parse("file:///interface/hudmenu.swf").unwrap();
        let prepared = prepare_movie(&avm2_movie(), None, Some(&url)).unwrap();
        assert_eq!(prepared.profile, ScaleformProfile::Fallout4Avm2);
        assert_eq!(
            prepared.decode_counts,
            SwfDecodeCounts {
                decompresses: 1,
                tag_parses: 1
            }
        );
        assert!(prepared.import_asset_paths.is_empty());
    }

    /// The loose-file AVM1 route needs neither injection nor an import scan,
    /// so it must not pay for a tag walk at all.
    #[test]
    fn a_loose_avm1_movie_is_decompressed_once_and_never_parsed() {
        let data = avm1_movie();
        let prepared = prepare_movie(&data, None, None).unwrap();
        assert_eq!(prepared.profile, ScaleformProfile::SkyrimAvm1);
        assert_eq!(
            prepared.host_object_state,
            ScaleformHostObjectState::NotRequired
        );
        assert_eq!(
            prepared.decode_counts,
            SwfDecodeCounts {
                decompresses: 1,
                tag_parses: 0
            }
        );
        // Untouched bytes, not a re-serialisation.
        assert_eq!(prepared.data, data);
    }

    /// A movie whose profile contradicts the caller's must be rejected before
    /// any injection or import work — the error the archive and
    /// explicit-profile constructors used to raise from `detect`.
    #[test]
    fn a_profile_mismatch_is_rejected_before_any_further_decode() {
        let Err(error) = prepare_movie(&avm1_movie(), Some(ScaleformProfile::Fallout4Avm2), None)
        else {
            panic!("an AVM1 movie must not satisfy a Fallout4Avm2 request");
        };
        assert!(error.contains("profile mismatch"), "{error}");
    }

    /// An AVM2 movie without Fallout 4's root-object contract keeps its
    /// original bytes: `None` from injection must not become a re-serialised
    /// movie, which would change what Ruffle parses for no reason.
    #[test]
    fn an_uninjected_avm2_movie_keeps_its_original_bytes() {
        let data = avm2_movie();
        let prepared = prepare_movie(&data, Some(ScaleformProfile::Fallout4Avm2), None).unwrap();
        assert_eq!(
            prepared.host_object_state,
            ScaleformHostObjectState::NotPresent
        );
        assert_eq!(prepared.data, data);
    }
}

// ── #4470 — Starfield's PlaceObject3 dialect ─────────────────────────────
//
// hudmenu.swf's four PlaceObject3 records (nested in DefineSprites) carry
// flags 0x0824 — HAS_MATRIX | HAS_NAME | HAS_CLASS_NAME, action bits both
// clear — which the pinned swf reader rejects as "Invalid PlaceObject
// type". The shim sets MOVE, length-preserving.

#[cfg(test)]
mod dialect_tests {
    use super::*;

    /// A minimal uncompressed SWF: header (FWS v12), a 1-bit-per-field
    /// stage RECT, frame rate/count, then the given tag stream and END.
    fn swf_with_tags(tags: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"FWS");
        out.push(12);
        out.extend_from_slice(&0u32.to_le_bytes()); // length, fixed below
        // RECT: nbits=0 → 5 header bits + 0 field bits = 1 byte (0x00).
        out.push(0x00);
        out.extend_from_slice(&30u16.to_le_bytes()); // frame rate
        out.extend_from_slice(&1u16.to_le_bytes()); // frame count
        out.extend_from_slice(tags);
        out.extend_from_slice(&[0x00, 0x00]); // END tag
        let len = out.len() as u32;
        out[4..8].copy_from_slice(&len.to_le_bytes());
        out
    }

    fn tag(code: u16, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(6 + body.len());
        out.extend_from_slice(&(((code << 6) | (body.len() as u16 & 0x3F)) as u16).to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    fn sprite(inner: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&7u16.to_le_bytes()); // character id
        body.extend_from_slice(&1u16.to_le_bytes()); // frame count
        body.extend_from_slice(inner);
        body.extend_from_slice(&[0x00, 0x00]); // sprite END
        tag(39, &body)
    }

    /// A PlaceObject3 body whose flags carry neither MOVE nor
    /// HAS_CHARACTER (the Starfield form, 0x0824), depth 0x1125.
    fn starfield_place3() -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&0x0824u16.to_le_bytes()); // flags
        body.extend_from_slice(&0x1125u16.to_le_bytes()); // depth
        body.extend_from_slice(b"SomeClass\0");
        body.extend_from_slice(&[0; 12]); // matrix bits + name stub
        body
    }

    #[test]
    fn shim_sets_move_on_starfield_place_object3_inside_sprites() {
        let file = swf_with_tags(&sprite(&tag(70, &starfield_place3())));
        let normalized =
            normalize_scaleform_dialect(&file).expect("the bad record must be patched");
        // FWS out, length fixed.
        assert_eq!(&normalized[0..3], b"FWS");
        assert_eq!(
            u32::from_le_bytes(normalized[4..8].try_into().unwrap()) as usize,
            normalized.len()
        );
        // The flags word now carries MOVE (bit 0); nothing else moved.
        let idx = normalized
            .windows(2)
            .position(|w| w == 0x0825u16.to_le_bytes())
            .expect("flags 0x0824 → 0x0825");
        assert!(idx > 8);
        assert!(!normalized.windows(2).any(|w| w == 0x0824u16.to_le_bytes()));
    }

    #[test]
    fn clean_movies_and_healthy_place3_are_untouched() {
        // A healthy PlaceObject3 (HAS_CHARACTER set) must not be rewritten.
        let mut healthy = starfield_place3();
        healthy[0..2].copy_from_slice(&0x0826u16.to_le_bytes());
        let file = swf_with_tags(&sprite(&tag(70, &healthy)));
        assert!(normalize_scaleform_dialect(&file).is_none());
        // An ordinary movie without PlaceObject3 at all.
        let plain = swf_with_tags(&tag(2, &[0x01, 0x00, 0x61, 0x00]));
        assert!(normalize_scaleform_dialect(&plain).is_none());
        // A short buffer and a non-SWF signature decline.
        assert!(normalize_scaleform_dialect(b"ZWS\x00\x00").is_none());
        assert!(normalize_scaleform_dialect(b"abc").is_none());
    }

    #[test]
    fn compressed_container_is_re_emitted_uncompressed() {
        use std::io::Write;
        let file = swf_with_tags(&sprite(&tag(70, &starfield_place3())));
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&file[8..]).unwrap();
        let body = encoder.finish().unwrap();
        let mut cws = file[0..8].to_vec();
        cws[0..3].copy_from_slice(b"CWS");
        cws.extend_from_slice(&body);
        let normalized = normalize_scaleform_dialect(&cws).expect("CWS is handled");
        assert_eq!(&normalized[0..3], b"FWS");
        assert!(normalize_scaleform_dialect(&cws).is_some());
        // The patch is visible in the re-emitted stream.
        assert!(normalized.windows(2).any(|w| w == 0x0825u16.to_le_bytes()));
    }
}
