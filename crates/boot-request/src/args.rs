//! Intent → argv translation.
//!
//! This is the only place that knows engine flag names, and it is deliberately
//! one-way: the launcher never parses argv, and nothing downstream of the
//! expansion seam learns that a launcher exists.
//!
//! **Precedence.** Generated flags are *appended after* the user's own argv, so
//! the engine's established first-occurrence-wins rule leaves an explicit
//! command-line flag in control of its request-file counterpart. Where
//! appending would be actively confusing rather than merely inert — a second
//! `--cell` when the user already named one — the generated flag is suppressed
//! and a note explains why. This mirrors what `expand_game_profile_args`
//! already does with its own `has_location` check.

use std::path::Path;

use crate::{Action, BootRequest};

/// The generated flags plus human-readable notes about anything suppressed or
/// unresolvable. Callers print the notes; nothing here logs on its own, so the
/// translation stays a pure function and is testable without a logger.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expansion {
    pub args: Vec<String>,
    pub notes: Vec<String>,
}

/// Location flags that select where to load. Any one of these on the command
/// line suppresses the request's own action. Matches the `has_location` set in
/// `expand_game_profile_args`.
const LOCATION_FLAGS: [&str; 3] = ["--cell", "--grid", "--wrld"];

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn has_any_location(args: &[String]) -> bool {
    LOCATION_FLAGS.iter().any(|flag| has_flag(args, flag))
}

impl BootRequest {
    /// Translate this request into engine argv, given the argv the user
    /// actually typed.
    ///
    /// `existing` is read only to decide what to suppress; it is never
    /// modified or reordered. The returned args are meant to be appended to it.
    pub fn to_args(&self, existing: &[String]) -> Expansion {
        let mut out = Expansion::default();
        self.push_content_source(existing, &mut out);
        self.push_action(existing, &mut out);
        out
    }

    /// `--game <key>`, or the explicit `--esm` / archive set. See [`crate::GameSpec`].
    fn push_content_source(&self, existing: &[String], out: &mut Expansion) {
        if self.is_self_contained() {
            self.push_self_contained_source(existing, out);
        } else {
            self.push_profile_source(existing, out);
        }
    }

    fn push_profile_source(&self, existing: &[String], out: &mut Expansion) {
        let profile = self.game.profile.trim();
        if profile.is_empty() {
            out.notes.push(
                "boot request names neither a profile nor a data dir; the engine will fall back \
                 to its own launch defaults"
                    .to_owned(),
            );
            return;
        }
        if has_flag(existing, "--game") {
            out.notes.push(format!(
                "boot request profile {profile:?} suppressed: --game on the command line wins"
            ));
            return;
        }
        out.args.push("--game".to_owned());
        out.args.push(profile.to_owned());
    }

    fn push_self_contained_source(&self, existing: &[String], out: &mut Expansion) {
        if has_flag(existing, "--esm") {
            out.notes.push(
                "boot request data dir suppressed: --esm on the command line wins".to_owned(),
            );
            return;
        }

        let data_dir = Path::new(self.game.data_dir.trim());
        let join =
            |name: &str| -> String { data_dir.join(name.trim()).to_string_lossy().into_owned() };

        // Masters precede the main plugin: the cell loader composes the load
        // order as `[masters…, esm]` and both are opened as paths, so each is
        // joined against the data dir rather than left relative to the CWD.
        for master in self.game.masters.iter().filter(|m| !m.trim().is_empty()) {
            out.args.push("--master".to_owned());
            out.args.push(join(master));
        }

        if self.game.esm.trim().is_empty() {
            out.notes.push(format!(
                "boot request sets data_dir {} but no esm; no plugin will be loaded",
                data_dir.display()
            ));
        } else {
            out.args.push("--esm".to_owned());
            out.args.push(join(&self.game.esm));
        }

        let archives = &self.game.archives;
        for (flag, names) in [
            ("--bsa", &archives.meshes),
            ("--textures-bsa", &archives.textures),
            ("--scripts-bsa", &archives.scripts),
            ("--sounds-bsa", &archives.sounds),
            ("--materials-ba2", &archives.materials),
        ] {
            for name in names.iter().filter(|n| !n.trim().is_empty()) {
                out.args.push(flag.to_owned());
                out.args.push(join(name));
            }
        }
    }

