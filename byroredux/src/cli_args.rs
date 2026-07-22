//! Tiny argv parsers used by `App::new` and `main`. Free functions
//! split out of `main.rs` to stay below the 2000-LOC ceiling
//! (TD9-NEW-01 / #1267).

use std::sync::OnceLock;

use anyhow::{bail, Result};
use byroredux_renderer::{FsrQuality, RendererConfig, UpscalerMode};

/// Process-wide effective args list. Phase 20 / 20.1 — main()
/// computes the expanded args (after `--game <key>` expansion)
/// once at startup and seeds this slot via
/// [`set_effective_args`]. Every site that previously called
/// `std::env::args().collect()` now reads through
/// [`effective_args`] so the expansion is universal — without
/// this indirection, scene loading + asset providers re-read
/// the raw argv and lose the synthesized `--esm` / `--bsa` /
/// etc. flags. See the Phase-20.1 commit for the
/// debugging journey.
static EFFECTIVE_ARGS: OnceLock<Vec<String>> = OnceLock::new();

/// Store the post-expansion args list. Call exactly once at
/// program start, after `--game` expansion. Re-call panics —
/// the singleton is set-once for the lifetime of the process.
pub fn set_effective_args(args: Vec<String>) {
    EFFECTIVE_ARGS
        .set(args)
        .expect("set_effective_args called more than once");
}

/// Read the effective args list. Falls back to
/// `std::env::args()` when the singleton hasn't been seeded —
/// preserves behaviour for unit tests / dev paths that don't
/// run through `main`.
pub fn effective_args() -> Vec<String> {
    if let Some(args) = EFFECTIVE_ARGS.get() {
        return args.clone();
    }
    std::env::args().collect()
}

pub fn parse_string_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

/// Parse the renderer selection once at application startup. Renderer code
/// receives the typed result and never re-reads process arguments.
pub fn parse_renderer_config(args: &[String]) -> Result<RendererConfig> {
    let option = |flag: &str| -> Result<Option<&str>> {
        let Some(index) = args.iter().position(|arg| arg == flag) else {
            return Ok(None);
        };
        let Some(value) = args.get(index + 1) else {
            bail!("{flag} requires a value");
        };
        Ok(Some(value.as_str()))
    };

    let quality = option("--fsr-quality")?
        .map(str::parse::<FsrQuality>)
        .transpose()?;
    let upscaler = match option("--upscaler")?.unwrap_or("taa") {
        "taa" => {
            if let Some(inactive) = quality {
                log::warn!(
                    "--fsr-quality {inactive} is inactive because --upscaler taa is selected"
                );
            }
            UpscalerMode::Taa
        }
        "fsr3" => UpscalerMode::Fsr3(quality.unwrap_or(FsrQuality::Quality)),
        value => bail!(
            "{}",
            byroredux_renderer::vulkan::upscaling::ParseRendererOptionError::Upscaler(
                value.to_owned()
            )
        ),
    };
    Ok(RendererConfig { upscaler })
}

/// Parse `x,y,z` into a `(f32, f32, f32)` tuple — stored as a plain
/// triple here to avoid leaking the renderer's `Vec3` into main.rs.
pub fn parse_vec3_arg(args: &[String], flag: &str) -> Option<(f32, f32, f32)> {
    let s = parse_string_arg(args, flag)?;
    let parts: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    match parts.as_slice() {
        [x, y, z] => Some((*x, *y, *z)),
        _ => {
            log::warn!("`{flag} {s}` could not be parsed as x,y,z floats; ignoring");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn renderer_config_defaults_to_taa() {
        assert_eq!(
            parse_renderer_config(&args(&["byroredux"])).unwrap(),
            RendererConfig::default()
        );
    }

    #[test]
    fn renderer_config_parses_every_fsr_preset() {
        for (name, quality) in [
            ("native-aa", FsrQuality::NativeAa),
            ("quality", FsrQuality::Quality),
            ("balanced", FsrQuality::Balanced),
            ("performance", FsrQuality::Performance),
        ] {
            let parsed = parse_renderer_config(&args(&[
                "byroredux",
                "--upscaler",
                "fsr3",
                "--fsr-quality",
                name,
            ]))
            .unwrap();
            assert_eq!(parsed.upscaler, UpscalerMode::Fsr3(quality));
        }
    }

    #[test]
    fn renderer_config_rejects_unknown_or_missing_values() {
        assert!(parse_renderer_config(&args(&["byroredux", "--upscaler", "dlss"])).is_err());
        assert!(parse_renderer_config(&args(&["byroredux", "--upscaler"])).is_err());
        assert!(parse_renderer_config(&args(&[
            "byroredux",
            "--upscaler",
            "fsr3",
            "--fsr-quality",
            "ultra"
        ]))
        .is_err());
    }
}
