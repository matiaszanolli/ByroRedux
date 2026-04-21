# Investigation

Two call sites currently clone mesh.material_path into the Material
component without resolving:
- scene.rs:955 (CLI single-NIF path)
- cell_loader.rs:1571 (cell loader path)

Plan: add `MaterialProvider` in asset_provider.rs that:
- owns Vec<Archive> for Materials BA2s
- owns a TemplateCache + implements TemplateResolver internally
- exposes `resolve_bgsm(&mut self, path: &str) -> Option<Arc<ResolvedMaterial>>`

Add helper `merge_bgsm_into_mesh(mesh, provider)` that fills empty
texture/normal_map/parallax_map/etc slots from the resolved BGSM's
walk chain (child wins). NIF-filled fields are left alone.

CLI: add `--materials-ba2 <path>` flag (repeatable).

Files: Cargo.toml, asset_provider.rs, scene.rs, cell_loader.rs, main.rs.
