//! FormID → archive-relative audio path resolution, and the sound provider:
//! the sound archives plus the loose `Data` tier, which the music controller
//! (`byroredux/src/music`, `docs/engine/music.md`) resolves every music path
//! through, loose first (Game Music MUS-SRC-011, MUS-SRC-013).
//!
//! EX-16 item 1 (#2372) named the FormID→path gap as the missing half of
//! REGN ambient-sound consumption: `RegionDataKind::Sound` carries a
//! `sound_form: u32` pointing at a `SOUN` record, but until
//! `soun::parse_soun` there was no decoded file path to resolve it to.
//! [`resolve_sound_path`]/[`sound_archive_path`] are the pure
//! lookup/normalization pair (mirroring `script::pex_archive_path`'s shape for
//! Papyrus `.pex` names). REGN background music is the music controller's
//! now: the region path that played a `SOUN` from `--sounds-bsa` is gone.
//!
//! Deliberately still NOT covered: the `incidental`/`sounds` ambient-loop
//! fields, or a spatial REGN-keyed `AudioEmitter` (#3301).

use super::*;
use byroredux_core::ecs::Resource;
use byroredux_plugin::esm::records::SounRecord;
use std::collections::HashMap;

/// Look up a `SOUN` FormID's decoded `FNAM` path, verbatim. Returns `None`
/// when the FormID isn't a known SOUN (bad data) or the record omitted
/// `FNAM` (rare placeholder records — see `soun::parse_soun`'s doc).
///
/// #3914 — the path is a **file or a folder** (`SounRecord::is_folder`;
/// 50.8 % of FNV's `SOUN` records author a folder of variants). Callers
/// must check [`sound_is_folder`] before handing the result to
/// [`sound_archive_path`] + [`SoundArchiveProvider::extract`]: a folder is
/// never an archive entry, so extracting it is a guaranteed miss that
/// looks exactly like missing content.
pub(crate) fn resolve_sound_path(sounds: &HashMap<u32, SounRecord>, form_id: u32) -> Option<&str> {
    sounds
        .get(&form_id)
        .map(|s| s.sound_path.as_str())
        .filter(|p| !p.is_empty())
}

/// Whether a `SOUN` FormID's `FNAM` names a folder of variant files rather
/// than one file (#3914; see [`SounRecord::is_folder`]). `false` for an
/// unresolved FormID, same fail-closed posture as [`resolve_sound_path`] —
/// kept as its own pure lookup, mirroring [`resolve_sound_path`].
pub(crate) fn sound_is_folder(sounds: &HashMap<u32, SounRecord>, form_id: u32) -> bool {
    sounds.get(&form_id).is_some_and(|s| s.is_folder())
}

/// Normalise a `SOUN.FNAM` value to its archive form: lowercase,
/// backslash-separated, under the `sound\` folder. `FNAM` is authored
/// relative to `Data\Sound\` without that prefix, the way `MODL` is
/// relative to `Meshes\` and `ICON` to `Textures\` — but unlike those,
/// `FNAM` is not always a file (#3914): a folder-form value keeps its
/// trailing separator here (`fx\amb\ceilingcrumble\` →
/// `sound\fx\amb\ceilingcrumble\`), which is an entry *prefix*, never an
/// entry key — only a file-form result is valid input to
/// [`SoundArchiveProvider::extract`]. A path that already carries the
/// folder, or uses forward slashes, is accepted unchanged in meaning.
/// Mirrors `script::pex_archive_path`.
pub(crate) fn sound_archive_path(sound_path: &str) -> String {
    let mut path = sound_path.replace('/', "\\").to_ascii_lowercase();
    // Some records author the path from the game root (`Data\Sound\FX\OBJ\PullChain\…`,
    // Skyrim.esm's pull-chain descriptors); the archive key is `sound\fx\…`.
    if let Some(rest) = path.strip_prefix("data\\") {
        path = rest.to_owned();
    }
    if !path.starts_with("sound\\") {
        path = format!("sound\\{path}");
    }
    path
}

/// Searches game archives for sound files by archive-relative path.
/// `--sounds-bsa` is repeatable — LAST-listed wins (#3917 flipped the
/// lookup from #3776's first-wins; list override/mod archives after the
/// vanilla one), the opposite precedence of `ScriptProvider`. An
/// empty provider (flag absent) makes every lookup a clean miss so callers
/// fall through to "nothing to play" the same way an unregistered SOUN
/// would.
///
/// This is a different animal from the ad hoc `Archive::open` calls in
/// `try_load_default_footstep`/`try_load_default_water_splash`: those each
/// resolve exactly one hardcoded canonical path once at boot, so reopening
/// the archive on the spot is fine. REGN ambient dispatch resolves an
/// arbitrary FormID-driven path on every cell/tile change for the whole
/// engine session, which needs a persistent handle — the same reason
/// `ScriptProvider` exists instead of every VMAD attach reopening
/// `--scripts-bsa` from scratch.
pub(crate) struct SoundArchiveProvider {
    archives: Vec<Archive>,
    /// The loose `Data` tier (#6200's shared index, which includes the
    /// `music` root): it wins over every archive, as loose files do for
    /// every other asset (Game Music MUS-SRC-013).
    loose: Option<Archive>,
}

