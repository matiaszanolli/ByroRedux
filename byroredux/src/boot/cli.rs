//! CLI argument expansion (#3855, split from `boot.rs`).
//!
//! Two expanders sit in front of the real arg parser: `expand_boot_request`
//! turns a `--boot <request>` seam into the equivalent command line, and
//! `expand_game_profile_args` resolves `--game <profile>` into the explicit
//! ESM/BSA flags. Both are pure `Vec<String> -> Vec<String>`, which is what
//! makes the equivalence tests at the bottom of this file possible.

use anyhow::{Context, Result};
use byroredux_boot_request::BootRequest;

use crate::cli_args::parse_string_arg;
use crate::settings_io;

/// Phase 20 — `--game <key>` CLI expansion. Looks up the named
/// profile in `assets/debug_profiles.toml` (+ per-user override at
/// `~/.byroredux/profiles.toml`), resolves `<games-root>/<subdir>`,
/// and appends synthetic archive args to the input list. With
/// `--new-game`, it also expands the profile's authored intro
/// worldspace/grid/radius. The user's own flags
/// stay at the FRONT — first-occurrence-wins for unique args means
/// a user `--esm Custom.esm` beats the profile default; additive
/// args (`--bsa`) take both. No-op when `--game` is absent.
///
/// `--games-root <path>` and `BYROREDUX_GAMES_ROOT` env var
/// override the default `/mnt/data/SteamLibrary/steamapps/common`.
/// Both stripped from the returned arg list — the rest of the
/// engine doesn't read them.
///
/// Missing profile / non-existent paths → log a warning and
/// return the input unchanged so the engine still boots (the
/// user can correct + retry without a re-build cycle).
/// Expand `--boot <path>` — the launcher handoff — into engine argv.
///
/// Runs *before* [`expand_game_profile_args`] so a request may legitimately
/// emit `--game <key>` and let the already-tested profile expander do the
/// archive fan-out; a self-contained request emits resolved paths instead and
/// the profile expander then sees `--esm` and leaves it alone. Nothing
/// downstream of these two functions learns that a launcher exists.
///
/// Generated flags are appended after the user's own argv, so an explicit
/// command-line flag keeps first-occurrence precedence over its counterpart in
/// the request file. See `docs/engine/launcher.md` §2.
///
/// A request that cannot be read, parsed, or version-matched is a hard error
/// rather than a warning: the launcher wrote it, so a bad one means the two
/// binaries disagree, and half-loading it would strand the user in a scene they
/// did not ask for with no indication why.
pub(super) fn expand_boot_request(args: Vec<String>) -> Result<Vec<String>> {
    let Some(path) = parse_string_arg(&args, "--boot") else {
        return Ok(args);
    };
    let request = BootRequest::load(&path)
        .with_context(|| format!("--boot {path}: could not load the launcher boot request"))?;

    let mut args = strip_flag_and_value(args, "--boot");

    // The settings registry is read from this path before `VulkanContext` is
    // created, so pointing at it here is what lets a launcher steer renderer
    // setup. An explicit env var still wins — same precedence as argv.
    if let Some(settings_path) = request.settings_path() {
        if std::env::var_os(settings_io::SETTINGS_PATH_ENV).is_none() {
            std::env::set_var(settings_io::SETTINGS_PATH_ENV, settings_path);
        } else {
            eprintln!(
                "--boot {path}: settings path {settings_path:?} suppressed; \
                 {} is set in the environment",
                settings_io::SETTINGS_PATH_ENV
            );
        }
    }

    let expansion = request.to_args(&args);
    for note in &expansion.notes {
        eprintln!("--boot {path}: {note}");
    }
    eprintln!(
        "--boot {path}: expanded to {} argument(s)",
        expansion.args.len()
    );
    args.extend(expansion.args);
    Ok(args)
}

