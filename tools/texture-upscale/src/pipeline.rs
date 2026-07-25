use crate::guided::{guided_upscale, merge_reference_alpha};
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

pub fn run_manifest(
    sources: &SourceStack,
    manifest: &Manifest,
    options: RunOptions<'_>,
) -> Result<RunReport> {
    manifest.validate()?;
    preflight_outputs(manifest, &options)?;
    let scratch = tempfile::tempdir().context("create texture-upscale scratch directory")?;
    let mut report = RunReport {
        scale: manifest.scale,
        sets: Vec::with_capacity(manifest.sets.len()),
    };

    for (set_index, set) in manifest.sets.iter().enumerate() {
        let reference_bytes = sources
            .extract(&set.reference)
            .with_context(|| format!("load reference for set {:?}", set.name))?;
        let reference_low = decode_texture(&reference_bytes, &set.reference)?;
        let set_scratch = scratch.path().join(format!("set-{set_index:05}"));
        std::fs::create_dir_all(&set_scratch)
            .with_context(|| format!("create scratch {}", set_scratch.display()))?;
        let upscaler_input = set_scratch.join("reference-input.png");
        let upscaler_output = set_scratch.join("reference-upscaled.png");
        reference_low
            .save(&upscaler_input)
            .with_context(|| format!("write {}", upscaler_input.display()))?;

        let args =
            manifest
                .upscaler
                .expanded_args(&upscaler_input, &upscaler_output, manifest.scale);
        if options.dry_run {
            eprintln!(
                "dry-run: {} {}",
                manifest.upscaler.program,
                args.iter()
                    .map(|arg| format!("{arg:?}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            continue;
        }

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
        let expected_reference_dimensions = (
            reference_low.width().saturating_mul(manifest.scale),
            reference_low.height().saturating_mul(manifest.scale),
        );
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
    if options.dry_run || options.overwrite {
        return Ok(());
    }
    for set in &manifest.sets {
        let reference = output_png_path(options.output_root, &set.reference)?;
        if reference.exists() {
            bail!(
                "refusing to overwrite {}; pass --overwrite to replace it",
                reference.display()
            );
        }
        for map in &set.maps {
            let output = output_png_path(options.output_root, &map.path)?;
            if output.exists() {
                bail!(
                    "refusing to overwrite {}; pass --overwrite to replace it",
                    output.display()
                );
            }
        }
    }
    Ok(())
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
