//! Bounded, world-lifetime reuse of the DDS mip used for skin tone sampling.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use byroredux_core::ecs::{Resource, World};
use byroredux_renderer::vulkan::dds::{self, DdsMetadata};

use super::TONE_SAMPLE_TEXTURE_WIDTH;
use crate::asset_provider::TextureProvider;

const MAX_CACHE_BYTES: usize = 32 * 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 256;
type ToneTexture = (DdsMetadata, Vec<u8>);

#[derive(Default)]
struct ToneCache {
    textures: HashMap<String, Option<Arc<ToneTexture>>>,
    order: VecDeque<String>,
    bytes: usize,
}

impl ToneCache {
    fn get_or_load(
        &mut self,
        key: &str,
        load: impl FnOnce() -> Option<ToneTexture>,
    ) -> Option<Arc<ToneTexture>> {
        if let Some(texture) = self.textures.get(key) {
            return texture.clone();
        }
        let texture = load().map(Arc::new);
        let bytes = texture.as_ref().map_or(0, |t| t.1.len());
        // Oversized mip-less mod textures still sample correctly, but cannot
        // evict the whole cache or make its retained byte bound meaningless.
        if bytes > MAX_CACHE_BYTES {
            return texture;
        }
        while self.textures.len() >= MAX_CACHE_ENTRIES || self.bytes + bytes > MAX_CACHE_BYTES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(Some(old)) = self.textures.remove(&oldest) {
                self.bytes -= old.1.len();
            }
        }
        self.bytes += bytes;
        self.order.push_back(key.to_owned());
        self.textures.insert(key.to_owned(), texture.clone());
        texture
    }
}

#[derive(Default, Clone)]
struct SharedToneCache(Arc<Mutex<ToneCache>>);
impl Resource for SharedToneCache {}

pub(crate) struct ToneSampler<'a> {
    provider: &'a TextureProvider,
    shared: SharedToneCache,
    // A part touches only a few textures. Keep these references so individual
    // vertex samples neither lock the shared cache nor repeat archive reads.
    textures: HashMap<String, Option<Arc<ToneTexture>>>,
}

impl<'a> ToneSampler<'a> {
    pub(crate) fn new(world: &mut World, provider: &'a TextureProvider) -> Self {
        if world.try_resource::<SharedToneCache>().is_none() {
            world.insert_resource(SharedToneCache::default());
        }
        let shared = world.resource::<SharedToneCache>().clone();
        Self {
            provider,
            shared,
            textures: HashMap::new(),
        }
    }

    pub(crate) fn sample(&mut self, texture: &str, uv: [f32; 2]) -> Option<[f32; 3]> {
        let key = texture.replace('/', "\\").to_ascii_lowercase();
        let entry = self.textures.entry(key.clone()).or_insert_with(|| {
            self.shared
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_or_load(&key, || {
                    let data = self.provider.extract(texture)?;
                    let meta = dds::parse_dds(&data).ok()?;
                    tone_mip(meta, &data)
                })
        });
        let (meta, data) = entry.as_deref()?;
        dds::sample_region_rgb(meta, data, uv, TONE_SAMPLE_TEXTURE_WIDTH)
    }
}

/// Retain exactly the mip the original sampler would have read, preserving
/// BC blocks and sampling behavior while releasing the inflated high mips.
fn tone_mip(mut meta: DdsMetadata, data: &[u8]) -> Option<ToneTexture> {
    if meta.expand.is_some() || meta.width == 0 || meta.height == 0 {
        return None;
    }
    let mut mip = 0;
    let mut offset = meta.data_offset;
    while mip + 1 < meta.mip_count
        && dds::mip_dimension(meta.width, mip) > TONE_SAMPLE_TEXTURE_WIDTH.max(4)
    {
        offset = offset.checked_add(
            usize::try_from(dds::mip_size(
                meta.width,
                meta.height,
                mip,
                meta.block_size,
                meta.compressed,
            ))
            .ok()?,
        )?;
        mip += 1;
    }
    let size = usize::try_from(dds::mip_size(
        meta.width,
        meta.height,
        mip,
        meta.block_size,
        meta.compressed,
    ))
    .ok()?;
    let bytes = data.get(offset..offset.checked_add(size)?)?.to_vec();
    meta.width = dds::mip_dimension(meta.width, mip);
    meta.height = dds::mip_dimension(meta.height, mip);
    meta.mip_count = 1;
    meta.data_offset = 0;
    meta.array_layers = 1;
    meta.is_cubemap = false;
    Some((meta, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mip_fixture() -> (DdsMetadata, Vec<u8>) {
        // Uncompressed 512x512 and 256x256 mips with deliberately distinct
        // colours, so sampling the wrong level cannot accidentally pass.
        let meta = DdsMetadata {
            width: 512,
            height: 512,
            mip_count: 2,
            format: ash::vk::Format::R8G8B8A8_UNORM,
            block_size: 4,
            compressed: false,
            array_layers: 1,
            is_cubemap: false,
            data_offset: 0,
            expand: None,
        };
        let mut bytes = vec![255; 512 * 512 * 4];
        for _ in 0..256 * 256 {
            bytes.extend_from_slice(&[32, 128, 224, 255]);
        }
        (meta, bytes)
    }

    #[test]
    fn retained_mip_matches_full_dds_samples_and_reuses_its_allocation() {
        let (meta, bytes) = mip_fixture();
        let (small_meta, small_bytes) = tone_mip(meta.clone(), &bytes).unwrap();
        assert_eq!(small_bytes.len(), 256 * 256 * 4);
        for uv in [[0.0, 0.0], [0.5, 0.9], [-0.01, 1.1]] {
            assert_eq!(
                dds::sample_region_rgb(&meta, &bytes, uv, TONE_SAMPLE_TEXTURE_WIDTH),
                dds::sample_region_rgb(&small_meta, &small_bytes, uv, TONE_SAMPLE_TEXTURE_WIDTH)
            );
        }
        let mut cache = ToneCache::default();
        let first = cache
            .get_or_load("skin", || Some((small_meta, small_bytes)))
            .unwrap();
        let second = cache
            .get_or_load("skin", || panic!("repeated archive extraction"))
            .unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(cache.bytes, first.1.len());
        assert!(tone_mip(meta, &bytes[..20]).is_none());
    }

    #[test]
    fn negative_results_are_shared_and_the_entry_bound_is_enforced() {
        let mut cache = ToneCache::default();
        assert!(cache.get_or_load("missing", || None).is_none());
        assert!(cache
            .get_or_load("missing", || panic!("second extraction"))
            .is_none());
        for i in 0..MAX_CACHE_ENTRIES {
            cache.get_or_load(&i.to_string(), || None);
        }
        assert_eq!(cache.textures.len(), MAX_CACHE_ENTRIES);
        assert!(!cache.textures.contains_key("missing"));
    }
}