#[cfg(test)]
mod boot_request_seam_tests {
    //! The `--boot` seam, tested as *equivalence* rather than against literal
    //! flag vectors.
    //!
    //! Both expanders read the real profile registry and resolve paths against
    //! the machine's games root, so a literal expectation would encode one
    //! developer's disk layout. Comparing a request-driven expansion against
    //! the equivalent hand-typed command line cancels the environment out and
    //! asserts the property that actually matters: **a boot request reaches the
    //! same engine state as the documented CLI.**

    use super::{expand_boot_request, expand_game_profile_args};
    use byroredux_boot_request::{Action, BootRequest};

    fn argv(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    /// Expand a request written to a scratch file, exactly as `run()` does.
    fn expand_via_file(request: &BootRequest, extra: &[&str]) -> Vec<String> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("boot.toml");
        request.save(&path).unwrap();

        let mut args = argv(&["byroredux", "--boot"]);
        args.push(path.to_string_lossy().into_owned());
        args.extend(argv(extra));
        expand_game_profile_args(expand_boot_request(args).unwrap())
    }

    /// The P1 gate: `--boot` with a profile request lands on the same argv as
    /// the equivalent `--game <key> --cell <edid>` invocation from the README.
    #[test]
    fn a_profile_request_expands_to_the_same_argv_as_the_documented_cli() {
        let request = BootRequest::for_profile("skyrim_se").with_action(Action::Cell {
            edid: "WhiterunBanneredMare".into(),
        });
        let typed = expand_game_profile_args(argv(&[
            "byroredux",
            "--game",
            "skyrim_se",
            "--cell",
            "WhiterunBanneredMare",
        ]));
        assert_eq!(expand_via_file(&request, &[]), typed);
    }

    /// Same property for an exterior grid, which exercises the multi-flag
    /// `--wrld` / `--grid` / `--radius` triple.
    #[test]
    fn a_grid_request_expands_to_the_same_argv_as_the_documented_cli() {
        let request = BootRequest::for_profile("fnv").with_action(Action::Grid {
            worldspace: "WastelandNV".into(),
            x: 0,
            y: 0,
            radius: Some(3),
        });
        let typed = expand_game_profile_args(argv(&[
            "byroredux",
            "--game",
            "fnv",
            "--wrld",
            "WastelandNV",
            "--grid",
            "0,0",
            "--radius",
            "3",
        ]));
        assert_eq!(expand_via_file(&request, &[]), typed);
    }

    /// `--boot` itself must not survive into the expanded argv — every
    /// downstream parser walks the whole vector, and an unrecognised flag
    /// followed by a path is exactly the shape that confuses positional
    /// mesh-path handling.
    #[test]
    fn the_boot_flag_and_its_value_are_stripped() {
        let expanded = expand_via_file(&BootRequest::for_profile("fnv"), &[]);
        assert!(!expanded.iter().any(|arg| arg == "--boot"));
        assert!(!expanded.iter().any(|arg| arg.ends_with("boot.toml")));
    }

    /// §2.4 — an explicit flag overrides its request counterpart, so one field
    /// can be changed without editing the file.
    #[test]
    fn an_explicit_cell_overrides_the_request_action() {
        let request = BootRequest::for_profile("skyrim_se").with_action(Action::Cell {
            edid: "WhiterunBanneredMare".into(),
        });
        let expanded = expand_via_file(&request, &["--cell", "BleakFallsBarrow01"]);
        assert!(expanded
            .windows(2)
            .any(|pair| pair == ["--cell", "BleakFallsBarrow01"]));
        assert!(!expanded.iter().any(|arg| arg == "WhiterunBanneredMare"));
    }

    /// A version skew means the launcher and engine are from different builds.
    /// It must stop the launch, not degrade it — a half-loaded request would
    /// strand the user in a scene they did not ask for.
    #[test]
    fn a_version_mismatch_refuses_the_launch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("boot.toml");
        std::fs::write(&path, "version = 99\n[game]\nprofile = \"fnv\"\n").unwrap();

