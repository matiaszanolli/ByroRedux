//! One decode of a Scaleform movie, shared by every load stage (#2968).
//!
//! `SwfPlayer`'s constructors used to hand raw bytes to four independent
//! stages — profile detection, host-object injection, `ImportAssets`
//! extraction, and Ruffle's own `SwfMovieData::from_data` — each of which began by
//! inflating the whole compressed stream again, and two of which then walked
//! every tag. On Fallout 4's multi-megabyte `hudmenu.swf` / `pipboymenu.swf`
//! that was four zlib inflates and two full tag walks per menu open, run
//! synchronously on the winit main-loop thread, buying nothing: the stages
//! took bytes only because that was the convenient signature, not because any
//! of them needed a fresh decode.
//!
//! [`prepare_movie`] does the decompress once and the tag parse at most once,
//! then hands each stage what it actually wanted. The final
//! `SwfMovieData::from_data` still runs its own decode — Ruffle exposes no
//! constructor taking an already-decoded `SwfBuf` — but since #5479 it is
//! handed the dialect shim's uncompressed file, so on the shipped CWS content
//! a whole menu open costs ONE inflate rather than four, and one tag walk
//! rather than two.
//!
//! #5479 — the #4470 dialect shim broke that floor by inflating the raw
//! bytes a third time (and returning `None` — the conforming-CWS common
//! case — with the decode discarded). The shim now OWNS the prepare
//! decode: it inflates once, walks, and always returns the uncompressed
//! file on CWS input, so `swf::decompress_swf` takes the uncompressed
//! copy path and `SwfDecodeCounts::decompresses` counts the real
//! work again.
//!
//! #5967 — that count is measured, not a literal: the shim reports whether it
//! inflated, and `prepare_movie` adds one more if the stock decode still
//! receives a compressed container. A compressed (CWS) movie therefore reads
//! exactly 1; a shim that inflated and then handed back the compressed bytes
//! (the #5479 shape) reads 2; an uncompressed (FWS) movie has nothing to
//! inflate and reads 0.
//!
//! #5966 — every decode site in this crate refuses an over-ceiling declared
//! length first ([`ensure_declared_len_within_ceiling`]); the stock reader
//! honours the header's own `u32`, so the shim's private cap was not enough.
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
use crate::navigator::import_asset_paths_from_tags;
use crate::{ScaleformHostCatalog, ScaleformProfile};

/// #4470 — normalize Starfield's Scaleform SWF dialect before Ruffle sees
/// it. The pinned `swf` reader (`2edfd97d`) still rejects a `PlaceObject3`
/// whose flags carry neither `MOVE` nor `HAS_CHARACTER` — "Invalid
/// PlaceObject type" — while Scaleform's authoring emits exactly that form
/// as a
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
/// loading; the class-instance placement itself remains unmodelled. The
/// reader was re-pinned to `2edfd97d` in the 2026-10 dependency refresh
/// and `swf/src/read.rs` still rejects the neither-MOVE-nor-HAS_CHARACTER
/// form there, so the shim stays (#4470 keeps tracking that).
///
/// #5479 — the dialect shim owns the prepare path's one decode. On a CWS
/// movie it inflates ONCE, walks the tags, and ALWAYS returns the full
/// uncompressed file (FWS signature, length fixed, body byte-identical
/// apart from any #4470 patches) — patched or not — so the caller's
/// `swf::decompress_swf` takes the Compression::None copy path instead
/// of a second whole-stream inflate. On FWS input there is nothing to
/// inflate: patch in place and return `Some` only when a record
/// changed, letting the caller decode the original bytes otherwise.
/// `None` for everything this shim does not own — conforming FWS, ZWS
/// (LZMA, left to the stock loader), and truncated/foreign containers.
pub(crate) fn normalize_scaleform_dialect(swf_data: &[u8]) -> Option<Vec<u8>> {
    normalize_scaleform_dialect_counting(swf_data, &mut 0)
}

