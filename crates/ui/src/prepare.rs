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
