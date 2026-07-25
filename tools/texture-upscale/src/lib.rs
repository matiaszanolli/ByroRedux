//! Archive-aware, reference-guided texture-set upscaling.
//!
//! The learned upscaler is deliberately an external process. This crate owns
//! the deterministic parts of the pipeline: texture-set discovery, source
//! extraction, command construction, semantic map upsampling, and reporting.

mod guided;
mod pipeline;
mod source;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub use pipeline::{run_manifest, RunOptions, RunReport};
pub use source::SourceStack;

pub const MANIFEST_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Manifest {
    pub version: u32,
    pub scale: u32,
    #[serde(default = "default_guide_sigma")]
    pub guide_sigma: f64,
    pub upscaler: CommandSpec,
    #[serde(default)]
    pub sets: Vec<TextureSet>,
}

impl Manifest {
    pub fn discovered(scale: u32, paths: impl IntoIterator<Item = String>) -> Self {
        Self {
            version: MANIFEST_VERSION,
            scale,
            guide_sigma: default_guide_sigma(),
            upscaler: CommandSpec::default_realesrgan(),
            sets: discover_sets(paths),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != MANIFEST_VERSION {
            bail!(
                "unsupported manifest version {} (expected {})",
                self.version,
                MANIFEST_VERSION
            );
        }
        if !(2..=8).contains(&self.scale) {
            bail!("scale must be in 2..=8, got {}", self.scale);
        }
        if !self.guide_sigma.is_finite() || self.guide_sigma <= 0.0 {
            bail!(
                "guide_sigma must be finite and positive, got {}",
                self.guide_sigma
            );
        }
        if self.upscaler.program.trim().is_empty() {
            bail!("upscaler.program must not be empty");
        }
        if !self.upscaler.args.iter().any(|arg| arg.contains("{input}"))
            || !self
                .upscaler
                .args
                .iter()
                .any(|arg| arg.contains("{output}"))
        {
            bail!("upscaler.args must contain both {{input}} and {{output}} placeholders");
        }

        let mut names = BTreeSet::new();
        let mut references = BTreeSet::new();
        for set in &self.sets {
            if set.name.trim().is_empty() {
                bail!("texture-set name must not be empty");
            }
            if !names.insert(set.name.as_str()) {
                bail!("duplicate texture-set name {:?}", set.name);
            }
            validate_asset_path(&set.reference)?;
            if !references.insert(normalize_asset_path(&set.reference)) {
                bail!("duplicate texture-set reference {:?}", set.reference);
            }
            let mut map_paths = BTreeSet::new();
            for map in &set.maps {
                validate_asset_path(&map.path)?;
                let map_path = normalize_asset_path(&map.path);
                if map_path == normalize_asset_path(&set.reference) {
                    bail!(
                        "map path {:?} duplicates the reference in set {:?}",
                        map.path,
                        set.name
                    );
                }
                if !map_paths.insert(map_path) {
                    bail!("duplicate map path {:?} in set {:?}", map.path, set.name);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

impl CommandSpec {
    pub fn default_realesrgan() -> Self {
        Self {
            program: "realesrgan-ncnn-vulkan".to_string(),
            args: vec![
                "-i".to_string(),
                "{input}".to_string(),
                "-o".to_string(),
                "{output}".to_string(),
                "-s".to_string(),
                "{scale}".to_string(),
            ],
        }
    }

    pub fn expanded_args(&self, input: &Path, output: &Path, scale: u32) -> Vec<String> {
        let input = input.to_string_lossy();
        let output = output.to_string_lossy();
        let scale = scale.to_string();
        self.args
            .iter()
            .map(|arg| {
                arg.replace("{input}", &input)
                    .replace("{output}", &output)
                    .replace("{scale}", &scale)
            })
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct TextureSet {
    pub name: String,
    pub reference: String,
    #[serde(default)]
    pub maps: Vec<TextureMap>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct TextureMap {
    pub role: MapRole,
    pub path: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MapRole {
    Normal,
    Glow,
    Specular,
    Mask,
    Height,
}

fn default_guide_sigma() -> f64 {
    0.12
}

pub fn load_manifest(path: &Path) -> Result<Manifest> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read manifest {}", path.display()))?;
    let manifest: Manifest =
        toml::from_str(&text).with_context(|| format!("parse manifest {}", path.display()))?;
    manifest.validate()?;
    Ok(manifest)
}

pub fn save_manifest(path: &Path, manifest: &Manifest) -> Result<()> {
    manifest.validate()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create manifest directory {}", parent.display()))?;
    }
    let text = toml::to_string_pretty(manifest).context("serialize texture manifest")?;
    std::fs::write(path, text).with_context(|| format!("write manifest {}", path.display()))
}

pub fn normalize_asset_path(path: &str) -> String {
    path.trim()
        .trim_start_matches("./")
        .trim_start_matches(".\\")
        .replace('\\', "/")
        .to_ascii_lowercase()
}

pub fn output_png_path(root: &Path, asset_path: &str) -> Result<PathBuf> {
    validate_asset_path(asset_path)?;
    let relative = normalize_asset_path(asset_path);
    let mut output = root.to_path_buf();
    for component in relative.split('/') {
        output.push(component);
    }
    output.set_extension("png");
    Ok(output)
}

fn validate_asset_path(path: &str) -> Result<()> {
    let normalized = normalize_asset_path(path);
    if normalized.is_empty() {
        bail!("asset path must not be empty");
    }
    if Path::new(&normalized).is_absolute()
        || normalized
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        bail!("asset path must be a safe relative path, got {:?}", path);
    }
    Ok(())
}

fn discover_sets(paths: impl IntoIterator<Item = String>) -> Vec<TextureSet> {
    let paths: BTreeSet<String> = paths
        .into_iter()
        .map(|path| normalize_asset_path(&path))
        .filter(|path| supported_image_path(path))
        .collect();

    let mut stems = BTreeMap::<(String, String), String>::new();
    for path in &paths {
        if let Some((parent, stem)) = parent_and_stem(path) {
            stems.insert((parent, stem), path.clone());
        }
    }

    let mut sets = BTreeMap::<String, TextureSet>::new();
    for path in &paths {
        let Some((parent, stem)) = parent_and_stem(path) else {
            continue;
        };
        let Some((base_stem, role)) = classify_companion_stem(&stem) else {
            continue;
        };
        let Some(reference) = stems.get(&(parent.clone(), base_stem.clone())) else {
            continue;
        };
        let key = reference.clone();
        let set = sets.entry(key).or_insert_with(|| TextureSet {
            name: strip_extension(reference),
            reference: reference.clone(),
            maps: Vec::new(),
        });
        set.maps.push(TextureMap {
            role,
            path: path.clone(),
        });
    }

    for set in sets.values_mut() {
        set.maps.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then_with(|| role_rank(a.role).cmp(&role_rank(b.role)))
        });
    }
    sets.into_values().collect()
}

fn supported_image_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("dds" | "png" | "tga" | "bmp" | "jpg" | "jpeg")
    )
}

fn parent_and_stem(path: &str) -> Option<(String, String)> {
    let path = Path::new(path);
    let stem = path.file_stem()?.to_str()?.to_ascii_lowercase();
    let parent = path
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or_default()
        .replace('\\', "/")
        .to_ascii_lowercase();
    Some((parent, stem))
}

fn classify_companion_stem(stem: &str) -> Option<(String, MapRole)> {
    [
        ("_n", MapRole::Normal),
        ("_g", MapRole::Glow),
        ("_s", MapRole::Specular),
        ("_m", MapRole::Mask),
        ("_p", MapRole::Height),
    ]
    .into_iter()
    .find_map(|(suffix, role)| {
        stem.strip_suffix(suffix)
            .filter(|base| !base.is_empty())
            .map(|base| (base.to_string(), role))
    })
}

fn strip_extension(path: &str) -> String {
    Path::new(path)
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/")
}

fn role_rank(role: MapRole) -> u8 {
    match role {
        MapRole::Normal => 0,
        MapRole::Glow => 1,
        MapRole::Specular => 2,
        MapRole::Mask => 3,
        MapRole::Height => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_groups_only_unambiguous_companions() {
        let manifest = Manifest::discovered(
            4,
            [
                "textures/clutter/wood.dds".to_string(),
                "textures/clutter/wood_n.dds".to_string(),
                "textures/clutter/wood_g.dds".to_string(),
                "textures/clutter/wood_d.dds".to_string(),
                "textures/clutter/orphan_n.dds".to_string(),
            ],
        );
        assert_eq!(manifest.sets.len(), 1);
        assert_eq!(manifest.sets[0].reference, "textures/clutter/wood.dds");
        assert_eq!(
            manifest.sets[0].maps,
            vec![
                TextureMap {
                    role: MapRole::Glow,
                    path: "textures/clutter/wood_g.dds".to_string(),
                },
                TextureMap {
                    role: MapRole::Normal,
                    path: "textures/clutter/wood_n.dds".to_string(),
                },
            ]
        );
    }

    #[test]
    fn command_expansion_does_not_invoke_a_shell() {
        let spec = CommandSpec::default_realesrgan();
        assert_eq!(
            spec.expanded_args(Path::new("/tmp/in image.png"), Path::new("/tmp/out.png"), 4),
            vec!["-i", "/tmp/in image.png", "-o", "/tmp/out.png", "-s", "4"]
        );
    }

    #[test]
    fn output_paths_cannot_escape_the_output_root() {
        assert!(output_png_path(Path::new("/tmp/out"), "../escape.dds").is_err());
        assert_eq!(
            output_png_path(Path::new("/tmp/out"), "textures\\wood.dds").unwrap(),
            Path::new("/tmp/out/textures/wood.png")
        );
    }

    #[test]
    fn manifest_rejects_missing_command_placeholders() {
        let mut manifest = Manifest::discovered(4, Vec::new());
        manifest.upscaler.args = vec!["literal".to_string()];
        assert!(manifest.validate().is_err());
    }
}
