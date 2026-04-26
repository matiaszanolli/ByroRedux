//! Template inheritance for BGSM files.
//!
//! Every creature BGSM in vanilla Fallout 4 ships a non-empty
//! `root_material_path` pointing at a shared template
//! (`template/CreatureTemplate_Wet.bgsm` and friends). Naively walking
//! the chain on every lookup dominates cell-load time — so this
//! module caches resolved chains in an LRU.
//!
//! # Usage
//!
//! ```no_run
//! use byroredux_bgsm::{TemplateCache, TemplateResolver};
//! use std::collections::HashMap;
//!
//! struct InMemoryResolver(HashMap<String, Vec<u8>>);
//! impl TemplateResolver for InMemoryResolver {
//!     fn read(&mut self, path: &str) -> Option<Vec<u8>> {
//!         self.0.get(&path.to_ascii_lowercase()).cloned()
//!     }
//! }
//!
//! let mut cache = TemplateCache::new(256);
//! let mut resolver = InMemoryResolver(HashMap::new());
//! // cache.resolve(&mut resolver, "materials/foo.bgsm"); // returns Arc<ResolvedMaterial>
//! ```

use crate::{parse_bgsm, BgsmFile};
use std::collections::HashMap;
use std::sync::Arc;

/// Caller-supplied BGSM file opener. The parser crate is intentionally
/// filesystem-agnostic — integrations wrap Materials.ba2 extraction,
/// loose-file lookup, or a test-harness HashMap.
pub trait TemplateResolver {
    /// Return the raw bytes of a BGSM file referenced by its
    /// case-insensitive path. `None` means "not found"; the resolver
    /// returns the empty body as `Some(vec![])` if the archive does
    /// contain an empty file.
    fn read(&mut self, path: &str) -> Option<Vec<u8>>;
}

/// A fully-resolved BGSM — if the parsed file had a non-empty
/// `root_material_path`, `parent` points at its (recursively resolved)
/// template. Callers that want merged semantics can walk the chain
/// via [`ResolvedMaterial::walk`].
#[derive(Debug, Clone)]
pub struct ResolvedMaterial {
    pub file: BgsmFile,
    pub parent: Option<Arc<ResolvedMaterial>>,
}

impl ResolvedMaterial {
    /// Depth — 1 for a leaf BGSM, N for a BGSM with N-1 levels of
    /// parents.
    pub fn depth(&self) -> usize {
        1 + self.parent.as_deref().map(Self::depth).unwrap_or(0)
    }

    /// Iterate from self up through every ancestor, child-first.
    /// Useful for "child wins" merge semantics — the first `Some`
    /// encountered for any field wins.
    pub fn walk(&self) -> WalkIter<'_> {
        WalkIter { cursor: Some(self) }
    }
}

/// Iterator returned by [`ResolvedMaterial::walk`].
pub struct WalkIter<'a> {
    cursor: Option<&'a ResolvedMaterial>,
}

impl<'a> Iterator for WalkIter<'a> {
    type Item = &'a ResolvedMaterial;

    fn next(&mut self) -> Option<Self::Item> {
        let out = self.cursor?;
        self.cursor = out.parent.as_deref();
        Some(out)
    }
}

/// Errors from [`TemplateCache::resolve`].
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("template '{path}' could not be opened by the resolver")]
    NotFound { path: String },

    #[error("parse failed for '{path}': {source}")]
    Parse {
        path: String,
        #[source]
        source: crate::Error,
    },

    /// A chain longer than the depth limit — most likely a cycle
    /// (`A → B → A → B → …`) or a pathological test file. Default limit
    /// is 16; vanilla chains top out at 3.
    #[error("template chain exceeded depth limit {limit} at '{path}' (likely a cycle)")]
    DepthLimit { path: String, limit: usize },
}

/// LRU cache of resolved template chains. Keyed by lowercase path.
///
/// Eviction is insertion-order: the oldest key is dropped when the
/// cache exceeds `capacity`. Good enough for the template use case —
/// the hot set (creature templates, weapon templates) stays warm, and
/// transitive one-off leafs get evicted first.
pub struct TemplateCache {
    capacity: usize,
    /// Keyed by lowercase path. Each entry's `parent` points at
    /// another resolved chain that may or may not still be in the
    /// cache — `Arc` makes that safe.
    entries: HashMap<String, Arc<ResolvedMaterial>>,
    /// Insertion order for LRU eviction. `String` is duplicated with
    /// `entries.key()` — minor memory cost bought simplicity.
    order: Vec<String>,
}

