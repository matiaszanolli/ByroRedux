use crate::guided::{guided_upscale, merge_reference_alpha};
use crate::space::{ensure_output_and_scratch_space, human_bytes};
use crate::{output_png_path, Manifest, MapRole, SourceStack};
use anyhow::{bail, Context, Result};
use image::RgbaImage;
use serde::Serialize;
use std::path::Path;
use std::process::Command;

pub struct RunOptions<'a> {
    pub output_root: &'a Path,
    pub dry_run: bool,
    pub overwrite: bool,
}

#[derive(Debug, Serialize)]
pub struct RunReport {
    pub scale: u32,
    pub sets: Vec<SetReport>,
}

#[derive(Debug, Serialize)]
pub struct SetReport {
    pub name: String,
    pub reference: OutputReport,
    pub maps: Vec<OutputReport>,
}

#[derive(Debug, Serialize)]
pub struct OutputReport {
    pub role: String,
    pub source: String,
    pub output: String,
    pub source_dimensions: [u32; 2],
    pub output_dimensions: [u32; 2],
}

#[derive(Clone, Copy, Debug, Default)]
struct RunEstimate {
    output_bytes: u64,
    scratch_peak_bytes: u64,
    texture_count: usize,
}

pub fn run_manifest(
    sources: &SourceStack,
    manifest: &Manifest,
    options: RunOptions<'_>,
) -> Result<RunReport> {
    manifest.validate()?;
    preflight_outputs(manifest, &options)?;
    let estimate = estimate_run_space(sources, manifest)?;
    let scratch_root = std::env::temp_dir();
    let space = ensure_output_and_scratch_space(
        options.output_root,
        estimate.output_bytes,
        &scratch_root,
        estimate.scratch_peak_bytes,
    )?;
    if space.shared_volume {
        eprintln!(
            "space preflight: {} textures need approximately {} output + {} scratch; {} available on their shared filesystem",
            estimate.texture_count,
            human_bytes(space.output_estimate_bytes),
            human_bytes(space.scratch_estimate_bytes),
            human_bytes(space.output_available_bytes),
        );
    } else {
        eprintln!(
            "space preflight: {} textures need approximately {} output ({} available) + {} scratch ({} available)",
            estimate.texture_count,
            human_bytes(space.output_estimate_bytes),
            human_bytes(space.output_available_bytes),
            human_bytes(space.scratch_estimate_bytes),
            human_bytes(space.scratch_available_bytes),
        );
    }

    let mut report = RunReport {
        scale: manifest.scale,
        sets: Vec::with_capacity(manifest.sets.len()),
    };

    if options.dry_run {
        print_dry_run_commands(manifest, &scratch_root);
        return Ok(report);
    }

    for (set_index, set) in manifest.sets.iter().enumerate() {
        let reference_bytes = sources
            .extract(&set.reference)
            .with_context(|| format!("load reference for set {:?}", set.name))?;
        let reference_low = decode_texture(&reference_bytes, &set.reference)?;
        let set_scratch = tempfile::Builder::new()
            .prefix(&format!("byro-texture-upscale-{set_index:05}-"))
            .tempdir_in(&scratch_root)
            .with_context(|| {
                format!(
                    "create texture-upscale scratch in {}",
                    scratch_root.display()
                )
            })?;
        let set_scratch = set_scratch.path();
        let upscaler_input = set_scratch.join("reference-input.png");
        let upscaler_output = set_scratch.join("reference-upscaled.png");
        reference_low
            .save(&upscaler_input)
            .with_context(|| format!("write {}", upscaler_input.display()))?;

        let args =
            manifest
                .upscaler
                .expanded_args(&upscaler_input, &upscaler_output, manifest.scale);
        let status = Command::new(&manifest.upscaler.program)
            .args(&args)
            .status()
            .with_context(|| {
                format!(
                    "launch upscaler {:?}; install it or edit [upscaler] in the manifest",
                    manifest.upscaler.program
                )
            })?;
        if !status.success() {
            bail!(
                "upscaler {:?} failed for set {:?} with status {}",
                manifest.upscaler.program,
                set.name,
                status
            );
        }
        if !upscaler_output.is_file() {
            bail!(
                "upscaler succeeded but did not create {}",
                upscaler_output.display()
            );
        }

        let reference_high = image::open(&upscaler_output)
            .with_context(|| format!("decode upscaled reference {}", upscaler_output.display()))?
            .to_rgba8();
        let expected_reference_dimensions = scaled_dimensions(
            reference_low.width(),
            reference_low.height(),
            manifest.scale,
        )?;
        if reference_high.dimensions() != expected_reference_dimensions {
            bail!(
                "upscaler output for {:?} is {:?}, expected {:?}",
                set.name,
                reference_high.dimensions(),
                expected_reference_dimensions
            );
        }

        let reference_merged = merge_reference_alpha(
            &reference_low,
            &reference_high,
            manifest.scale,
            manifest.guide_sigma as f32,
        );
        let reference_output = output_png_path(options.output_root, &set.reference)?;
        save_output(&reference_merged, &reference_output, options.overwrite)?;

        let mut set_report = SetReport {
            name: set.name.clone(),
            reference: OutputReport {
                role: "reference".to_string(),
                source: set.reference.clone(),
                output: reference_output.display().to_string(),
                source_dimensions: [reference_low.width(), reference_low.height()],
                output_dimensions: [reference_merged.width(), reference_merged.height()],
            },
            maps: Vec::with_capacity(set.maps.len()),
        };

        for map in &set.maps {
            let map_bytes = sources
                .extract(&map.path)
                .with_context(|| format!("load {:?} map {}", map.role, map.path))?;
            let map_low = decode_texture(&map_bytes, &map.path)?;
            let map_high = guided_upscale(
                &reference_low,
                &reference_high,
                &map_low,
                manifest.scale,
                map.role,
                manifest.guide_sigma as f32,
            );
            let map_output = output_png_path(options.output_root, &map.path)?;
            save_output(&map_high, &map_output, options.overwrite)?;
            set_report.maps.push(OutputReport {
                role: role_name(map.role).to_string(),
                source: map.path.clone(),
                output: map_output.display().to_string(),
                source_dimensions: [map_low.width(), map_low.height()],
                output_dimensions: [map_high.width(), map_high.height()],
            });
        }
        report.sets.push(set_report);
    }

    if !options.dry_run {
        std::fs::create_dir_all(options.output_root).with_context(|| {
            format!("create output directory {}", options.output_root.display())
        })?;
        let report_path = options.output_root.join("texture-upscale-report.json");
        let json = serde_json::to_vec_pretty(&report).context("serialize upscale report")?;
        std::fs::write(&report_path, json)
            .with_context(|| format!("write report {}", report_path.display()))?;
    }
    Ok(report)
}

