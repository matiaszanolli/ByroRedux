//! Per-terrain-layer height maps: the translation half (#6195).
//!
//! `byroredux-terrain-height` turns one texture into a height map and knows
//! nothing about games. This module decides, per LAND layer, *which* texture
//! that is and how to read it:
//!
//! 1. the layer's authored height map — `TXST.TX04`, or a BGSM's
//!    `displacement_texture` once #6199 fills it — so a texture set that ships
//!    one, vanilla or mod, wins outright;
//! 2. its normal map, integrated, read with the game's green convention;
//! 3. its diffuse's high-passed brightness.
//!
//! Generation is pure CPU and runs off the main thread ([`run_job`]). Results
//! are cached on disk keyed by a hash of the source texture bytes, so a
//! texture is generated once per machine, and a mod that replaces the source
//! (a different archive copy, a loose override #6200) gets its own entry.

use super::{extract_first, Archive};
use byroredux_menuxml::tex::Rgba8;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_terrain_height as th;
use std::path::{Path, PathBuf};

/// The textures one terrain layer's height can come from, each an
/// archive-canonical path, in [`run_job`]'s priority order. Also the key the
/// service tracks a layer's height by, so two layers painting the same
/// texture set share one map.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct LayerHeightSources {
    pub(crate) authored: Option<String>,
    pub(crate) normal: Option<String>,
    pub(crate) diffuse: Option<String>,
}

impl LayerHeightSources {
    /// `None` when there is nothing to generate from at all.
    pub(crate) fn new(
        authored: Option<&str>,
        normal: Option<&str>,
        diffuse: Option<&str>,
    ) -> Option<Self> {
        let canon = |p: Option<&str>| {
            p.filter(|p| !p.is_empty())
                .map(super::canonical_texture_key)
        };
        let sources = Self {
            authored: canon(authored),
            normal: canon(normal),
            diffuse: canon(diffuse),
        };
        (sources.authored.is_some() || sources.normal.is_some() || sources.diffuse.is_some())
            .then_some(sources)
    }
}

/// The green-channel convention a game reads its tangent-space normal maps
/// with. Measured, not assumed: integrating every vanilla LAND normal map
/// (`crates/terrain-height/examples/layer_census.rs`, 2026-10-20) and letting
/// the integrability test pick per map chose green-down on 159/160 Oblivion,
/// 65/66 Fallout 3, 73/75 New Vegas, 65/66 Skyrim SE, 46/46 Skyrim LE and
/// 54/56 Fallout 4 layers; every dissenter was a low-decisiveness map
/// (other/chosen curl ≤ 1.61). A game not measured here returns `None`, and
/// each map's own test decides.
pub(crate) fn normal_map_green(game: GameKind) -> Option<th::GreenConvention> {
    match game {
        GameKind::Oblivion | GameKind::Fallout3NV | GameKind::Skyrim | GameKind::Fallout4 => {
            Some(th::GreenConvention::YDown)
        }
        _ => None,
    }
}

/// A generated (or cache-loaded) height map, ready to upload.
#[derive(Debug, Clone)]
pub(crate) struct CachedHeight {
    /// Texture-registry key: unique per generated map, never an archive path.
    pub(crate) registry_key: String,
    /// The `.dds` on disk the upload reads.
    pub(crate) dds_path: PathBuf,
    /// Which archive texture the map came from.
    pub(crate) source_path: String,
    pub(crate) meta: th::HeightMeta,
}

/// Where generated maps live: `terrain-height/` next to the executable,
/// beside the Vulkan pipeline cache, so it survives working-directory changes.
/// `BYRO_CACHE_DIR` overrides the parent (tests, read-only installs).
pub(crate) fn default_cache_dir() -> PathBuf {
    let root = std::env::var_os("BYRO_CACHE_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(Path::to_path_buf))
        })
        .unwrap_or_else(|| PathBuf::from("."));
    root.join("terrain-height")
}

/// One unit of background work: everything [`run_job`] needs, owned, so it
/// can run on a pool thread. `archives` is a snapshot of the texture pool
/// (cheap `Arc` clones) in provider order — last-listed wins, loose on top.
pub(crate) struct HeightJob {
    pub(crate) sources: LayerHeightSources,
    pub(crate) green: Option<th::GreenConvention>,
    pub(crate) archives: Vec<Archive>,
    pub(crate) cache_dir: PathBuf,
}