impl SoundArchiveProvider {
    pub(crate) fn new() -> Self {
        Self {
            archives: Vec::new(),
            loose: None,
        }
    }

    /// True when no sound archive was supplied — dispatch can skip the
    /// extract attempt entirely.
    pub(crate) fn is_empty(&self) -> bool {
        self.archives.is_empty()
    }

    /// Extract raw bytes for an archive-relative **file** path (as produced
    /// by [`sound_archive_path`] from a file-form `FNAM`).
    ///
    /// **Precedence: last-listed archive wins**, matching Bethesda load order
    /// and the mesh / texture / material pools #3637 (`3562401b`) inverted.
    /// Sound archives are plain content archives like those three, so they
    /// follow the same rule; `ScriptProvider` is first-wins deliberately and
    /// for a different reason (`--scripts-bsa` carries a mod principal and
    /// override archives are listed first — #1743 / SCR-D7-03), which is why
    /// that one says so in its own doc.
    ///
    /// #3917 — this provider was missed by #3637's sweep and stayed
    /// first-wins. That was inert while the FNV profile listed one sound
    /// archive, but `[profiles.fnv]` now lists
    /// `["Fallout - Sound.bsa", "Update.bsa"]` with a comment stating the
    /// patch goes last "under last-wins" — which this function was not
    /// honouring. Measured on the shipped archives: the two overlap on
    /// exactly one key, `sound\fx\wpn\minigun\wpn_minigun_spin_lpm.wav`,
    /// and the copies differ (267 774 B in `Fallout - Sound.bsa` vs 210 534 B
    /// in `Update.bsa`), so first-wins really was serving the unpatched
    /// audio. `--game skyrim_se` supplies two sound archives as well.
    ///
    /// A folder-form path (#3914) is never an entry key — it always misses
    /// here, so callers gate on [`sound_is_folder`] first rather than reading
    /// that miss as missing content. Variant selection inside a folder is a
    /// policy decision no consumer has made yet, so there is deliberately no
    /// `extract_any_in(folder)` sibling.
    pub(crate) fn extract(&self, archive_path: &str) -> Option<Vec<u8>> {
        extract_first(self.loose.iter().chain(self.archives.iter().rev()), archive_path)
    }

    /// Every entry under `music\` the loose tier and the archives hold, lower-cased and
    /// backslash-separated: what a music path or folder form resolves against
    /// (`docs/engine/music.md` §1).
    pub(crate) fn music_keys(&self) -> std::collections::BTreeSet<String> {
        self.loose
            .iter()
            .chain(&self.archives)
            .flat_map(|archive| archive.all_paths())
            .map(|path| path.replace('/', "\\").to_ascii_lowercase())
            .filter(|path| path.starts_with("music\\"))
            .collect()
    }
}

impl Resource for SoundArchiveProvider {}

