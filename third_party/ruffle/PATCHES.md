# Vendored `ruffle_core` (patched)

`ruffle_core`, `ruffle_common` and `ruffle_macros` from
[ruffle-rs/ruffle](https://github.com/ruffle-rs/ruffle) at revision
`2edfd97d11897e6cf6301c30cd87e86d774919c1` — the revision the workspace's git dependencies pin —
with one engine patch. The root `Cargo.toml` redirects those three crates here through
`[patch."https://github.com/ruffle-rs/ruffle"]`; every other Ruffle crate (`ruffle_render`,
`ruffle_render_wgpu`, `ruffle_video*`, `swf`, `ruffle_wstr`, `flv-rs`, `naga-agal`,
`build_playerglobal`) still comes from git at the same revision, and the vendored manifests name
them there, so the graph holds one copy of each. Licence: MIT OR Apache-2.0 (`LICENSE.md`).

## Layout changes

- `Cargo.toml` here is a trimmed copy of Ruffle's workspace manifest: members are only the
  vendored crates; `[workspace.package]`, `[workspace.dependencies]` and `[workspace.lints]` are
  unchanged, so the crates keep their inherited settings. The engine workspace excludes this
  directory.
- `core/Cargo.toml` and `core/common/Cargo.toml`: path dependencies on sibling Ruffle crates
  (`../render`, `../video`, `../wstr`, `../swf`, `../flv`, `../render/naga-agal`,
  `build_playerglobal`) became git dependencies at the pinned revision.
- `core/build_playerglobal` is not vendored (the git crate builds the AS2/AS3 globals from
  `core/src` here, which `core/build.rs` points it at).

## Patch: GFx `$`-key text translation (M48 GUI-FONT-004, #6665)

GFx shows a translation-table entry in a text field whose text is a `$` key
(`interface\translate_<language>.txt`); Flash, and so Ruffle, has no such pass.

- `core/src/byro_translation.rs` (new): a process-wide lookup the engine installs
  (`set_text_translator`) and `translate`, which replaces a whole-text key, or each key run
  between HTML tags.
- `core/src/lib.rs`: `pub mod byro_translation;`.
- `core/src/display_object/edit_text.rs`: `from_swf_tag` (the text a field is created with),
  `set_text` and `set_html_text` pass their text through `translate`. Each site is marked
  `byroredux patch`.

The engine side is `crates/ui/src/translate.rs` (`install_translations`); its test
`the_ruffle_text_pass_translates_installed_keys` exercises the patch.

## Moving to a new Ruffle revision

1. Bump the `rev` of every Ruffle git dependency in the workspace.
2. Re-copy `core` (without `build_playerglobal`) and Ruffle's root `Cargo.toml` into this
   directory, redo the layout changes above with the new revision, and re-apply the three
   `byroredux patch` sites (`rg "byroredux patch" third_party/ruffle`).
3. `cargo tree -p byroredux-ui | rg ruffle_` must show `ruffle_core` / `ruffle_common` /
   `ruffle_macros` from this path and every other Ruffle crate once, from git.