/// Run one job: pick the first source tier whose texture is present and
/// decodable, then load its map from the cache or generate and store it.
pub(crate) fn run_job(job: &HeightJob) -> Result<CachedHeight, String> {
    let tiers = [
        (th::HeightSource::Authored, job.sources.authored.as_deref()),
        (th::HeightSource::Normals, job.sources.normal.as_deref()),
        (th::HeightSource::Brightness, job.sources.diffuse.as_deref()),
    ];
    let mut skipped = Vec::new();
    for (source, path) in tiers {
        let Some(path) = path else { continue };
        let Some(bytes) = extract_first(job.archives.iter().rev(), path) else {
            skipped.push(format!("{path}: not in any archive"));
            continue;
        };
        let Some(image) = Rgba8::decode_dds(&bytes) else {
            // BC6H/BC7 and exotic formats: fall through to the next tier.
            skipped.push(format!("{path}: undecodable DDS"));
            continue;
        };
        return cached_or_generate(job, source, path, &bytes, &image);
    }
    Err(format!("no usable height source ({})", skipped.join("; ")))
}

/// Cache key: everything the output depends on — the generator version, the
/// tier, the convention it was read with, and the source bytes themselves.
fn cache_key(source: th::HeightSource, green: Option<th::GreenConvention>, bytes: &[u8]) -> String {
    use std::hash::Hasher;
    let mut hasher = twox_hash::XxHash64::with_seed(0);
    hasher.write_u32(th::GENERATOR_VERSION);
    hasher.write_u8(match source {
        th::HeightSource::Authored => 0,
        th::HeightSource::Normals => 1,
        th::HeightSource::Brightness => 2,
    });
    hasher.write_u8(match (source, green) {
        (th::HeightSource::Normals, Some(th::GreenConvention::YUp)) => 1,
        (th::HeightSource::Normals, Some(th::GreenConvention::YDown)) => 2,
        _ => 0,
    });
    hasher.write(bytes);
    format!("{:016x}", hasher.finish())
}