/// [`normalize_scaleform_dialect`], also reporting through `inflates` whether
/// the shim ran a whole-stream zlib inflate (#5967). It is set the moment the
/// inflate starts, so a shim that inflates and then returns `None` (a
/// malformed tag stream, a declared-length overrun) still counts: the caller's
/// stock decode inflates the same bytes again, which is exactly the double
/// decode [`SwfDecodeCounts`] exists to expose.
fn normalize_scaleform_dialect_counting(swf_data: &[u8], inflates: &mut usize) -> Option<Vec<u8>> {
    const DEFINE_SPRITE: u16 = 39;
    const PLACE_OBJECT_3: u16 = 70;
    if swf_data.len() < 8 {
        return None;
    }
    let was_cws = &swf_data[0..3] == b"CWS";
    let mut file = match &swf_data[0..3] {
        b"CWS" => decompress_zlib_after_header_counting(swf_data, MAX_SWF_BODY_BYTES, inflates)?,
        b"FWS" => swf_data.to_vec(),
        // ZWS (LZMA) is not something this shim touches; leave it to the
        // stock loader.
        _ => return None,
    };
    let tags_start = tag_stream_start(&file)?;
    let patched =
        patch_place_object3_stream(&mut file[tags_start..], DEFINE_SPRITE, PLACE_OBJECT_3, 0);
    if patched > 0 {
        log::info!(
            "Scaleform dialect: set MOVE on {patched} PlaceObject3 record(s) with neither MOVE \
             nor HAS_CHARACTER (Starfield hudmenu class-name places, #4470)"
        );
    } else if !was_cws {
        // #5479 — an FWS movie needed no patch and no inflate happened
        // here: `None` keeps the caller on the original bytes.
        return None;
    }
    if was_cws {
        // Re-emit uncompressed: FWS signature + fixed total length. The
        // tag stream's own bytes never moved.
        file[0..3].copy_from_slice(b"FWS");
        let len = file.len() as u32;
        file[4..8].copy_from_slice(&len.to_le_bytes());
    }
    Some(file)
}

/// Absolute ceiling on an inflated CWS body (#5719). The header's declared
/// length bounds a stream that lies about a SMALL size, but that field is
/// attacker-controlled (`0xFFFF_FFFF` is a legal value), so on its own it
/// lets a ~4 MB bomb grow the buffer toward 4 GiB. Measured against the
/// archived corpus (every `.swf` in Skyrim SE, FO4, FO76 and Starfield):
/// the largest declared length is 65,902,267 B — FO76's CJK
/// `fonts_zhhant.swf`, a font container; the largest menu is under 20 MB.
/// 128 MiB is about twice the largest shipped movie.
const MAX_SWF_BODY_BYTES: u64 = 128 * 1024 * 1024;

/// Inflate a CWS body, bounded by the header's declared uncompressed length
/// (`u32` at `[4..8]`, counting the 8-byte header) so a hostile stream
/// cannot balloon past what it claims (#5456), and by [`MAX_SWF_BODY_BYTES`]
/// so a header cannot claim its way past any sane size (#5719). A body that
/// overruns either is malformed: `None` hands the movie to the stock
/// loader, exactly what happened before the shim owned the decode.
#[cfg(test)]
fn decompress_zlib_after_header(swf_data: &[u8]) -> Option<Vec<u8>> {
    decompress_zlib_after_header_capped(swf_data, MAX_SWF_BODY_BYTES)
}

#[cfg(test)]
fn decompress_zlib_after_header_capped(swf_data: &[u8], ceiling: u64) -> Option<Vec<u8>> {
    decompress_zlib_after_header_counting(swf_data, ceiling, &mut 0)
}

fn decompress_zlib_after_header_counting(
    swf_data: &[u8],
    ceiling: u64,
    inflates: &mut usize,
) -> Option<Vec<u8>> {
    use std::io::Read;
    let declared_body =
        u64::from(u32::from_le_bytes(swf_data[4..8].try_into().ok()?).saturating_sub(8));
    if declared_body > ceiling {
        // The claim alone is past any shipped movie: decline before
        // inflating a single byte.
        return None;
    }
    *inflates = 1;
    let mut out = swf_data[0..8].to_vec();
    // One byte past the declared length is enough to detect an overrun
    // without ever materializing the rest.
    let decoder = flate2::read::ZlibDecoder::new(&swf_data[8..]).take(declared_body + 1);
    let mut body = Vec::new();
    { decoder }.read_to_end(&mut body).ok()?;
    if body.len() as u64 > declared_body {
        return None;
    }
    out.extend_from_slice(&body);
    Some(out)
}