fn preflight_outputs(manifest: &Manifest, options: &RunOptions<'_>) -> Result<()> {
    for set in &manifest.sets {
        let reference = output_png_path(options.output_root, &set.reference)?;
        if !options.dry_run && !options.overwrite && reference.exists() {
            bail!(
                "refusing to overwrite {}; pass --overwrite to replace it",
                reference.display()
            );
        }
        for map in &set.maps {
            let output = output_png_path(options.output_root, &map.path)?;
            if !options.dry_run && !options.overwrite && output.exists() {
                bail!(
                    "refusing to overwrite {}; pass --overwrite to replace it",
                    output.display()
                );
            }
        }
    }
    Ok(())
}

fn estimate_run_space(sources: &SourceStack, manifest: &Manifest) -> Result<RunEstimate> {
    let mut estimate = RunEstimate {
        // Generous allowance for the JSON report and filesystem metadata.
        output_bytes: 1024 * 1024,
        ..RunEstimate::default()
    };

    for set in &manifest.sets {
        let reference_bytes = sources
            .extract(&set.reference)
            .with_context(|| format!("preflight reference for set {:?}", set.name))?;
        let reference = decode_texture(&reference_bytes, &set.reference)?;
        let reference_low_bytes = encoded_rgba_upper_bound(reference.width(), reference.height())?;
        let (high_width, high_height) =
            scaled_dimensions(reference.width(), reference.height(), manifest.scale)?;
        let reference_high_bytes = encoded_rgba_upper_bound(high_width, high_height)?;
        estimate.output_bytes = checked_add(
            estimate.output_bytes,
            reference_high_bytes,
            "output estimate",
        )?;
        estimate.scratch_peak_bytes = estimate.scratch_peak_bytes.max(checked_add(
            reference_low_bytes,
            reference_high_bytes,
            "scratch estimate",
        )?);
        estimate.texture_count += 1;

        for map in &set.maps {
            let map_bytes = sources
                .extract(&map.path)
                .with_context(|| format!("preflight {:?} map {}", map.role, map.path))?;
            let map_image = decode_texture(&map_bytes, &map.path)?;
            let (map_width, map_height) =
                scaled_dimensions(map_image.width(), map_image.height(), manifest.scale)?;
            estimate.output_bytes = checked_add(
                estimate.output_bytes,
                encoded_rgba_upper_bound(map_width, map_height)?,
                "output estimate",
            )?;
            estimate.texture_count += 1;
        }
    }
    Ok(estimate)
}

