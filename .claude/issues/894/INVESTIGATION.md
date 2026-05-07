# Investigation — #893 / LC-D6-NEW-01

## Approach selection

The issue suggests two options:
1. Stack-buffer fast path
2. Lookup-first fast path

**Lookup-first won't work.** The `string_interner::StringBackend` stores keys *as inserted* — i.e. lowercased. A `self.0.get("Bip01 Spine")` on a pool that already contains `"bip01 spine"` does a **case-sensitive** lookup and misses. The fast path would only fire when callers pass already-lowercase input, which is rare in NIF/ESM content.

**Stack-buffer fast path works.** Lowercase into a `[u8; 256]` stack buffer, hand the resulting `&str` to the interner without allocating. Falls back to `String` for the rare >256-byte input.

## UTF-8 safety

`<[u8]>::make_ascii_lowercase` only flips the 0x20 bit on bytes in 0x41..=0x5A. Multi-byte UTF-8 sequences have the high bit set on every byte, so they're untouched. The output buffer is therefore guaranteed valid UTF-8 if the input was — which is structurally true since `s` arrived as `&str`.

`std::str::from_utf8_unchecked` is sound here, with a one-line safety comment. Could use `from_utf8().unwrap()` instead — adds a UTF-8 validation walk but never fails. Performance difference is negligible at 256 bytes; using `unchecked` keeps the fast path branch-free.

## Buffer size

256 bytes covers every string in vanilla content I've seen — longest BSA-internal asset path is ~120 chars, longest NIF node name ~64. 256 leaves headroom and is well below typical stack frame size.

## Files touched
- `crates/core/src/string/mod.rs` — only file