/// Build a [`SoundArchiveProvider`] from CLI arguments. Accepts repeated
/// `--sounds-bsa <path>` flags (the same flag `try_load_default_footstep`/
/// `try_load_default_water_splash` already consume for their one-off
/// canonical-path loads — opening the archives here again is cheap:
/// `BsaArchive`/`Ba2Archive::open` reads only the file table, not the
/// payload). Silently returns an empty provider when no flag is present.
/// With an `--esm`, the loose tier of its `Data` folder is added on top
/// (MUS-SRC-011, -013); [`SoundArchiveProvider::is_empty`] still answers for
/// the archives alone.
pub(crate) fn build_sound_archive_provider(args: &[String]) -> SoundArchiveProvider {
    let mut provider = SoundArchiveProvider::new();
    provider.loose = super::archive::loose_data_dir(args).and_then(|data| Archive::open_loose(&data));
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--sounds-bsa" {
            if let Some(path) = args.get(i + 1) {
                match Archive::open(path) {
                    Ok(a) => {
                        log::info!("Opened sound archive: '{path}'");
                        provider.archives.push(a);
                    }
                    Err(e) => log::warn!("Failed to open sound archive '{path}': {e}"),
                }
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    provider
}

#[cfg(test)]
mod tests {
    use super::*;


    fn soun(form_id: u32, path: &str) -> SounRecord {
        SounRecord {
            form_id,
            editor_id: String::new(),
            sound_path: path.to_string(),
            looping: false,
            variants: Vec::new(),
            ..Default::default()
        }
    }

    #[test]
    fn resolve_sound_path_finds_known_form_id() {
        let mut sounds = HashMap::new();
        sounds.insert(0x1234, soun(0x1234, "fx\\explosion01.wav"));
        assert_eq!(
            resolve_sound_path(&sounds, 0x1234),
            Some("fx\\explosion01.wav")
        );
    }

    #[test]
    fn resolve_sound_path_missing_form_id_returns_none() {
        let sounds = HashMap::new();
        assert_eq!(resolve_sound_path(&sounds, 0xDEAD_BEEF), None);
    }

    #[test]
    fn resolve_sound_path_empty_fnam_returns_none() {
        // Placeholder SOUN with no FNAM decodes to an empty path — treat
        // it the same as "not found" rather than handing callers an
        // archive lookup that can never hit.
        let mut sounds = HashMap::new();
        sounds.insert(0x5, soun(0x5, ""));
        assert_eq!(resolve_sound_path(&sounds, 0x5), None);
    }




    /// #3914 — the folder form is carried through the FormID lookup
    /// unchanged, and the sibling predicate reports it, so a consumer can
    /// branch before it ever reaches `extract`.
    #[test]
    fn sound_is_folder_true_for_a_folder_form_record() {
        let mut sounds = HashMap::new();
        sounds.insert(0x77, soun(0x77, "fx\\amb\\ceilingcrumble\\"));
        assert!(sound_is_folder(&sounds, 0x77));
        assert_eq!(
            resolve_sound_path(&sounds, 0x77),
            Some("fx\\amb\\ceilingcrumble\\"),
            "the folder path itself still resolves — only its kind differs"
        );
    }

    #[test]
    fn sound_is_folder_false_for_a_file_form_record_and_unknown_form_id() {
        let mut sounds = HashMap::new();
        sounds.insert(0x1234, soun(0x1234, "fx\\explosion01.wav"));
        assert!(!sound_is_folder(&sounds, 0x1234));
        assert!(!sound_is_folder(&sounds, 0xDEAD_BEEF));
    }

    /// Skyrim.esm authors the pull-chain descriptors from the game root
    /// (`Data\Sound\FX\OBJ\PullChain\…`); the archive key has neither prefix.
    #[test]
    fn sound_archive_path_drops_a_game_root_prefix() {
        assert_eq!(
            sound_archive_path("Data\\Sound\\FX\\OBJ\\PullChain\\OBJ_PullChain_Down_01.wav"),
            "sound\\fx\\obj\\pullchain\\obj_pullchain_down_01.wav"
        );
        assert_eq!(sound_archive_path("fx\\a.wav"), "sound\\fx\\a.wav");
    }

    /// #3914 — a folder-form value keeps its trailing separator through
    /// normalisation: the result is an entry prefix, never an entry key.
    #[test]
    fn sound_archive_path_keeps_folder_form_trailing_separator() {
        assert_eq!(
            sound_archive_path("FX\\Amb\\CeilingCrumble\\"),
            "sound\\fx\\amb\\ceilingcrumble\\"
        );
        assert_eq!(
            sound_archive_path("fx/amb/ceilingcrumble/"),
            "sound\\fx\\amb\\ceilingcrumble\\"
        );
    }

    #[test]
    fn sound_archive_path_prepends_folder_and_lowercases() {
        assert_eq!(
            sound_archive_path("FX\\Explosion01.wav"),
            "sound\\fx\\explosion01.wav"
        );
    }

    #[test]
    fn sound_archive_path_normalizes_forward_slashes() {
        assert_eq!(
            sound_archive_path("amb/wind_loop.wav"),
            "sound\\amb\\wind_loop.wav"
        );
    }

    #[test]
    fn sound_archive_path_does_not_double_prefix() {
        // Defensive: a path already authored with the `sound\` folder
        // (non-vanilla authoring) must not become `sound\sound\...`.
        assert_eq!(
            sound_archive_path("sound\\fx\\explosion01.wav"),
            "sound\\fx\\explosion01.wav"
        );
    }

    #[test]
    fn sound_archive_provider_starts_empty() {
        let provider = SoundArchiveProvider::new();
        assert!(provider.is_empty());
        assert!(provider.extract("sound\\fx\\explosion01.wav").is_none());
    }

    #[test]
    fn build_sound_archive_provider_without_flag_is_empty() {
        let provider = build_sound_archive_provider(&["byroredux".to_string()]);
        assert!(provider.is_empty());
    }

    #[test]
    fn build_sound_archive_provider_with_unopenable_path_stays_empty() {
        // A bad/missing path must warn and skip, not panic the whole boot.
        let provider = build_sound_archive_provider(&[
            "--sounds-bsa".to_string(),
            "/nonexistent/path/does-not-exist.bsa".to_string(),
        ]);
        assert!(provider.is_empty());
    }
}
