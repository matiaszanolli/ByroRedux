//! #6521 — every per-frame `Vec<GpuBuffer>` family of `SceneBuffers` is spelled at
//! several sites (field, bundle, alloc loop, bundle literal, move into `Self`) and
//! torn down by a hand-unrolled loop in `destroy`. #6250 added
//! `moved_surface_buffers` at all of them but `destroy`, which leaked the allocator
//! `Arc` and failed the teardown `unwrap` at every shutdown (SAFE-2026-10-27-01).
//! `destroy` needs a live device, so pin the coverage at the source: each family
//! the struct declares must be walked by `destroy`.

/// The text of `src` from the first `start` to the first `end` after it.
fn between<'a>(src: &'a str, start: &str, end: &str) -> &'a str {
    let from = src.find(start).unwrap_or_else(|| panic!("`{start}` not found"));
    let to = src[from..].find(end).unwrap_or_else(|| panic!("`{end}` not found after `{start}`")) + from;
    &src[from..to]
}

/// `(name, type)` of every field declared in the `SceneBuffers` struct, whatever its
/// visibility spelling (`pub`, `pub(super)`, `pub(crate)`). A field spelled any other way
/// was invisible to the old scan, which keyed on `pub(super)` alone (#7381).
fn struct_fields(buffers_rs: &str) -> Vec<(String, String)> {
    between(buffers_rs, "pub struct SceneBuffers {", "\n}\n")
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = ["pub(super) ", "pub(crate) ", "pub "]
                .iter()
                .find_map(|vis| line.strip_prefix(vis))?;
            let (name, ty) = rest.split_once(':')?;
            Some((name.trim().to_string(), ty.trim().trim_end_matches(',').to_string()))
        })
        .collect()
}

/// Names of the `Vec<GpuBuffer>` fields declared in the `SceneBuffers` struct.
fn buffer_families(buffers_rs: &str) -> Vec<String> {
    struct_fields(buffers_rs)
        .into_iter()
        .filter_map(|(name, ty)| (ty == "Vec<GpuBuffer>").then_some(name))
        .collect()
}

/// Names of the single `GpuBuffer` fields (the same leak applies: a scalar buffer missed in
/// `destroy` keeps its allocator `Arc` clone alive, #7381).
fn scalar_buffers(buffers_rs: &str) -> Vec<String> {
    struct_fields(buffers_rs)
        .into_iter()
        .filter_map(|(name, ty)| (ty == "GpuBuffer").then_some(name))
        .collect()
}

/// Names of the deferred-destroy queues of `GpuBuffer`s, drained by `destroy`.
fn buffer_queues(buffers_rs: &str) -> Vec<String> {
    struct_fields(buffers_rs)
        .into_iter()
        .filter_map(|(name, ty)| ty.ends_with("DeferredDestroyQueue<GpuBuffer>").then_some(name))
        .collect()
}

#[test]
fn destroy_walks_every_per_frame_buffer_family() {
    let buffers_rs = include_str!("buffers.rs");
    let descriptors_rs = include_str!("descriptors.rs");
    let destroy = between(descriptors_rs, "pub unsafe fn destroy(&mut self", "\n    }\n}\n");
    let families = buffer_families(buffers_rs);
    assert!(
        families.len() >= 12,
        "the field scan lost its footing (found {families:?}); fix `buffer_families`"
    );
    for family in &families {
        assert!(
            destroy.contains(&format!("for buf in &mut self.{family} {{"))
                && destroy.contains(&format!("self.{family}.clear();")),
            "`SceneBuffers::destroy` never tears down `{family}` — a per-frame buffer family \
             that skips teardown leaks the allocator Arc and fails the shutdown unwrap (#6521)"
        );
    }
}

/// #7381 — the single `GpuBuffer` fields (`bind_inverses_persistent`,
/// `bind_inverse_upload_staging`, `terrain_tile_buffer`, `ray_budget_buffer` and, since
/// RLT-IBL-003, `env_brdf_lut_buffer`) and the deferred-destroy queues of buffers fail the
/// shutdown the same way a missed family does, and were covered only by hand review.
#[test]
fn destroy_tears_down_every_single_buffer_and_drains_every_buffer_queue() {
    let buffers_rs = include_str!("buffers.rs");
    let descriptors_rs = include_str!("descriptors.rs");
    let destroy = between(descriptors_rs, "pub unsafe fn destroy(&mut self", "\n    }\n}\n");
    let scalars = scalar_buffers(buffers_rs);
    assert!(
        scalars.len() >= 5,
        "the scalar-field scan lost its footing (found {scalars:?}); fix `scalar_buffers`"
    );
    for name in &scalars {
        assert!(
            destroy.contains(&format!("self.{name}.destroy(device, allocator);")),
            "`SceneBuffers::destroy` never tears down `{name}` — a buffer that skips teardown \
             leaks the allocator Arc and fails the shutdown unwrap (#7381, the #6521 class)"
        );
    }
    let queues = buffer_queues(buffers_rs);
    assert!(!queues.is_empty(), "the queue scan lost its footing; fix `buffer_queues`");
    for name in &queues {
        let from = destroy
            .find(&format!("self.{name}"))
            .unwrap_or_else(|| panic!("`SceneBuffers::destroy` never reaches the `{name}` queue (#7381)"));
        assert!(
            destroy[from..].contains(".drain("),
            "`SceneBuffers::destroy` does not drain `{name}`, so its retired buffers outlive the allocator (#7381)"
        );
    }
}

#[test]
fn the_bundle_struct_carries_every_family_too() {
    // The alloc-time bundle and the struct must list the same families, or a new one
    // is allocated and then dropped (or never moved into `Self`).
    let buffers_rs = include_str!("buffers.rs");
    let bundle = between(buffers_rs, "struct SceneRenderBuffers {", "\n}\n");
    for family in buffer_families(buffers_rs) {
        assert!(
            bundle.contains(&format!("{family}: Vec<GpuBuffer>,")),
            "the per-frame allocation bundle lacks `{family}` (#6521)"
        );
    }
}