        let mut args = argv(&["byroredux", "--boot"]);
        args.push(path.to_string_lossy().into_owned());
        let error = expand_boot_request(args).unwrap_err();
        let rendered = format!("{error:#}");
        assert!(rendered.contains("99"), "{rendered}");
        assert!(rendered.contains("different builds"), "{rendered}");
    }

    /// A `--boot` pointing at nothing is a hard error naming the path, not a
    /// silent fallback to the default scene.
    #[test]
    fn a_missing_request_file_refuses_the_launch() {
        let args = argv(&["byroredux", "--boot", "/nonexistent/boot.toml"]);
        let rendered = format!("{:#}", expand_boot_request(args).unwrap_err());
        assert!(rendered.contains("/nonexistent/boot.toml"), "{rendered}");
    }

    /// No `--boot` at all must leave argv byte-for-byte untouched, so the seam
    /// is inert for every existing invocation.
    #[test]
    fn argv_without_the_flag_passes_through_unchanged() {
        let args = argv(&[
            "byroredux",
            "--mesh",
            "sweetroll.nif",
            "--bench-frames",
            "10",
        ]);
        assert_eq!(expand_boot_request(args.clone()).unwrap(), args);
    }
}

/// Expand a profile's present-only [`GameProfileEntry::optional_bsas`] tier
/// into archive args, skipping every entry that is not on disk (#3924).
///
/// Split out of [`expand_game_profile_args`] because that function reads the
/// real profile registry and the real games root, so the present-only rule —
/// the whole point of this tier — could not otherwise be exercised without
/// a particular game installed.
fn push_optional_archive_args(
    args: &mut Vec<String>,
    data_dir: &std::path::Path,
    optional_bsas: &[String],
    game_key: &str,
) {
    for bsa in optional_bsas {
        let path = data_dir.join(bsa);
        if !path.is_file() {
            eprintln!("--game {game_key}: optional archive {bsa} not present, skipping");
            continue;
        }
        let joined = path.to_string_lossy().into_owned();
        // All three flags, not `--bsa` alone: a Creation Club archive is a
        // mod bundle rather than a category archive, so one file carries
        // that mod's meshes, textures and sounds together. The
        // already-opened sets in `build_texture_provider` are per pool
        // (#2584), so naming one path in two of them opens it in both
        // rather than deduplicating it away.
        for flag in ["--bsa", "--textures-bsa", "--sounds-bsa"] {
            args.push(flag.to_string());
            args.push(joined.clone());
        }
    }
}