/// Refuse a movie whose container header claims more than
/// [`MAX_SWF_BODY_BYTES`], before ANYTHING in this crate decodes it (#5966).
///
/// #5719 bounded only the dialect shim's own inflate: a declined claim made
/// the shim return `None`, and the very next call — `swf::decompress_swf`
/// on the original bytes — reads `take(body_len).read_to_end(..)` with
/// `body_len` taken from that same attacker-controlled `u32`, so a ~1 MB
/// zlib body under a `0xFFFF_FFFF` header still inflated to about a gigabyte
/// (measured: 1,073,741,819 B in 0.62 s, 1.1 GB RSS). Ruffle's
/// `SwfMovieData::from_data` then decodes it again. The navigator's
/// dependency path, `ScaleformProfile::detect` and the AVM1 scanner call the
/// stock reader directly and never saw the ceiling at all.
///
/// Every decode site calls this first, so an over-ceiling claim is a hard
/// `Err` however the movie is reached. It reads the claim for all three
/// containers (FWS / CWS / ZWS): LZMA is handed to the stock reader too, and
/// an FWS claim bounds nothing but a hostile header. Input too short to carry
/// a header passes through to the decoder, which rejects it itself.
pub(crate) fn ensure_declared_len_within_ceiling(swf_data: &[u8]) -> Result<(), String> {
    ensure_declared_len_within(swf_data, MAX_SWF_BODY_BYTES)
}

fn ensure_declared_len_within(swf_data: &[u8], ceiling: u64) -> Result<(), String> {
    if !matches!(swf_data.get(..3), Some(b"FWS" | b"CWS" | b"ZWS")) {
        return Ok(());
    }
    let Some(claim) = swf_data.get(4..8).and_then(|b| <[u8; 4]>::try_from(b).ok()) else {
        return Ok(());
    };
    let declared_body = u64::from(u32::from_le_bytes(claim).saturating_sub(8));
    if declared_body > ceiling {
        return Err(format!(
            "SWF header declares a {declared_body}-byte body, over the {ceiling}-byte \
             ceiling (#5719/#5966): refusing to decode a hostile or corrupt movie"
        ));
    }
    Ok(())
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
    let rect_end = 8 + total_bits.div_ceil(8) + 4;
    (rect_end < file.len()).then_some(rect_end)
}

/// Deepest tag-stream nesting the walker descends to: the movie's root
/// stream is depth 0 and a `DefineSprite` body is depth 1. The SWF spec
/// does not allow a `DefineSprite` inside a `DefineSprite`, so a sprite
/// found at depth 1 is not walked — an unbounded descent was a stack
/// overflow (process abort) on a crafted movie at ~10 bytes per level
/// (#5456). Retail content never nests sprites, so nothing it carries is
/// skipped.
const MAX_SPRITE_DEPTH: usize = 1;

/// Walk a tag stream (and DefineSprite bodies within it, up to
/// [`MAX_SPRITE_DEPTH`]), setting MOVE on every PlaceObject3 whose action
/// bits are both clear. Length-preserving; returns the patch count.
fn patch_place_object3_stream(
    stream: &mut [u8],
    define_sprite: u16,
    place_object_3: u16,
    depth: usize,
) -> usize {
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
        } else if code == define_sprite && len >= 4 && depth < MAX_SPRITE_DEPTH {
            // DefineSprite body: character id u16 + frame count u16, then a
            // nested (END-terminated) tag stream.
            patched += patch_place_object3_stream(
                &mut stream[body_start + 4..body_end],
                define_sprite,
                place_object_3,
                depth + 1,
            );
        }
        p = body_end;
    }
    patched
}

/// How many times [`prepare_movie`] decoded the movie. Reported so the
/// property this module exists to hold — "one decompress, at most one tag
/// parse, per menu open" — is assertable rather than merely intended.
///
/// `decompresses` counts whole-stream zlib/LZMA inflates actually performed
/// (#5967): the shim's, plus the stock decode's when it still receives a
/// compressed container. An uncompressed movie has nothing to inflate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SwfDecodeCounts {
    pub decompresses: usize,
    pub tag_parses: usize,
}