fn print_dry_run_commands(manifest: &Manifest, scratch_root: &Path) {
    let preview_root = scratch_root.join("byro-texture-upscale-dry-run");
    for (set_index, _set) in manifest.sets.iter().enumerate() {
        let set_scratch = preview_root.join(format!("set-{set_index:05}"));
        let input = set_scratch.join("reference-input.png");
        let output = set_scratch.join("reference-upscaled.png");
        let args = manifest
            .upscaler
            .expanded_args(&input, &output, manifest.scale);
        eprintln!(
            "dry-run: {} {}",
            manifest.upscaler.program,
            args.iter()
                .map(|arg| format!("{arg:?}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
}

fn scaled_dimensions(width: u32, height: u32, scale: u32) -> Result<(u32, u32)> {
    let width = width
        .checked_mul(scale)
        .context("upscaled texture width exceeds u32")?;
    let height = height
        .checked_mul(scale)
        .context("upscaled texture height exceeds u32")?;
    Ok((width, height))
}

/// Conservative PNG-sized bound: raw RGBA bytes, five percent encoder
/// overhead, per-row framing, and a fixed header/metadata allowance.
fn encoded_rgba_upper_bound(width: u32, height: u32) -> Result<u64> {
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .context("texture pixel-count overflow")?;
    let raw = pixels
        .checked_mul(4)
        .context("texture RGBA byte-count overflow")?;
    let overhead = raw / 20;
    raw.checked_add(overhead)
        .and_then(|bytes| bytes.checked_add(u64::from(height) * 8))
        .and_then(|bytes| bytes.checked_add(64 * 1024))
        .context("texture output-size estimate overflow")
}

fn checked_add(left: u64, right: u64, label: &str) -> Result<u64> {
    left.checked_add(right)
        .with_context(|| format!("{label} overflow"))
}

fn decode_texture(bytes: &[u8], name: &str) -> Result<RgbaImage> {
    image::load_from_memory(bytes)
        .with_context(|| {
            format!(
                "decode {name}; the built-in DDS path supports BC1/BC2/BC3, \
                 while BC5/BC7 inputs need conversion to PNG first"
            )
        })
        .map(|image| image.to_rgba8())
}

fn save_output(image: &RgbaImage, path: &Path, overwrite: bool) -> Result<()> {
    if path.exists() && !overwrite {
        bail!(
            "refusing to overwrite {}; pass --overwrite to replace it",
            path.display()
        );
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create output directory {}", parent.display()))?;
    }
    image
        .save(path)
        .with_context(|| format!("write output {}", path.display()))
}

fn role_name(role: MapRole) -> &'static str {
    match role {
        MapRole::Normal => "normal",
        MapRole::Glow => "glow",
        MapRole::Specular => "specular",
        MapRole::Mask => "mask",
        MapRole::Height => "height",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_bound_exceeds_raw_rgba_size() {
        assert!(encoded_rgba_upper_bound(1024, 1024).unwrap() > 1024 * 1024 * 4);
    }

    #[test]
    fn scaled_dimensions_reject_overflow() {
        assert!(scaled_dimensions(u32::MAX, 1, 2).is_err());
    }

    #[test]
    fn dry_run_checks_the_plan_without_creating_output_or_launching_the_model() {
        let fixture = tempfile::tempdir().unwrap();
        let source_root = fixture.path().join("source");
        std::fs::create_dir(&source_root).unwrap();
        RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255]))
            .save(source_root.join("tiny.png"))
            .unwrap();

        let sources = SourceStack::open(&[source_root]).unwrap();
        let mut manifest = Manifest::discovered(2, ["tiny.png".to_string()]);
        manifest.upscaler.program = "this-program-must-not-run".to_string();
        let output = fixture.path().join("output");

        let report = run_manifest(
            &sources,
            &manifest,
            RunOptions {
                output_root: &output,
                dry_run: true,
                overwrite: false,
            },
        )
        .unwrap();

        assert!(report.sets.is_empty());
        assert!(!output.exists());
    }
}