    fn push_action(&self, existing: &[String], out: &mut Expansion) {
        let Some(action) = &self.action else {
            return;
        };
        match action {
            Action::NewGame => {
                if has_any_location(existing) {
                    out.notes.push(
                        "boot request action new_game suppressed: an explicit location flag wins"
                            .to_owned(),
                    );
                    return;
                }
                if self.is_self_contained() {
                    // The new-game placement lives in the profile registry, which
                    // a self-contained request bypasses by construction. Say so
                    // rather than emitting a flag that will quietly do nothing.
                    out.notes.push(
                        "boot request action new_game has no target in a self-contained request: \
                         the launcher should resolve it to an explicit grid"
                            .to_owned(),
                    );
                }
                out.args.push("--new-game".to_owned());
            }
            Action::Continue { slot } => {
                if has_flag(existing, "--load") {
                    out.notes.push(format!(
                        "boot request action continue (slot {slot}) suppressed: \
                         --load on the command line wins"
                    ));
                    return;
                }
                out.args.push("--load".to_owned());
                out.args.push(slot.to_string());
            }
            Action::Cell { edid } => {
                if has_any_location(existing) {
                    out.notes.push(format!(
                        "boot request action cell {edid:?} suppressed: \
                         an explicit location flag wins"
                    ));
                    return;
                }
                out.args.push("--cell".to_owned());
                out.args.push(edid.clone());
            }
            Action::Grid {
                worldspace,
                x,
                y,
                radius,
            } => {
                if has_any_location(existing) {
                    out.notes.push(format!(
                        "boot request action grid {worldspace:?} ({x},{y}) suppressed: \
                         an explicit location flag wins"
                    ));
                    return;
                }
                out.args.push("--wrld".to_owned());
                out.args.push(worldspace.clone());
                out.args.push("--grid".to_owned());
                out.args.push(format!("{x},{y}"));
                if let Some(radius) = radius {
                    if has_flag(existing, "--radius") {
                        out.notes.push(format!(
                            "boot request radius {radius} suppressed: \
                             --radius on the command line wins"
                        ));
                    } else {
                        out.args.push("--radius".to_owned());
                        out.args.push(radius.to_string());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Archives, GameSpec};

    fn argv(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).to_owned()).collect()
    }

    fn self_contained() -> BootRequest {
        BootRequest {
            game: GameSpec {
                profile: "skyrim_se".into(),
                data_dir: "/games/SkyrimSE/Data".into(),
                esm: "Dawnguard.esm".into(),
                masters: vec!["Skyrim.esm".into(), "Update.esm".into()],
                archives: Archives {
                    meshes: vec!["Skyrim - Meshes0.bsa".into()],
                    textures: vec!["Skyrim - Textures0.bsa".into()],
                    scripts: vec!["Skyrim - Misc.bsa".into()],
                    sounds: vec![],
                    materials: vec![],
                },
            },
            ..BootRequest::default()
        }
    }

    /// The common case: a profile key expands to `--game`, and nothing more —
    /// archive fan-out stays the profile expander's job, so there is exactly
    /// one implementation of "which archives does Skyrim need".
    #[test]
    fn a_profile_request_defers_archive_resolution_to_the_profile_expander() {
        let request = BootRequest::for_profile("skyrim_se").with_action(Action::Cell {
            edid: "WhiterunBanneredMare".into(),
        });
        let expansion = request.to_args(&argv(&["byroredux"]));
        assert_eq!(
            expansion.args,
            argv(&["--game", "skyrim_se", "--cell", "WhiterunBanneredMare"])
        );
        assert!(expansion.notes.is_empty(), "{:?}", expansion.notes);
    }

    /// Masters come first and every path is joined against the data dir — the
    /// cell loader opens masters as paths, so leaving them bare would only work
    /// when the CWD happened to be the data dir.
    #[test]
    fn a_self_contained_request_emits_joined_paths_with_masters_first() {
        let expansion = self_contained().to_args(&argv(&["byroredux"]));
        assert_eq!(
            expansion.args,
            argv(&[
                "--master",
                "/games/SkyrimSE/Data/Skyrim.esm",
                "--master",
                "/games/SkyrimSE/Data/Update.esm",
                "--esm",
                "/games/SkyrimSE/Data/Dawnguard.esm",
                "--bsa",
                "/games/SkyrimSE/Data/Skyrim - Meshes0.bsa",
                "--textures-bsa",
                "/games/SkyrimSE/Data/Skyrim - Textures0.bsa",
                "--scripts-bsa",
                "/games/SkyrimSE/Data/Skyrim - Misc.bsa",
            ])
        );
        // Never `--game`: a self-contained request exists precisely because the
        // registry's answer was wrong, so re-entering the registry would undo it.
        assert!(!expansion.args.iter().any(|a| a == "--game"));
    }

    #[test]
    fn each_action_maps_to_its_flags() {
        let cases = [
            (Action::NewGame, argv(&["--new-game"])),
            (Action::Continue { slot: 3 }, argv(&["--load", "3"])),
            (
                Action::Cell {
                    edid: "Vault101a".into(),
                },
                argv(&["--cell", "Vault101a"]),
            ),
            (
                Action::Grid {
                    worldspace: "Tamriel".into(),
                    x: 5,
                    y: -24,
                    radius: Some(1),
                },
                argv(&["--wrld", "Tamriel", "--grid", "5,-24", "--radius", "1"]),
            ),
            (
                Action::Grid {
                    worldspace: "WastelandNV".into(),
                    x: 0,
                    y: 0,
                    radius: None,
                },
                argv(&["--wrld", "WastelandNV", "--grid", "0,0"]),
            ),
        ];
        for (action, expected) in cases {
            let request = BootRequest::for_profile("fnv").with_action(action.clone());
            let expansion = request.to_args(&argv(&["byroredux"]));
            assert_eq!(
                expansion.args[2..],
                expected[..],
                "wrong flags for {action:?}"
            );
        }
    }

    /// §2.4 — an explicit flag overrides its request-file counterpart, so a
    /// developer can override one field without editing TOML.
    #[test]
    fn explicit_command_line_flags_suppress_their_request_counterparts() {
        let request = BootRequest::for_profile("skyrim_se").with_action(Action::Cell {
            edid: "WhiterunBanneredMare".into(),
        });

        let overridden = request.to_args(&argv(&["byroredux", "--cell", "BleakFallsBarrow01"]));
        assert_eq!(overridden.args, argv(&["--game", "skyrim_se"]));
        assert_eq!(overridden.notes.len(), 1);

        // A different location flag suppresses the cell action just as well —
        // the check is on the whole location set, not on the matching flag.
        let by_grid = request.to_args(&argv(&["byroredux", "--wrld", "Tamriel"]));
        assert_eq!(by_grid.args, argv(&["--game", "skyrim_se"]));

        let by_game = request.to_args(&argv(&["byroredux", "--game", "fo4"]));
        assert_eq!(by_game.args, argv(&["--cell", "WhiterunBanneredMare"]));

        let by_esm = self_contained().to_args(&argv(&["byroredux", "--esm", "Other.esm"]));
        assert!(by_esm.args.is_empty(), "{:?}", by_esm.args);
        assert_eq!(by_esm.notes.len(), 1);

        let by_load = BootRequest::for_profile("fnv")
            .with_action(Action::Continue { slot: 3 })
            .to_args(&argv(&["byroredux", "--load", "7"]));
        assert_eq!(by_load.args, argv(&["--game", "fnv"]));
    }

    /// An explicit `--radius` wins, but must not take the grid with it: the
    /// worldspace and cell coordinates are still the request's to supply.
    #[test]
    fn an_explicit_radius_suppresses_only_the_radius() {
        let request = BootRequest::for_profile("fnv").with_action(Action::Grid {
            worldspace: "WastelandNV".into(),
            x: 0,
            y: 0,
            radius: Some(3),
        });
        let expansion = request.to_args(&argv(&["byroredux", "--radius", "5"]));
        assert_eq!(
            expansion.args,
            argv(&["--game", "fnv", "--wrld", "WastelandNV", "--grid", "0,0"])
        );
        assert_eq!(expansion.notes.len(), 1);
    }

    /// `new_game` reads its target from the profile registry, which a
    /// self-contained request bypasses. The flag is still emitted (it is inert,
    /// not harmful), but the caller is told the placement will not resolve.
    #[test]
    fn new_game_without_a_profile_source_reports_that_it_has_no_target() {
        let mut request = self_contained();
        request.action = Some(Action::NewGame);
        let expansion = request.to_args(&argv(&["byroredux"]));
        assert!(expansion.args.iter().any(|a| a == "--new-game"));
        assert!(
            expansion.notes.iter().any(|n| n.contains("no target")),
            "{:?}",
            expansion.notes
        );
    }

    /// A request naming nothing is legal — it is how a launcher says "just
    /// start the engine" — but it must say so rather than expand to silence.
    #[test]
    fn an_empty_request_expands_to_nothing_with_an_explanation() {
        let expansion = BootRequest::default().to_args(&argv(&["byroredux"]));
        assert!(expansion.args.is_empty());
        assert_eq!(expansion.notes.len(), 1);
    }

    /// Blank archive/master entries are dropped rather than joined into a
    /// trailing-separator path that would fail to open at load time.
    #[test]
    fn blank_archive_and_master_entries_are_skipped() {
        let mut request = self_contained();
        request.game.masters.push("   ".into());
        request.game.archives.textures.push(String::new());
        let expansion = request.to_args(&argv(&["byroredux"]));
        assert!(
            expansion
                .args
                .iter()
                .all(|arg| !arg.ends_with('/') && !arg.trim().is_empty()),
            "{:?}",
            expansion.args
        );
    }
}
