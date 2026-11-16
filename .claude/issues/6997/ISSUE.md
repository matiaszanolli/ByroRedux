# #6997: UI-D4-2026-11-15-01: a `CFX` / `GFX`-signature movie bypasses the #5719/#5966/#6225 declared-length ceiling everywhere (root, import, runtime fetch)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6997
- **Labels**: medium,ui,safety,bug
- **Source**: docs/audits/AUDIT_UI_2026-11-15.md

From the UI audit `docs/audits/AUDIT_UI_2026-11-15.md` (HEAD 8473ae994).

- **Severity**: MEDIUM (defence-in-depth on untrusted archive content; same class and rating as #5966 and #6225)
- **Dimension**: Resource Navigator
- **Profile**: both (Skyrim AVM1 and FO4 AVM2 share the navigator)
- **Existing**: none. A residual of closed #5966 / #6225 (their guard `every_decode_site_is_behind_the_declared_length_ceiling` only counts `decompress_swf(` call sites, so it is blind to a signature the check ignores).
- **Location**:
  - `crates/ui/src/prepare.rs:211` (`ensure_declared_len_within` returns `Ok` unless the first three bytes are `FWS|CWS|ZWS`)
  - `crates/ui/src/prepare.rs:105-109` (shim: `_ => return None` for anything else)
  - `crates/ui/src/prepare.rs:375` (`stock_inflates` counts only `CWS|ZWS`)
  - `crates/ui/src/navigator.rs:658` (`is_swf`: same three signatures)
  - The decision points that consume it: `navigator.rs:283-287` (ceiling), `:300` (`normalize_scaleform_dialect`), `:306-326` (`prepare_import_asset_swf` / `import_asset_paths`).
- **Evidence**:
  - The workspace's pinned Ruffle (`rev=2edfd97`, `~/.cargo/git/checkouts/ruffle-b1b568dcb5621339/2edfd97/swf/src/read.rs:2584-2594`): `read_compression_type` maps `b"FWS" | b"GFX"` to `Compression::None` and `b"CWS" | b"CFX"` to `Compression::Zlib`. `decompress_swf` then reads `take(uncompressed_len - 8)` from the zlib stream, so the header's `u32` bounds the inflate (the #5719 shape).
  - The shipped corpus carries `CFX`: scanning the installed archives, `Skyrim - Interface.bsa` has 6 `.gfx` entries, all `CFX` (the other 53 movies are 51 `CWS`, 2 `FWS`), and `Fallout4 - Interface.ba2` has 4 `.gfx`, all `CFX`. No vanilla movie names a `.gfx` or `exported\` path, so vanilla content never fetches them. A mod or replacement movie can.
  - Probe in the worktree (`navigator.rs` test, removed with the worktree), 1.2 MB of zlib over 256 MiB of zeros, signature changed to `CFX`, header claim `u32::MAX`:
    - `ensure_declared_len_within_ceiling(&cfx)` -> `Ok(())` (the same body with `CWS` is refused).
    - `ScaleformNavigator::fetch("bomb.gfx")` served the archive bytes unchanged (`body == cfx` true), recorded `errors=[]`, and recorded a normal `loads` entry.
    - `swf::decompress_swf` on the served body returned `Ok` with 268,435,451 bytes in 146 ms (ceiling is `MAX_SWF_BODY_BYTES` = 128 MiB, `prepare.rs:143`); `prepare_movie(&cfx, None, None)` is `Ok` in 167 ms, i.e. the root route is open too.
  - `navigator::is_swf(CFX)` is `false`, so a `CFX` dependency also skips `prepare_import_asset_swf` and the nested `ImportAssets` scan (it still reaches Ruffle, which parses it as a movie).
- **Impact**: a hostile or corrupt `CFX` movie in an interface archive inflates to the header's `u32` (up to 4 GiB) inside Ruffle's own `SwfMovieData::from_data`, with no error and no ceiling; this is the same memory-DoS the ceiling was added for on `CWS`. It needs attacker-controlled archive content, hence MEDIUM, not HIGH. Because `CFX` is Scaleform's native compressed form, a mod movie using it is not exotic.
- **Suggested Fix**: one shared predicate, one place. Make `prepare::is_swf_signature` (or the existing `ensure_declared_len_within` match) accept `FWS|CWS|ZWS|GFX|CFX`, have `navigator::is_swf` call it (it is a duplicate of the `matches!` at `prepare.rs:211` and `:375`), and treat `CFX` like `CWS` in the shim / `stock_inflates` accounting (or refuse `CFX`/`GFX` outright as a navigator-served movie, since vanilla never fetches them). Extend the source guard to pin the signature list, and add a unit test beside `dependency_decodes_refuse_an_over_ceiling_claim` with the `hostile_cws_header_bomb()` signature flipped to `CFX` for the root, import and runtime-fetch routes.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