fn cached_or_generate(
    job: &HeightJob,
    source: th::HeightSource,
    source_path: &str,
    bytes: &[u8],
    image: &Rgba8,
) -> Result<CachedHeight, String> {
    let key = cache_key(source, job.green, bytes);
    let dds_path = job.cache_dir.join(format!("{key}.dds"));
    let meta_path = job.cache_dir.join(format!("{key}.json"));
    let cached = || -> Option<th::HeightMeta> {
        let meta: th::HeightMeta = serde_json::from_slice(&std::fs::read(&meta_path).ok()?).ok()?;
        (dds_path.is_file() && meta.generator_version == th::GENERATOR_VERSION).then_some(meta)
    };
    let meta = match cached() {
        Some(meta) => meta,
        None => {
            let view = th::Rgba {
                width: image.width as usize,
                height: image.height as usize,
                pixels: &image.pixels,
            };
            let generated = match source {
                th::HeightSource::Authored => th::from_authored(&view),
                th::HeightSource::Normals => th::from_normal_map(&view, job.green),
                th::HeightSource::Brightness => th::from_brightness(&view),
            };
            std::fs::create_dir_all(&job.cache_dir)
                .map_err(|e| format!("create {}: {e}", job.cache_dir.display()))?;
            // Data first, then metadata, each through a rename: a reader that
            // finds the `.json` can rely on the `.dds` beside it being whole.
            write_atomic(&dds_path, &generated.dds)?;
            let json = serde_json::to_vec_pretty(&generated.meta).map_err(|e| e.to_string())?;
            write_atomic(&meta_path, &json)?;
            generated.meta
        }
    };
    Ok(CachedHeight {
        registry_key: format!("byro-generated\\terrain-height\\{key}.dds"),
        dds_path,
        source_path: source_path.to_string(),
        meta,
    })
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("rename to {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loose_archive(dir: &Path, files: &[(&str, Vec<u8>)]) -> Archive {
        for (rel, bytes) in files {
            let path = dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        Archive::open_loose(dir).expect("fixture has loose textures")
    }

    /// A minimal uncompressed 32-bit BGRA DDS of `w × h` filled by `px`.
    fn bgra_dds(w: u32, h: u32, px: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[0..4].copy_from_slice(b"DDS ");
        b[4..8].copy_from_slice(&124u32.to_le_bytes());
        b[12..16].copy_from_slice(&h.to_le_bytes());
        b[16..20].copy_from_slice(&w.to_le_bytes());
        b[76..80].copy_from_slice(&32u32.to_le_bytes());
        b[80..84].copy_from_slice(&0x41u32.to_le_bytes());
        b[88..92].copy_from_slice(&32u32.to_le_bytes());
        for (i, m) in [0x00FF_0000u32, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000]
            .iter()
            .enumerate()
        {
            b[92 + i * 4..96 + i * 4].copy_from_slice(&m.to_le_bytes());
        }
        for y in 0..h {
            for x in 0..w {
                let [r, g, bl, a] = px(x, y);
                b.extend_from_slice(&[bl, g, r, a]);
            }
        }
        b
    }

    fn job(dir: &Path, archive: Archive, sources: LayerHeightSources) -> HeightJob {
        HeightJob {
            sources,
            green: Some(th::GreenConvention::YDown),
            archives: vec![archive],
            cache_dir: dir.join("cache"),
        }
    }

    /// The tiers apply in order, a missing tier falls through, and the
    /// source is recorded.
    #[test]
    fn source_tiers_apply_in_priority_order() {
        let tmp = tempfile::tempdir().unwrap();
        let wave = |x: u32, _y: u32| {
            let v = (128.0 + 100.0 * (x as f32 * 0.4).sin()) as u8;
            [v, v, v, 255]
        };
        let archive = loose_archive(
            tmp.path(),
            &[
                ("textures/landscape/dirt_d.dds", bgra_dds(16, 16, wave)),
                (
                    "textures/landscape/dirt_n.dds",
                    bgra_dds(16, 16, |_, _| [128, 128, 255, 255]),
                ),
                ("textures/landscape/dirt_p.dds", bgra_dds(16, 16, wave)),
            ],
        );
        let all = LayerHeightSources::new(
            Some(r"landscape\dirt_p.dds"),
            Some(r"landscape\dirt_n.dds"),
            Some(r"landscape\dirt_d.dds"),
        )
        .unwrap();
        let out = run_job(&job(tmp.path(), archive.clone(), all.clone())).unwrap();
        assert_eq!(out.meta.source, th::HeightSource::Authored);
        assert_eq!(out.source_path, r"textures\landscape\dirt_p.dds");

        // An authored path that ships nowhere falls through to the normals.
        let missing_authored = LayerHeightSources {
            authored: Some(r"textures\landscape\gone_p.dds".into()),
            ..all.clone()
        };
        let out = run_job(&job(tmp.path(), archive.clone(), missing_authored)).unwrap();
        assert_eq!(out.meta.source, th::HeightSource::Normals);
        assert_eq!(out.meta.green, Some(th::GreenConvention::YDown));

        let diffuse_only =
            LayerHeightSources::new(None, None, Some(r"landscape\dirt_d.dds")).unwrap();
        let out = run_job(&job(tmp.path(), archive, diffuse_only)).unwrap();
        assert_eq!(out.meta.source, th::HeightSource::Brightness);
        assert!(LayerHeightSources::new(None, None, None).is_none());
    }

    /// Generation runs once per source: the second run reads the cache, and
    /// a changed source (a mod's copy) gets a different entry.
    #[test]
    fn cache_is_keyed_by_source_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = loose_archive(
            tmp.path(),
            &[(
                "textures/a_n.dds",
                bgra_dds(8, 8, |x, _| [100 + x as u8 * 8, 128, 255, 255]),
            )],
        );
        let sources = LayerHeightSources::new(None, Some(r"textures\a_n.dds"), None).unwrap();
        let first = run_job(&job(tmp.path(), archive.clone(), sources.clone())).unwrap();
        let stamp = std::fs::metadata(&first.dds_path)
            .unwrap()
            .modified()
            .unwrap();
        let second = run_job(&job(tmp.path(), archive, sources.clone())).unwrap();
        assert_eq!(first.registry_key, second.registry_key);
        assert_eq!(
            std::fs::metadata(&second.dds_path)
                .unwrap()
                .modified()
                .unwrap(),
            stamp
        );

        let other = tempfile::tempdir().unwrap();
        let modded = loose_archive(
            other.path(),
            &[(
                "textures/a_n.dds",
                bgra_dds(8, 8, |_, y| [128, 100 + y as u8 * 8, 255, 255]),
            )],
        );
        let third = run_job(&job(tmp.path(), modded, sources)).unwrap();
        assert_ne!(third.registry_key, first.registry_key);
    }
}