/// A movie decoded once and readied for `SwfMovieData::from_data`.
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
    /// #5479 / #5967 — `decompresses` is the load path's measured
    /// whole-stream inflate count: the dialect shim's zlib inflate (whose
    /// output the `swf::decompress_swf` step consumes as an uncompressed
    /// copy) plus `swf::decompress_swf`'s own when it is still handed a
    /// CWS/ZWS container. Not a literal, so a second inflate shows up.
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
    // #4470 / #5479 — the dialect shim runs before every stage, so
    // detection, injection and Ruffle's parse all see the same
    // normalized bytes. On a CWS movie the shim's inflate IS this
    // path's one decode: it returns the uncompressed file, and the
    // `swf::decompress_swf` below takes the Compression::None copy path
    // instead of a second whole-stream inflate. (Ruffle's own decode
    // inside `SwfMovieData::from_data` is outside this crate's reach.)
    //
    // #5966 — the declared-length ceiling gates the whole path, not just the
    // shim's inflate: the stock decode below honours the header's own `u32`.
    ensure_declared_len_within_ceiling(swf_data)?;
    let mut shim_inflates = 0;
    let normalized = normalize_scaleform_dialect_counting(swf_data, &mut shim_inflates);
    let swf_data: &[u8] = normalized.as_deref().unwrap_or(swf_data);
    // #5967 — the stock decode inflates only if what it receives is still
    // compressed. A shim that handed back FWS leaves it an uncompressed copy;
    // a shim that inflated and then declined (or never ran) leaves a CWS/ZWS
    // container for it to inflate AGAIN, and that second pass is counted.
    let stock_inflates = usize::from(matches!(swf_data.get(..3), Some(b"CWS" | b"ZWS")));
    let decompresses = shim_inflates + stock_inflates;
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
                decompresses,
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
            decompresses,
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

