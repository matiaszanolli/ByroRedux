# UI-D4-2026-10-27-01: #5480's navigator comment says an unpatched CWS dependency reaches Ruffle compressed (it reaches it as FWS); `ui.md` still calls runtime-loaded movies un-shimmed and miscounts the UI tests

Labels: low, ui, documentation, doc-rot
Source: docs/audits/AUDIT_UI_2026-10-27.md

**Source**: `docs/audits/AUDIT_UI_2026-10-27.md` — finding `UI-D4-2026-10-27-01` (LOW, doc-rot, Dim 4 Resource Navigator, both Scaleform profiles)
**Location (current)**: `crates/ui/src/navigator.rs` (the #5480 comment in `load_archive_resource`, plus `ScaleformResourceLoad::byte_len`), `docs/engine/ui.md` (the `prepare.rs` bullet near line 883-886 and the test count near line 753); contradicted contract at `crates/ui/src/prepare.rs` (`normalize_scaleform_dialect_counting`).

## Description
Three stale statements left behind by #5480 (`1ac34024f`, closed):

1. The #5480 comment in `load_archive_resource` ends "`None` (no change) keeps the raw bytes — an unpatched CWS still reaches Ruffle compressed, as before". The shim's own contract (#5479) is that CWS input always comes back as `Some(file)`: one inflate, re-emitted as FWS with a corrected length, whether or not any record was patched. `None` is returned only for conforming FWS, ZWS, foreign containers, and a CWS that fails to inflate or overruns its declared length. So every well-formed CWS dependency the navigator serves reaches Ruffle **uncompressed**. As a consequence `ScaleformResourceLoad::byte_len` (`body.len()`) records the inflated served length, not the archive entry's size.
2. `docs/engine/ui.md` still describes the shim as the choke point "every root and `ImportAssets` movie passes through ... (a movie a running menu loads through `Loader.load` / `loadClip` reaches Ruffle un-shimmed, #5480)". That documents the closed bug as current behaviour; since `1ac34024f` every body the navigator serves is shimmed.
3. `docs/engine/ui.md` says "116 default tests plus 3 ignored"; measured at `fa6be4a6b` it is 118 + 3 (the #6225 and #5480 pins are the two new tests).

## Evidence
```rust
// crates/ui/src/prepare.rs, normalize_scaleform_dialect_counting
let was_cws = &swf_data[0..3] == b"CWS";
...
} else if !was_cws {
    return None;            // only FWS-with-no-patch returns None
}
if was_cws {
    file[0..3].copy_from_slice(b"FWS");   // CWS always re-emitted as FWS
    ...
}
Some(file)
```
```
// crates/ui/src/navigator.rs
// ... and `None` (no change) keeps the raw bytes —
// an unpatched CWS still reaches Ruffle compressed, as before.
```

## Impact
None at runtime (handing Ruffle the FWS copy is cheaper than before). But the comment misstates the decode-count property the #5479 / #5967 work exists to hold: a reader trusting it would expect Ruffle to inflate every dependency and might re-add a decode on the theory that one is already paid. The meaning of `byte_len` changed silently, and `ui.md` documents a fixed bug as current behaviour.

## Related
#5480, #6225 (both closed); #5479; #5967; UI-D4-2026-10-21-01.

## Suggested Fix
- Rewrite the `navigator.rs` comment: a CWS body always comes back inflated as FWS, so Ruffle takes the copy path; `None` keeps the raw bytes only for conforming FWS, ZWS and declined CWS.
- Document on `ScaleformResourceLoad::byte_len` that it is the served (post-shim) length.
- In `ui.md`, drop the "un-shimmed, #5480" parenthetical, describe the navigator fetch as shimming every served SWF, and re-measure the UI test count.

## Completeness Checks
- [ ] **SIBLING**: Other comments/docs that mention "compressed" or "un-shimmed" for navigator fetches (`docs/engine/ui.md`, `prepare.rs`, `lib.rs`) are checked
- [ ] **TESTS**: (doc-only) the `ui.md` test count is re-measured with `cargo test -p byroredux-ui`

