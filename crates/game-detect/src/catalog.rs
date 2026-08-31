//! The appid ↔ profile-key table.
//!
//! Every row was read off a real `appmanifest_<appid>.acf` on a machine with
//! the title installed, not from memory — the `installdir` values in
//! particular are load-bearing (they are what the detected data directory is
//! built from) and are not always what the store page calls the game
//! (`Fallout 3 goty`, not `Fallout 3`).
//!
//! The shipped `assets/debug_profiles.toml` gives each profile a `subdir` of
//! exactly `<installdir>/Data`; `catalog_matches_shipped_profiles` in the
//! crate's tests pins that correspondence so a profile edit cannot silently
//! desynchronise detection.

/// One Steam-distributed title the engine has a profile for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SteamApp {
    /// Steam application id — the `<appid>` in `appmanifest_<appid>.acf`.
    pub appid: u32,
    /// `GameProfileEntry` key in the profile registry.
    pub profile: &'static str,
    /// Steam's `installdir`, i.e. the folder under `steamapps/common/`.
    pub install_dir: &'static str,
}

/// Every title detection knows about, in engine-support order.
pub const STEAM_APPS: &[SteamApp] = &[
    SteamApp {
        appid: 22330,
        profile: "oblivion",
        install_dir: "Oblivion",
    },
    SteamApp {
        appid: 22370,
        profile: "fo3",
        install_dir: "Fallout 3 goty",
    },
    SteamApp {
        appid: 22380,
        profile: "fnv",
        install_dir: "Fallout New Vegas",
    },
    SteamApp {
        appid: 489830,
        profile: "skyrim_se",
        install_dir: "Skyrim Special Edition",
    },
    SteamApp {
        appid: 377160,
        profile: "fo4",
        install_dir: "Fallout 4",
    },
    SteamApp {
        appid: 1716740,
        profile: "starfield",
        install_dir: "Starfield",
    },
];

/// The title with this appid, if the engine has a profile for it.
pub fn by_appid(appid: u32) -> Option<&'static SteamApp> {
    STEAM_APPS.iter().find(|app| app.appid == appid)
}

/// The title backing this profile key.
pub fn by_profile(profile: &str) -> Option<&'static SteamApp> {
    STEAM_APPS.iter().find(|app| app.profile == profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appids_and_profile_keys_are_both_unique() {
        for (index, app) in STEAM_APPS.iter().enumerate() {
            for other in &STEAM_APPS[index + 1..] {
                assert_ne!(app.appid, other.appid, "duplicate appid {}", app.appid);
                assert_ne!(app.profile, other.profile, "duplicate key {}", app.profile);
            }
        }
    }

    #[test]
    fn lookups_agree_in_both_directions() {
        for app in STEAM_APPS {
            assert_eq!(by_appid(app.appid), Some(app));
            assert_eq!(by_profile(app.profile), Some(app));
        }
        assert!(by_appid(1).is_none());
        assert!(by_profile("morrowind").is_none());
    }
}
