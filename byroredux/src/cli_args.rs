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
    // FSR 3.1 Quality is the default as of execution phase 7 of the FSR plan,
    // on the phase-7 benchmark: +40% to +68% net frame recovery across every
    // measured game scene, at SSIM 0.955 (Cornell) / 0.990 (FO4 Dugout)
    // against the native TAA render. `--upscaler taa` remains the supported
    // fallback and is still what runs on a device where FSR context creation
    // fails.
    let upscaler = match option("--upscaler")?.unwrap_or("fsr3") {
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

/// Parse an upscaler spec — the same grammar the CLI accepts, minus the flag
/// names: `taa`, `fsr3`, or `fsr3 <quality>`.
///
/// Shared by `--upscaler`/`--fsr-quality` and by the runtime switch (the
/// `r.upscaler` console command and the settings panel), so there is exactly
/// one place that decides what a valid selection looks like.
///
/// `/` separates as well as whitespace, which makes `UpscalerMode`'s own
/// `Display` output (`fsr3/quality`) a valid input. The settings registry
/// stores that canonical spelling, so a mode round-trips through the panel
/// without a second name mapping to drift out of sync.
pub fn parse_upscaler_spec(spec: &str) -> Result<UpscalerMode> {
    let mut parts = spec
        .split(|c: char| c.is_whitespace() || c == '/')
        .filter(|part| !part.is_empty());
    let Some(name) = parts.next() else {
        bail!("expected an upscaler name: taa or fsr3");
    };
    let quality = parts.next().map(str::parse::<FsrQuality>).transpose()?;
    if parts.next().is_some() {
        bail!("expected at most 'fsr3/<quality>', got '{spec}'");
    }
    match name {
        "taa" => {
            if let Some(inactive) = quality {
                log::warn!("upscaler quality {inactive} is inactive because taa is selected");
            }
            Ok(UpscalerMode::Taa)
        }
        "fsr3" => Ok(UpscalerMode::Fsr3(quality.unwrap_or(FsrQuality::Quality))),
        value => bail!(
            "{}",
            byroredux_renderer::vulkan::upscaling::ParseRendererOptionError::Upscaler(
                value.to_owned()
            )
        ),
    }
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

    /// Every mode must survive `Display` -> `parse_upscaler_spec`. The
    /// settings registry stores the `Display` spelling and the main loop
    /// parses it back, so a divergence would silently reject the panel's own
    /// values — and the seed at startup would log a warning on every boot.
    #[test]
    fn every_upscaler_mode_round_trips_through_its_display_form() {
        let modes = [
            UpscalerMode::Taa,
            UpscalerMode::Fsr3(FsrQuality::NativeAa),
            UpscalerMode::Fsr3(FsrQuality::Quality),
            UpscalerMode::Fsr3(FsrQuality::Balanced),
            UpscalerMode::Fsr3(FsrQuality::Performance),
        ];
        for mode in modes {
            let spec = mode.to_string();
            assert_eq!(
                parse_upscaler_spec(&spec).unwrap(),
                mode,
                "'{spec}' did not parse back to {mode}"
            );
        }
    }

    /// The console command accepts the space-separated form operators are
    /// likely to type; bare `fsr3` keeps the CLI's Quality default.
    #[test]
    fn upscaler_spec_accepts_space_separated_and_bare_forms() {
        assert_eq!(
            parse_upscaler_spec("fsr3 balanced").unwrap(),
            UpscalerMode::Fsr3(FsrQuality::Balanced)
        );
        assert_eq!(
            parse_upscaler_spec("fsr3").unwrap(),
            UpscalerMode::Fsr3(FsrQuality::Quality)
        );
        assert_eq!(parse_upscaler_spec("taa").unwrap(), UpscalerMode::Taa);
        assert!(parse_upscaler_spec("dlss").is_err());
        assert!(parse_upscaler_spec("fsr3/ultra").is_err());
        assert!(parse_upscaler_spec("").is_err());
        assert!(parse_upscaler_spec("fsr3 quality extra").is_err());
    }

    /// No `--upscaler` means FSR 3.1 Quality (phase 7). Pinned against the
    /// literal rather than `RendererConfig::default()` alone, so that a change
    /// to the type default cannot silently move the CLI default with it —
    /// the two agreeing is asserted separately in `upscaling.rs`.
    #[test]
    fn renderer_config_defaults_to_fsr_quality() {
        let parsed = parse_renderer_config(&args(&["byroredux"])).unwrap();
        assert_eq!(parsed.upscaler, UpscalerMode::Fsr3(FsrQuality::Quality));
        assert_eq!(parsed, RendererConfig::default());
    }

    /// `--fsr-quality` composes with the implicit default, so selecting a
    /// preset does not also require spelling out `--upscaler fsr3`.
    #[test]
    fn fsr_quality_alone_selects_that_preset() {
        assert_eq!(
            parse_renderer_config(&args(&["byroredux", "--fsr-quality", "performance"]))
                .unwrap()
                .upscaler,
            UpscalerMode::Fsr3(FsrQuality::Performance)
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