/// A CWS movie whose header claims `u32::MAX` bytes over a small, valid zlib
/// body — the #5719 / #5966 hostile shape. Shared by every decode site's
/// ceiling test; the body is tiny so a site that wrongly decodes it fails the
/// assertion instead of allocating gigabytes.
#[cfg(test)]
pub(crate) fn hostile_cws_header_bomb() -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&vec![0u8; 64 * 1024]).unwrap();
    let mut out = b"CWS".to_vec();
    out.push(15); // version
    out.extend_from_slice(&u32::MAX.to_le_bytes());
    out.extend_from_slice(&encoder.finish().unwrap());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use swf::{DoAbc2, DoAbc2Flag, FileAttributes, SwfStr, Tag};

    /// Re-wrap an uncompressed (FWS) movie as a real zlib-compressed (CWS)
    /// one — what Skyrim and Fallout 4 actually ship (51 of 53 and 311 of 311
    /// interface movies). The header's length stays the uncompressed total.
    fn to_cws(fws: &[u8]) -> Vec<u8> {
        use std::io::Write;
        assert_eq!(&fws[0..3], b"FWS");
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&fws[8..]).unwrap();
        let mut cws = fws[0..8].to_vec();
        cws[0..3].copy_from_slice(b"CWS");
        cws.extend_from_slice(&encoder.finish().unwrap());
        cws
    }

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
    /// fourth inside `SwfMovieData::from_data`, and walked every tag twice.
    #[test]
    fn an_archive_menu_open_decompresses_and_parses_once() {
        let url = Url::parse("file:///interface/hudmenu.swf").unwrap();
        let prepared = prepare_movie(&avm2_movie(), None, Some(&url)).unwrap();
        assert_eq!(prepared.profile, ScaleformProfile::Fallout4Avm2);
        // `avm2_movie()` is uncompressed (FWS): nothing to inflate, one walk.
        assert_eq!(
            prepared.decode_counts,
            SwfDecodeCounts {
                decompresses: 0,
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
        // `avm1_movie()` is uncompressed (FWS): nothing to inflate, no walk.
        assert_eq!(
            prepared.decode_counts,
            SwfDecodeCounts {
                decompresses: 0,
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

    /// #5967 — the single-decode contract on the COMPRESSED path that ships.
    /// The FWS fixtures above cannot see a re-inflate (they have nothing to
    /// inflate); a real CWS archive open must read exactly one inflate, and
    /// the bytes handed to Ruffle must already be the uncompressed file.
    #[test]
    fn a_cws_archive_menu_open_inflates_exactly_once() {
        let url = Url::parse("file:///interface/hudmenu.swf").unwrap();
        let prepared = prepare_movie(&to_cws(&avm2_movie()), None, Some(&url)).unwrap();
        assert_eq!(prepared.profile, ScaleformProfile::Fallout4Avm2);
        assert_eq!(
            prepared.decode_counts,
            SwfDecodeCounts {
                decompresses: 1,
                tag_parses: 1
            },
            "a CWS open is one inflate (the shim's) and one tag walk"
        );
        assert_eq!(
            &prepared.data[0..3],
            b"FWS",
            "Ruffle must be handed the shim's uncompressed decode, not CWS to re-inflate"
        );
    }

    /// The loose AVM1 route on a compressed movie: one inflate, no tag walk,
    /// and the original uncompressed file comes back byte for byte.
    #[test]
    fn a_cws_loose_avm1_movie_inflates_once_and_is_never_parsed() {
        let fws = avm1_movie();
        let prepared = prepare_movie(&to_cws(&fws), None, None).unwrap();
        assert_eq!(prepared.profile, ScaleformProfile::SkyrimAvm1);
        assert_eq!(
            prepared.decode_counts,
            SwfDecodeCounts {
                decompresses: 1,
                tag_parses: 0
            }
        );
        assert_eq!(prepared.data, fws, "the shim's decode IS the original file");
    }

    /// The counter has teeth: a shim that inflates and then declines leaves
    /// the compressed container for the stock decode to inflate again, and
    /// that first inflate must still be counted (the #5479 shape).
    #[test]
    fn a_shim_that_inflates_and_then_declines_still_counts_its_inflate() {
        use std::io::Write;
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&[0x00, 0x00]).unwrap();
        // 8-byte header + a 2-byte body: too short to carry a RECT and a
        // tag stream, so the shim declines AFTER inflating it.
        let mut cws = b"CWS".to_vec();
        cws.push(15);
        cws.extend_from_slice(&10u32.to_le_bytes());
        cws.extend_from_slice(&encoder.finish().unwrap());
        let mut inflates = 0;
        assert!(normalize_scaleform_dialect_counting(&cws, &mut inflates).is_none());
        assert_eq!(inflates, 1, "the discarded inflate must be counted");
        // And a claim past the ceiling is declined without any inflate at all.
        let mut inflates = 0;
        assert!(
            normalize_scaleform_dialect_counting(&hostile_cws_header_bomb(), &mut inflates)
                .is_none()
        );
        assert_eq!(
            inflates, 0,
            "an over-ceiling claim is refused before inflating"
        );
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
        out.extend_from_slice(&((code << 6) | (body.len() as u16 & 0x3F)).to_le_bytes());
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

    /// #5479 — the single-decode contract: a CONFORMING CWS movie (the
    /// Skyrim/FO4 common case, where the pre-fix shim inflated, found
    /// nothing and returned `None` with the decode discarded) now comes
    /// back as `Some` carrying the uncompressed file, body
    /// byte-identical to the original — so the caller's
    /// `swf::decompress_swf` takes the Compression::None copy path and
    /// the shim's inflate is the one counted decode. `SwfDecodeCounts::
    /// decompresses: 1` therefore measures the real work again.
    #[test]
    fn conforming_cws_returns_its_decode_with_an_untouched_body() {
        use std::io::Write;
        let file = swf_with_tags(&tag(1, &[])); // ShowFrame — no PlaceObject3
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&file[8..]).unwrap();
        let body = encoder.finish().unwrap();
        let mut cws = file[0..8].to_vec();
        cws[0..3].copy_from_slice(b"CWS");
        cws.extend_from_slice(&body);

        let normalized =
            normalize_scaleform_dialect(&cws).expect("a CWS decode must be handed back (#5479)");
        assert_eq!(&normalized[0..3], b"FWS");
        assert_eq!(
            u32::from_le_bytes(normalized[4..8].try_into().unwrap()) as usize,
            normalized.len()
        );
        assert_eq!(
            &normalized[8..],
            &file[8..],
            "the uncompressed body must be byte-identical — the shim's \
             decode feeds the prepare path unchanged (#5479)"
        );
        // And the reused decode parses to the same movie the original
        // would have produced.
        let decompressed = swf::decompress_swf(normalized.as_slice()).expect("copy-path decode");
        let movie = swf::parse_swf(&decompressed).expect("parse");
        assert_eq!(movie.tags.len(), 1, "ShowFrame (parse_swf consumes End)");
    }

    /// #5456 — the walker recursed once per nested `DefineSprite` with no
    /// cap, ~10 bytes of input per stack frame: a crafted movie aborted the
    /// process with a stack overflow before Ruffle ever saw the bytes. Nested
    /// sprites are illegal SWF, so the walk stops descending at depth 1.
    #[test]
    fn deeply_nested_sprites_do_not_overflow_the_stack() {
        // Built without per-level copies, with LONG-form headers (a short
        // header's 6-bit length cannot describe these bodies — the walker
        // would stop at the first level and the test would prove nothing).
        const LEVELS: usize = 100_000;
        let innermost = tag(70, &starfield_place3());
        // Total size (header included) of the sprite at each level,
        // innermost-out; the outermost is last.
        let mut total = innermost.len();
        let mut totals = Vec::with_capacity(LEVELS);
        for _ in 0..LEVELS {
            let body_len = 4 + total + 2; // id + frames, inner, sprite END
            total = 6 + body_len;
            totals.push(total);
        }
        let mut inner = Vec::with_capacity(total);
        for level_total in totals.iter().rev() {
            let body_len = (level_total - 6) as u32;
            inner.extend_from_slice(&((39u16 << 6) | 0x3F).to_le_bytes());
            inner.extend_from_slice(&body_len.to_le_bytes());
            inner.extend_from_slice(&7u16.to_le_bytes()); // character id
            inner.extend_from_slice(&1u16.to_le_bytes()); // frame count
        }
        inner.extend_from_slice(&innermost);
        for _ in 0..LEVELS {
            inner.extend_from_slice(&[0x00, 0x00]); // sprite END
        }
        assert_eq!(inner.len(), total);
        let file = swf_with_tags(&inner);
        // Must return (not abort). The record is buried far below the cap,
        // so nothing is patched and the movie is handed back untouched.
        assert!(normalize_scaleform_dialect(&file).is_none());
    }

    /// #5456 — the cap leaves exactly the legal shape alone: a PlaceObject3
    /// directly in a top-level sprite (depth 1) is still patched, as in
    /// the retail Starfield hudmenu.
    #[test]
    fn one_level_of_sprite_nesting_is_still_patched() {
        let file = swf_with_tags(&sprite(&tag(70, &starfield_place3())));
        assert!(normalize_scaleform_dialect(&file).is_some());
        // A sprite inside that sprite (illegal SWF) is not walked.
        let illegal = swf_with_tags(&sprite(&sprite(&tag(70, &starfield_place3()))));
        assert!(normalize_scaleform_dialect(&illegal).is_none());
    }

    /// #5456 — the inflate is bounded by the header's declared length: a
    /// body that overruns its own header is declined (stock loader's
    /// problem, as before the shim), not read to the end.
    #[test]
    fn cws_body_longer_than_its_declared_length_is_declined() {
        use std::io::Write;
        let file = swf_with_tags(&sprite(&tag(70, &starfield_place3())));
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&file[8..]).unwrap();
        encoder.write_all(&[0u8; 4096]).unwrap(); // trailing bytes past the claim
        let body = encoder.finish().unwrap();
        let mut cws = file[0..8].to_vec(); // header still declares file.len()
        cws[0..3].copy_from_slice(b"CWS");
        cws.extend_from_slice(&body);
        assert!(normalize_scaleform_dialect(&cws).is_none());
    }

    /// #5719 — a header may claim any `u32`; the claim must not size the
    /// inflate. A 4 MiB zero stream (a few KiB compressed) behind a
    /// `u32::MAX` header is declined by the ceiling without the body ever
    /// being inflated past it.
    #[test]
    fn cws_header_claiming_u32_max_over_a_bomb_is_declined() {
        use std::io::Write;
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        encoder.write_all(&vec![0u8; 4 * 1024 * 1024]).unwrap();
        let body = encoder.finish().unwrap();
        assert!(body.len() < 16 * 1024, "fixture: the bomb is tiny on disk");
        let mut cws = b"CWS\x0f".to_vec();
        cws.extend_from_slice(&u32::MAX.to_le_bytes());
        cws.extend_from_slice(&body);
        // Production ceiling: the claim itself is far past it.
        assert!(decompress_zlib_after_header(&cws).is_none());
        // A ceiling under the bomb's real size with an in-range claim: the
        // declared-length bound still holds.
        let mut under_claim = cws.clone();
        under_claim[4..8].copy_from_slice(&(1024u32 + 8).to_le_bytes());
        assert!(decompress_zlib_after_header_capped(&under_claim, 1 << 20).is_none());
    }

    /// #5719 — the ceiling sits above every shipped movie (largest measured
    /// declared length: 65,902,267 B, FO76 `fonts_zhhant.swf`) and well
    /// under the 4 GiB a `u32` header could claim.
    #[test]
    fn swf_body_ceiling_clears_the_corpus_and_stops_short_of_u32() {
        const _: () = {
            assert!(MAX_SWF_BODY_BYTES > 65_902_267);
            assert!(MAX_SWF_BODY_BYTES < (u32::MAX as u64) / 8);
        };
    }

    /// A declared length at the ceiling still inflates normally.
    #[test]
    fn cws_within_the_ceiling_inflates() {
        use std::io::Write;
        let file = swf_with_tags(&sprite(&tag(70, &starfield_place3())));
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&file[8..]).unwrap();
        let mut cws = file[0..8].to_vec();
        cws[0..3].copy_from_slice(b"CWS");
        cws.extend_from_slice(&encoder.finish().unwrap());
        let out = decompress_zlib_after_header_capped(&cws, file.len() as u64).expect("at ceiling");
        assert_eq!(&out[8..], &file[8..]);
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
        // The patch is visible in the re-emitted stream.
        assert!(normalized.windows(2).any(|w| w == 0x0825u16.to_le_bytes()));
    }

    /// #5966 — the ceiling gates the WHOLE load path. Pre-fix the shim
    /// declined the hostile claim and `prepare_movie` handed the original
    /// bytes straight to `swf::decompress_swf`, which inflates to the
    /// header's own `u32`. The error must name the ceiling, not a downstream
    /// "Failed to parse SWF" from a decode that should never have run.
    #[test]
    fn prepare_movie_refuses_an_over_ceiling_claim_before_any_decode() {
        let Err(error) = prepare_movie(&hostile_cws_header_bomb(), None, None) else {
            panic!("a u32::MAX header over a zlib bomb must be refused");
        };
        assert!(error.contains("ceiling"), "{error}");
        // The ceiling is container-agnostic: ZWS goes to the stock LZMA
        // reader and FWS claims are just as attacker-controlled.
        for signature in [b"ZWS", b"FWS"] {
            let mut movie = hostile_cws_header_bomb();
            movie[0..3].copy_from_slice(signature);
            let Err(error) = prepare_movie(&movie, None, None) else {
                panic!(
                    "{} header claiming u32::MAX must be refused",
                    signature_name(signature)
                );
            };
            assert!(error.contains("ceiling"), "{error}");
        }
    }

    fn signature_name(signature: &[u8; 3]) -> &str {
        std::str::from_utf8(signature).unwrap()
    }

    /// The same refusal from every other decode entry point in the crate.
    #[test]
    fn every_public_decode_entry_refuses_an_over_ceiling_claim() {
        let bomb = hostile_cws_header_bomb();
        let error = ScaleformProfile::detect(&bomb).unwrap_err().to_string();
        assert!(error.contains("ceiling"), "detect: {error}");
        let error = crate::avm1_host::referenced_host_methods(&bomb).unwrap_err();
        assert!(error.contains("ceiling"), "AVM1 scanner: {error}");
        let error = crate::avm2_host::referenced_host_methods(&bomb).unwrap_err();
        assert!(error.contains("ceiling"), "AVM2 scanner: {error}");
    }

    /// Boundary of the shared pre-check: the claim is the body length plus
    /// the 8-byte header, a claim AT the ceiling passes, one byte over is
    /// refused, and input too short to carry a header or not a SWF at all is
    /// left to the decoder to reject.
    #[test]
    fn the_declared_length_ceiling_boundary() {
        let claim = |signature: &[u8; 3], total: u32| {
            let mut movie = signature.to_vec();
            movie.push(15);
            movie.extend_from_slice(&total.to_le_bytes());
            movie
        };
        let ceiling = 1000u64;
        assert!(ensure_declared_len_within(&claim(b"CWS", 1008), ceiling).is_ok());
        assert!(ensure_declared_len_within(&claim(b"CWS", 1009), ceiling).is_err());
        assert!(ensure_declared_len_within(&claim(b"ZWS", 1009), ceiling).is_err());
        assert!(ensure_declared_len_within(&claim(b"FWS", 1009), ceiling).is_err());
        // A length below the header size saturates to a zero body.
        assert!(ensure_declared_len_within(&claim(b"CWS", 3), ceiling).is_ok());
        // Too short for a header, or not a SWF: nothing to gate.
        assert!(ensure_declared_len_within(b"CWS", ceiling).is_ok());
        assert!(ensure_declared_len_within(b"CWS\x0f\xff\xff", ceiling).is_ok());
        assert!(ensure_declared_len_within(b"NOTASWF\x00\xff\xff\xff\xff", ceiling).is_ok());
        // The real constant clears every shipped movie measured on this
        // host (largest: 17,139,629 B) and stops far short of u32.
        assert!(ensure_declared_len_within_ceiling(&claim(b"CWS", 17_139_629)).is_ok());
        assert!(ensure_declared_len_within_ceiling(&claim(b"CWS", u32::MAX)).is_err());
    }

    /// Source-shape guard (#5966): every production `decompress_swf(` call in
    /// this crate sits behind the declared-length pre-check, and every
    /// production `SwfMovieData::from_data(` in `player.rs` is fed by
    /// `prepare_movie`. A new decode site that skips the pre-check re-opens
    /// the hole #5719 only half closed. Counting is per file over production
    /// code (the strong `#[cfg(test)] mod` cut, comment lines ignored).
    #[test]
    fn every_decode_site_is_behind_the_declared_length_ceiling() {
        fn production_code_lines(source: &str) -> Vec<&str> {
            let production = source
                .split_once("\n#[cfg(test)]\nmod ")
                .map_or(source, |(production, _)| production);
            production
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .collect()
        }
        fn count(lines: &[&str], needle: &str) -> usize {
            lines.iter().filter(|line| line.contains(needle)).count()
        }

        fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("read crates/ui/src") {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    if path.file_name().is_some_and(|name| name != "tests") {
                        rust_files(&path, out);
                    }
                } else if path.extension().is_some_and(|ext| ext == "rs")
                    && !path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name == "tests.rs" || name.ends_with("_tests.rs"))
                {
                    out.push(path);
                }
            }
        }
        let mut files = Vec::new();
        rust_files(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        assert!(
            files.len() >= 8,
            "found only {} source files — wrong directory?",
            files.len()
        );

        let mut decode_sites = 0;
        for path in &files {
            let source = std::fs::read_to_string(path).unwrap();
            let lines = production_code_lines(&source);
            let decodes = count(&lines, "decompress_swf(");
            // The definition line mentions the name too; only calls count.
            let guards = lines
                .iter()
                .filter(|line| {
                    line.contains("ensure_declared_len_within_ceiling(")
                        && !line.contains("fn ensure_declared_len_within_ceiling")
                })
                .count();
            decode_sites += decodes;
            // The shim's own `decompress_zlib_after_header_counting` carries
            // the ceiling inline, and the definition of the pre-check itself
            // is not a call site: only files that call the stock reader are
            // held to the rule.
            assert!(
                guards >= decodes,
                "{}: {decodes} `decompress_swf(` call(s) but only {guards} \
                 `ensure_declared_len_within_ceiling(` call(s) — an unguarded decode site (#5966)",
                path.display()
            );
        }
        assert!(
            decode_sites >= 6,
            "expected at least the 6 known stock-decode sites, found {decode_sites}: the \
             scan went blind"
        );

        let player = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/player.rs"),
        )
        .unwrap();
        let lines = production_code_lines(&player);
        let from_data = count(&lines, "SwfMovieData::from_data(");
        let prepared = count(&lines, "prepare_movie(");
        assert!(
            from_data >= 1,
            "player.rs lost its SwfMovieData::from_data sites"
        );
        assert_eq!(
            from_data, prepared,
            "every SwfMovieData::from_data in player.rs must be fed by a prepare_movie \
             call, which owns the ceiling (#5966)"
        );
    }
}