pub(super) fn expand_game_profile_args(mut args: Vec<String>) -> Vec<String> {
    let new_game = args.iter().any(|arg| arg == "--new-game");
    // Launch defaults from the `[defaults]` table (profiles.toml,
    // shipped + per-user override). Let an explicit `--game` win; fall
    // back to `[defaults].game` ONLY when no other content-loading flag
    // is present, so `--mesh foo.nif`, `--cmd`, an explicit `--esm`, or
    // a master-only run aren't hijacked into loading the default cell.
    let defaults = crate::game_profiles::load_launch_defaults();
    let has_other_load_flags = [
        "--esm",
        "--mesh",
        "--tree",
        "--menu",
        "--kf",
        "--cmd",
        "--master",
        "--studio",
        "--cornell",
        "--cornell-sun",
        "--cornell-oracle",
        "--cornell-glass-dragon",
        "--combustion-lab",
        "--combustion-lab-nuclear",
    ]
    .iter()
    .any(|f| args.iter().any(|a| a == f));
    let game_key = match parse_string_arg(&args, "--game") {
        Some(k) => k,
        None => match defaults.game.clone() {
            Some(k) if !has_other_load_flags => {
                eprintln!("[defaults] game = {k:?} (no --game / load flag given)");
                k
            }
            _ => return args,
        },
    };
    // `--games-root` CLI wins; else the config `[defaults].games_root`.
    let games_root_cli =
        parse_string_arg(&args, "--games-root").or_else(|| defaults.games_root.clone());

    // Strip the two new flags + their values from the returned
    // args; downstream code doesn't recognise them.
    args = strip_flag_and_value(args, "--game");
    args = strip_flag_and_value(args, "--games-root");
    args.retain(|arg| arg != "--new-game");

    // Load profile registry from disk. Same loader the App uses
    // at world init; safe to call before logging is initialised
    // (uses eprintln internally on failure).
    let registry = crate::game_profiles::load_default();
    let entry = match registry.get(&game_key) {
        Some(e) => e.clone(),
        None => {
            eprintln!(
                "--game {}: profile not found in `assets/debug_profiles.toml` \
                 or `~/.byroredux/profiles.toml`. Known keys: {:?}",
                game_key,
                registry.iter().map(|(k, _)| k).collect::<Vec<_>>()
            );
            return args;
        }
    };

    let games_root = crate::game_profiles::resolve_games_root(games_root_cli.as_deref());
    let data_dir = crate::game_profiles::resolve_profile_root(&entry, &games_root);

    if data_dir.as_os_str().is_empty() {
        eprintln!(
            "--game {}: profile carries neither `root` nor `subdir`; cannot resolve data dir",
            game_key,
        );
        return args;
    }
    if !data_dir.exists() {
        eprintln!(
            "--game {}: resolved data dir does not exist: {} \
             (set --games-root, BYROREDUX_GAMES_ROOT env var, or override \
              the profile's `root` in ~/.byroredux/profiles.toml)",
            game_key,
            data_dir.display(),
        );
        // Fall through anyway — let downstream loaders report
        // specific missing files for clearer diagnostics.
    }

    // Append profile-derived args. User's earlier --esm wins on
    // first-occurrence-wins; additive --bsa flags compose.
    eprintln!(
        "--game {} expanding from {} (data dir: {})",
        game_key,
        entry.name,
        data_dir.display(),
    );
    let join_arg =
        |archive: &str| -> String { data_dir.join(archive).to_string_lossy().into_owned() };

    args.push("--esm".to_string());
    args.push(join_arg(&entry.esm));
    for bsa in &entry.default_bsas {
        args.push("--bsa".to_string());
        args.push(join_arg(bsa));
    }
    for bsa in &entry.default_textures_bsas {
        args.push("--textures-bsa".to_string());
        args.push(join_arg(bsa));
    }
    for bsa in &entry.default_scripts_bsas {
        args.push("--scripts-bsa".to_string());
        args.push(join_arg(bsa));
    }
    for bsa in &entry.default_sounds_bsas {
        args.push("--sounds-bsa".to_string());
        args.push(join_arg(bsa));
    }
    for bsa in &entry.default_materials_bsas {
        args.push("--materials-ba2".to_string());
        args.push(join_arg(bsa));
    }

    // #3924 — the present-only tier. AE ships `_ResourcePack.bsa` and a
    // per-account set of `cc*.bsa` Creation Club bundles that no naming rule
    // can reach: none is a numeric sibling of a listed archive, so
    // `numeric_sibling_paths` cannot find them, and until this list existed
    // the runtime had no way to name them at all. The NIF corpus gate has
    // swept exactly this tier since #3369, which left it measuring content
    // the engine could not open.
    //
    // Appended last so #3637's last-wins precedence puts add-on content on
    // top of vanilla. Skipped in silence when absent — which of these an
    // install carries depends on the account and edition, so a miss is the
    // normal case, unlike a `default_bsas` miss, which is a broken install.
    //
    // Expanded into all three content flags rather than `--bsa` alone: a
    // Creation Club archive is a mod bundle, not a category archive, so one
    // file carries that mod's meshes, textures and sounds together. The
    // already-opened sets in `build_texture_provider` are per pool (#2584),
    // so naming one path in two of them opens it in both rather than
    // deduplicating it away.
    push_optional_archive_args(&mut args, &data_dir, &entry.optional_bsas, &game_key);

    if new_game {
        let has_location = ["--cell", "--grid", "--wrld"]
            .iter()
            .any(|flag| args.iter().any(|arg| arg == flag));
        match (&entry.new_game_worldspace, &entry.new_game_grid) {
            (Some(worldspace), Some(grid)) if !has_location => {
                args.push("--wrld".to_string());
                args.push(worldspace.clone());
                args.push("--grid".to_string());
                args.push(grid.clone());
                if !args.iter().any(|arg| arg == "--radius") {
                    if let Some(radius) = entry.new_game_radius {
                        args.push("--radius".to_string());
                        args.push(radius.to_string());
                    }
                }
            }
            (Some(_), Some(_)) => {
                eprintln!("--new-game: explicit location flags override the profile target")
            }
            _ => eprintln!("--game {game_key} --new-game: profile has no new-game worldspace/grid"),
        }
    }

    // Default cell: inject `[defaults].cell` only when the profile was
    // resolved and no explicit location flag is present. A bare
    // `--game fnv` (or a no-arg default-game launch) then boots
    // straight into the configured cell.
    if !args.iter().any(|arg| arg == "--studio") {
        if let Some(cell) = &defaults.cell {
            let has_location = ["--cell", "--grid", "--wrld"]
                .iter()
                .any(|f| args.iter().any(|a| a == f));
            if !has_location {
                eprintln!("[defaults] cell = {cell:?} (no --cell / --grid / --wrld given)");
                args.push("--cell".to_string());
                args.push(cell.clone());
            }
        }
    }
    args
}

