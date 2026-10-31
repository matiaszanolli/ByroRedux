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

/// Names of the `Vec<GpuBuffer>` fields declared in the `SceneBuffers` struct.
fn buffer_families(buffers_rs: &str) -> Vec<String> {
    between(buffers_rs, "pub struct SceneBuffers {", "\n}\n")
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (name, ty) = line.strip_prefix("pub(super) ")?.split_once(':')?;
            (ty.trim() == "Vec<GpuBuffer>,").then(|| name.trim().to_string())
        })
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
