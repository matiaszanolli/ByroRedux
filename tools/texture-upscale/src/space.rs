use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use sysinfo::Disks;

const MIN_HEADROOM_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug)]
struct VolumeSpace {
    mount_point: PathBuf,
    available_bytes: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct SpacePreflight {
    pub(crate) output_estimate_bytes: u64,
    pub(crate) scratch_estimate_bytes: u64,
    pub(crate) output_available_bytes: u64,
    pub(crate) scratch_available_bytes: u64,
    pub(crate) shared_volume: bool,
}

pub(crate) fn ensure_single_path_space(
    path: &Path,
    estimate_bytes: u64,
    label: &str,
) -> Result<()> {
    let volume = volume_for(path)?;
    ensure_available(
        volume.available_bytes,
        estimate_bytes,
        &format!("{label} on {}", volume.mount_point.display()),
    )
}

pub(crate) fn ensure_output_and_scratch_space(
    output_path: &Path,
    output_estimate_bytes: u64,
    scratch_path: &Path,
    scratch_estimate_bytes: u64,
) -> Result<SpacePreflight> {
    let output = volume_for(output_path)?;
    let scratch = volume_for(scratch_path)?;
    let shared_volume = output.mount_point == scratch.mount_point;

    if shared_volume {
        let combined = output_estimate_bytes
            .checked_add(scratch_estimate_bytes)
            .context("texture-upscale disk estimate overflow")?;
        ensure_available(
            output.available_bytes,
            combined,
            &format!("output and scratch on {}", output.mount_point.display()),
        )?;
    } else {
        ensure_available(
            output.available_bytes,
            output_estimate_bytes,
            &format!("output on {}", output.mount_point.display()),
        )?;
        ensure_available(
            scratch.available_bytes,
            scratch_estimate_bytes,
            &format!("scratch on {}", scratch.mount_point.display()),
        )?;
    }

    Ok(SpacePreflight {
        output_estimate_bytes,
        scratch_estimate_bytes,
        output_available_bytes: output.available_bytes,
        scratch_available_bytes: scratch.available_bytes,
        shared_volume,
    })
}

pub(crate) fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

fn volume_for(path: &Path) -> Result<VolumeSpace> {
    let ancestor = existing_ancestor(path)?;
    let disks = Disks::new_with_refreshed_list();
    let disk = disks
        .list()
        .iter()
        .filter(|disk| ancestor.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().components().count())
        .with_context(|| format!("find filesystem containing {}", ancestor.display()))?;
    Ok(VolumeSpace {
        mount_point: disk.mount_point().to_path_buf(),
        available_bytes: disk.available_space(),
    })
}

fn existing_ancestor(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .context("read current directory for disk-space preflight")?
            .join(path)
    };
    let mut candidate = absolute.as_path();
    while !candidate.exists() {
        candidate = candidate.parent().with_context(|| {
            format!("find existing ancestor for output path {}", path.display())
        })?;
    }
    candidate
        .canonicalize()
        .with_context(|| format!("resolve output filesystem for {}", path.display()))
}

fn ensure_available(available_bytes: u64, estimate_bytes: u64, label: &str) -> Result<()> {
    let headroom = (estimate_bytes / 10).max(MIN_HEADROOM_BYTES);
    let required = estimate_bytes
        .checked_add(headroom)
        .context("texture-upscale disk requirement overflow")?;
    if available_bytes < required {
        bail!(
            "insufficient disk space for {label}: need {} ({} estimate + {} headroom), but only {} is available; no files were changed",
            human_bytes(required),
            human_bytes(estimate_bytes),
            human_bytes(headroom),
            human_bytes(available_bytes),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insufficient_space_is_rejected_before_writes() {
        let error = ensure_available(1024, 2048, "test volume").unwrap_err();
        assert!(error.to_string().contains("no files were changed"));
    }

    #[test]
    fn byte_formatter_uses_binary_units() {
        assert_eq!(human_bytes(3 * 1024 * 1024), "3.0 MiB");
    }
}