impl TemplateCache {
    /// Create a cache with `capacity` entries. A reasonable default for
    /// FO4 cell loads is 256 — large enough that every creature /
    /// weapon / clothing template stays warm across a cell.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: HashMap::new(),
            order: Vec::new(),
        }
    }

    /// Number of entries currently cached. For telemetry.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Resolve a BGSM + its template chain. The same path is guaranteed
    /// to return `Arc::ptr_eq`-identical results between calls as long
    /// as the entry stays in the cache.
    pub fn resolve<R: TemplateResolver>(
        &mut self,
        resolver: &mut R,
        path: &str,
    ) -> Result<Arc<ResolvedMaterial>, ResolveError> {
        const DEPTH_LIMIT: usize = 16;
        self.resolve_depth(resolver, path, DEPTH_LIMIT)
    }

    fn resolve_depth<R: TemplateResolver>(
        &mut self,
        resolver: &mut R,
        path: &str,
        remaining: usize,
    ) -> Result<Arc<ResolvedMaterial>, ResolveError> {
        let key = path.to_ascii_lowercase();

        if let Some(hit) = self.entries.get(&key) {
            return Ok(Arc::clone(hit));
        }

        if remaining == 0 {
            return Err(ResolveError::DepthLimit {
                path: key,
                limit: 16,
            });
        }

        let bytes = resolver
            .read(&key)
            .ok_or_else(|| ResolveError::NotFound { path: key.clone() })?;

        let file = parse_bgsm(&bytes).map_err(|source| ResolveError::Parse {
            path: key.clone(),
            source,
        })?;

        // Capture the parent path BEFORE moving `file` into the Arc.
        let parent_path = file.root_material_path.clone();
        let parent = match parent_path {
            Some(pp) if !pp.is_empty() => Some(self.resolve_depth(resolver, &pp, remaining - 1)?),
            _ => None,
        };

        let resolved = Arc::new(ResolvedMaterial { file, parent });
        self.insert(key, Arc::clone(&resolved));
        Ok(resolved)
    }

    fn insert(&mut self, key: String, value: Arc<ResolvedMaterial>) {
        if self.entries.contains_key(&key) {
            // Refresh recency — pull the key to the back.
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                self.order.remove(pos);
            }
        } else if self.entries.len() >= self.capacity {
            // Evict the oldest.
            if let Some(evict) = self.order.first().cloned() {
                self.order.remove(0);
                self.entries.remove(&evict);
            }
        }
        self.order.push(key.clone());
        self.entries.insert(key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bgsm::tests::minimal_v2_bytes;

    struct StubResolver {
        files: HashMap<String, Vec<u8>>,
        read_count: usize,
    }

    impl StubResolver {
        fn new() -> Self {
            Self {
                files: HashMap::new(),
                read_count: 0,
            }
        }

        fn add(&mut self, path: &str, bytes: Vec<u8>) {
            self.files.insert(path.to_ascii_lowercase(), bytes);
        }
    }

    impl TemplateResolver for StubResolver {
        fn read(&mut self, path: &str) -> Option<Vec<u8>> {
            self.read_count += 1;
            self.files.get(&path.to_ascii_lowercase()).cloned()
        }
    }

    /// Minimal-v2 BGSM fixture with a specified `root_material_path`
    /// string patched in. We rebuild from scratch rather than mutating
    /// bytes because the length-prefix ripples through downstream offsets.
    fn bgsm_with_template(template: &str) -> Vec<u8> {
        let mut bytes = minimal_v2_bytes();
        // Locate the empty root_material_path (4 bytes of zeros in the
        // minimal fixture, after the post-wetness floats). Too fragile
        // to splice — instead, use a fresh builder. For the test we
        // just need a parseable file whose root_material_path matches.
        if template.is_empty() {
            return bytes;
        }
        // Find the `0u32` root_material_path slot. In minimal_v2_bytes
        // it sits after the 7 wetness/fresnel floats following
        // smoothness/fresnel_power. Rather than recompute the offset,
        // grep the buffer for the known sequence `[0, 0, 0, 0]` that
        // immediately precedes the aniso_lighting bool (which is `0`).
        //
        // Safer: just build a custom fixture inline.
        bytes.clear();
        build_bgsm_with_template(&mut bytes, template);
        bytes
    }

    fn build_bgsm_with_template(buf: &mut Vec<u8>, template: &str) {
        // Identical to minimal_v2_bytes but with the template path
        // injected at the root_material_path slot.
        use crate::bgsm::SIGNATURE;

        let append_string = |buf: &mut Vec<u8>, s: &str| {
            if s.is_empty() {
                buf.extend_from_slice(&0u32.to_le_bytes());
                return;
            }
            let bytes = s.as_bytes();
            let len = bytes.len() as u32 + 1;
            buf.extend_from_slice(&len.to_le_bytes());
            buf.extend_from_slice(bytes);
            buf.push(0);
        };

        buf.extend_from_slice(&SIGNATURE.to_le_bytes());
        crate::base::tests::append_base_v2(buf, 2);

        // Texture slots (v <= 2 uses legacy 5-texture layout).
        for _ in 0..4 {
            append_string(buf, "");
        }
        for _ in 0..5 {
            append_string(buf, "");
        }

        buf.push(0); // enable_editor_alpha_ref

        // v < 8 branch
        buf.push(0);
        buf.extend_from_slice(&2.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.push(0);
        buf.extend_from_slice(&0.3f32.to_le_bytes());

        buf.push(0);
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());

        buf.extend_from_slice(&5.0f32.to_le_bytes());
        for _ in 0..6 {
            buf.extend_from_slice(&(-1.0f32).to_le_bytes());
        }

        append_string(buf, template);

        buf.push(0);
        buf.push(0);
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.push(0);
        buf.push(0);
        buf.push(0);

        for _ in 0..5 {
            buf.push(0);
        }
        buf.push(0);
        buf.push(0);
        buf.push(0);

        buf.push(0);
        buf.extend_from_slice(&0.5f32.to_le_bytes());
        buf.extend_from_slice(&0.5f32.to_le_bytes());
        buf.extend_from_slice(&0.5f32.to_le_bytes());

        for _ in 0..4 {
            buf.push(0);
        }
        for _ in 0..5 {
            buf.extend_from_slice(&0.0f32.to_le_bytes());
        }

        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.push(0);
    }

    #[test]
    fn resolve_single_leaf_no_template() {
        let mut resolver = StubResolver::new();
        resolver.add("materials/leaf.bgsm", minimal_v2_bytes());

        let mut cache = TemplateCache::new(16);
        let r = cache.resolve(&mut resolver, "materials/leaf.bgsm").unwrap();
        assert!(r.parent.is_none());
        assert_eq!(r.depth(), 1);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn resolve_three_level_chain() {
        let mut resolver = StubResolver::new();
        resolver.add("materials/grandparent.bgsm", bgsm_with_template(""));
        resolver.add(
            "materials/parent.bgsm",
            bgsm_with_template("materials/grandparent.bgsm"),
        );
        resolver.add(
            "materials/child.bgsm",
            bgsm_with_template("materials/parent.bgsm"),
        );

        let mut cache = TemplateCache::new(16);
        let r = cache
            .resolve(&mut resolver, "materials/child.bgsm")
            .unwrap();
        assert_eq!(r.depth(), 3);

        // walk() iterates child → parent → grandparent.
        let chain: Vec<_> = r.walk().collect();
        assert_eq!(chain.len(), 3);
        assert_eq!(
            chain[0].file.root_material_path.as_deref(),
            Some("materials/parent.bgsm"),
        );
        assert_eq!(
            chain[1].file.root_material_path.as_deref(),
            Some("materials/grandparent.bgsm"),
        );
        assert!(chain[2].file.root_material_path.is_none());
    }

    #[test]
    fn cache_hit_avoids_second_resolver_read() {
        let mut resolver = StubResolver::new();
        resolver.add("materials/leaf.bgsm", minimal_v2_bytes());

        let mut cache = TemplateCache::new(16);
        let first = cache.resolve(&mut resolver, "materials/leaf.bgsm").unwrap();
        assert_eq!(resolver.read_count, 1);

        // Second call hits cache — resolver.read is NOT invoked again.
        let second = cache.resolve(&mut resolver, "materials/leaf.bgsm").unwrap();
        assert_eq!(resolver.read_count, 1);

        // Same Arc — bit-identical, not a re-parse.
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn case_insensitive_path_match() {
        let mut resolver = StubResolver::new();
        resolver.add("materials/leaf.bgsm", minimal_v2_bytes());

        let mut cache = TemplateCache::new(16);
        cache.resolve(&mut resolver, "Materials/LEAF.bgsm").unwrap();
        // Second call via uppercase hits cache — single resolver read.
        cache.resolve(&mut resolver, "MATERIALS/leaf.BGSM").unwrap();
        assert_eq!(resolver.read_count, 1);
    }

    #[test]
    fn missing_template_returns_not_found() {
        let mut resolver = StubResolver::new();
        resolver.add(
            "materials/orphan.bgsm",
            bgsm_with_template("materials/missing_parent.bgsm"),
        );

        let mut cache = TemplateCache::new(16);
        match cache.resolve(&mut resolver, "materials/orphan.bgsm") {
            Err(ResolveError::NotFound { path }) => {
                assert_eq!(path, "materials/missing_parent.bgsm");
            }
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn lru_eviction_caps_memory() {
        let mut resolver = StubResolver::new();
        for i in 0..5 {
            resolver.add(&format!("materials/file_{i}.bgsm"), minimal_v2_bytes());
        }

        let mut cache = TemplateCache::new(3);
        for i in 0..5 {
            cache
                .resolve(&mut resolver, &format!("materials/file_{i}.bgsm"))
                .unwrap();
        }
        assert_eq!(cache.len(), 3);
        // file_0, file_1 were evicted; file_2, file_3, file_4 remain.
        // Re-resolving file_0 triggers a fresh resolver read.
        let before = resolver.read_count;
        cache
            .resolve(&mut resolver, "materials/file_0.bgsm")
            .unwrap();
        assert_eq!(resolver.read_count, before + 1);
    }
}