/// Remove every `<flag> <value>` pair from the args list. Used
/// by `--game` expansion to strip the new flags before downstream
/// parsers see them.
fn strip_flag_and_value(args: Vec<String>, flag: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut iter = args.into_iter();
    while let Some(a) = iter.next() {
        if a == flag {
            // Also discard the next arg (the value).
            let _ = iter.next();
            continue;
        }
        out.push(a);
    }
    out
}

#[cfg(test)]
mod optional_archive_tier_tests {
    use super::push_optional_archive_args;

    /// #3924 — the present-only rule. Which Creation Club archives an
    /// install carries depends on the account and the edition, so an absent
    /// entry is the normal case and must not reach the archive layer at all:
    /// `open_with_numeric_siblings` would `log::warn!` per miss, training
    /// operators to ignore the one warning that means a broken install.
    #[test]
    fn an_absent_optional_archive_contributes_no_args() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("present.bsa"), b"x").unwrap();

        let mut args = Vec::new();
        push_optional_archive_args(
            &mut args,
            dir.path(),
            &["absent.bsa".to_owned(), "present.bsa".to_owned()],
            "test",
        );

        assert!(
            !args.iter().any(|a| a.contains("absent.bsa")),
            "an archive that is not on disk must be skipped silently: {args:?}"
        );
        assert_eq!(
            args.iter().filter(|a| a.contains("present.bsa")).count(),
            3,
            "a present archive is a mod bundle, so it is expanded into the \
             mesh, texture and sound pools alike: {args:?}"
        );
    }

    /// A directory entry is not an archive. `is_file` rather than `exists`
    /// is what keeps a stray `cc*.bsa/` folder from being handed to the
    /// archive opener as a path.
    #[test]
    fn a_directory_is_not_mistaken_for_an_archive() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("notanarchive.bsa")).unwrap();

        let mut args = Vec::new();
        push_optional_archive_args(
            &mut args,
            dir.path(),
            &["notanarchive.bsa".to_owned()],
            "test",
        );
        assert!(args.is_empty(), "{args:?}");
    }

    /// Each present entry is expanded once per pool, and the three flags
    /// name the same resolved path — the pools dedupe independently (#2584),
    /// so one file legitimately lands in all three.
    #[test]
    fn every_pool_receives_the_same_resolved_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("cc.bsa"), b"x").unwrap();

        let mut args = Vec::new();
        push_optional_archive_args(&mut args, dir.path(), &["cc.bsa".to_owned()], "test");

        let expected = dir.path().join("cc.bsa").to_string_lossy().into_owned();
        assert_eq!(
            args,
            vec![
                "--bsa".to_owned(),
                expected.clone(),
                "--textures-bsa".to_owned(),
                expected.clone(),
                "--sounds-bsa".to_owned(),
                expected,
            ]
        );
    }
}
