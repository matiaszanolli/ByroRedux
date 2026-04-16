//! BSA/BA2-backed texture and mesh extraction.

use byroredux_renderer::VulkanContext;

/// A game archive that can extract files by path.
/// Wraps either a BSA (Oblivion–Skyrim SE) or BA2 (FO4–Starfield) archive.
enum Archive {
    Bsa(byroredux_bsa::BsaArchive),
    Ba2(byroredux_bsa::Ba2Archive),
}

impl Archive {
    /// Open an archive file, auto-detecting BSA vs BA2 from the file magic.
    fn open(path: &str) -> Result<Self, String> {
        let magic = std::fs::read(path)
            .map_err(|e| format!("read '{}': {}", path, e))
            .and_then(|data| {
                if data.len() < 4 {
                    Err(format!("'{}' too small", path))
                } else {
                    Ok([data[0], data[1], data[2], data[3]])
                }
            })?;
        if &magic == b"BTDX" {
            byroredux_bsa::Ba2Archive::open(path)
                .map(Archive::Ba2)
                .map_err(|e| format!("BA2 '{}': {}", path, e))
        } else {
            byroredux_bsa::BsaArchive::open(path)
                .map(Archive::Bsa)
                .map_err(|e| format!("BSA '{}': {}", path, e))
        }
    }

    fn extract(&self, path: &str) -> Result<Vec<u8>, std::io::Error> {
        match self {
            Archive::Bsa(a) => a.extract(path),
            Archive::Ba2(a) => a.extract(path),
        }
    }
}

/// Provides file data by searching BSA/BA2 archives.
pub(crate) struct TextureProvider {
    texture_archives: Vec<Archive>,
    mesh_archives: Vec<Archive>,
}

impl TextureProvider {
    pub(crate) fn new() -> Self {
        Self {
            texture_archives: Vec::new(),
            mesh_archives: Vec::new(),
        }
    }

    /// Extract a texture (DDS) from texture archives.
    pub(crate) fn extract(&self, path: &str) -> Option<Vec<u8>> {
        for archive in &self.texture_archives {
            if let Ok(data) = archive.extract(path) {
                return Some(data);
            }
        }
        None
    }

    /// Extract a mesh (NIF) from mesh archives.
    pub(crate) fn extract_mesh(&self, path: &str) -> Option<Vec<u8>> {
        for archive in &self.mesh_archives {
            if let Ok(data) = archive.extract(path) {
                return Some(data);
            }
        }
        None
    }
}

/// Parse grid coordinates from a "x,y" string.
pub(crate) fn parse_grid_coords(s: &str) -> (i32, i32) {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() == 2 {
        let x = parts[0].trim().parse::<i32>().unwrap_or(0);
        let y = parts[1].trim().parse::<i32>().unwrap_or(0);
        (x, y)
    } else {
        log::warn!("Invalid grid format '{}', using (0,0)", s);
        (0, 0)
    }
}

/// Build a TextureProvider from CLI arguments.
pub(crate) fn build_texture_provider(args: &[String]) -> TextureProvider {
    let mut provider = TextureProvider::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--textures-bsa" => {
                if let Some(path) = args.get(i + 1) {
                    match Archive::open(path) {
                        Ok(a) => {
                            log::info!("Opened textures archive: '{}'", path);
                            provider.texture_archives.push(a);
                        }
                        Err(e) => log::warn!("Failed to open textures archive: {}", e),
                    }
                    i += 2;
                    continue;
                }
            }
            "--bsa" => {
                if let Some(path) = args.get(i + 1) {
                    match Archive::open(path) {
                        Ok(a) => {
                            log::info!("Opened mesh archive: '{}'", path);
                            provider.mesh_archives.push(a);
                        }
                        Err(e) => log::warn!("Failed to open mesh archive: {}", e),
                    }
                    i += 2;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    provider
}

/// Resolve a texture path to a texture handle, with BSA/BA2 lookup and caching.
pub(crate) fn resolve_texture(
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    tex_path: Option<&str>,
) -> u32 {
    let Some(tex_path) = tex_path else {
        return ctx.texture_registry.fallback();
    };
    if let Some(cached) = ctx.texture_registry.get_by_path(tex_path) {
        return cached;
    }
    if let Some(dds_bytes) = tex_provider.extract(tex_path) {
        let alloc = ctx.allocator.as_ref().unwrap();
        match ctx.texture_registry.load_dds(
            &ctx.device,
            alloc,
            &ctx.graphics_queue,
            ctx.transfer_pool,
            tex_path,
            &dds_bytes,
        ) {
            Ok(h) => {
                log::info!("Loaded DDS texture: '{}'", tex_path);
                return h;
            }
            Err(e) => {
                log::warn!("Failed to load DDS '{}': {}", tex_path, e);
            }
        }
    } else {
        log::debug!("Texture not found in archive: '{}'", tex_path);
    }
    ctx.texture_registry.fallback()
}
