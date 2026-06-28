# #1765: D3-NEW-01: build_handler byte-slices name[..2] — panics on a non-ASCII .pex function name

Filed from `docs/audits/AUDIT_SCRIPTING_2026-06-27.md` on 2026-06-27. Snapshot as-filed (GitHub is authoritative for live state).

**Severity**: HIGH · **Dimension**: Decompiler — AST lowering / event classification · **Untrusted-Input**: Yes
**Location**: `crates/pex/src/decompile/lower.rs:266` (`build_handler`)
**Status**: NEW
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-06-27.md` (D3-NEW-01)

## Description
The event-vs-function classifier slices the first two *bytes* of the function name:

```rust
let is_event = (name.len() > 2 && name[..2].eq_ignore_ascii_case("on") && is_event_name(name))
    || name.starts_with("::remote_");
```

`.pex` strings are decoded with `String::from_utf8_lossy` (names are kept lossy, "Windows-1252-ish"). Any invalid input byte (a Win-1252 byte ≥ 0x80 not valid UTF-8) becomes U+FFFD `�`, a **3-byte** sequence `EF BF BD`. A function name whose first source byte is invalid begins with this 3-byte char: `name.len() > 2` is satisfied, but byte index 2 lands *inside* the replacement char (not a char boundary). `name[..2]` then panics: `byte index 2 is not a char boundary; it is inside '�' (bytes 0..3)`.

## Evidence
Reproduced standalone — `String::from_utf8_lossy(&[0x81])` yields a 3-byte `[239,191,189]` string with `len()==3`; `name[..2]` panics. The name flows unfiltered: `decompile_script` → per-state `build_handler(object, f, &f.name)` (`lower.rs:379/384`), `f.name` straight from the lossy-decoded string table. This is the only `[..2]` site in the crate; no upstream ASCII guard.

## Impact
A single malformed/adversarial (or merely non-ASCII-corrupted) `.pex` in a modded `--scripts-bsa` panics the decompiler instead of returning a `DecompileError`. The panic unwinds past the entire error-returning design (`DecompileError`, the fail-closed #1732 work) — `translate_pex` catches `Err`, not panics, so it reaches the cell loader. Per the domain rule "panic from untrusted `.pex` → HIGH". The corpus-smoke 99.996% claim can't catch it because real vanilla names are ASCII.

## Suggested Fix
Use a boundary-safe check — `name.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("on"))`, or gate on `name.is_char_boundary(2)`. `is_event_name(name)` already lowercases safely.

## Completeness Checks
- [ ] **SIBLING**: grep the pex/papyrus/scripting crates for any other byte-index slice (`[..N]` / `[N..]`) or `is_char_boundary`-free slicing on a lossy-decoded string-table value
- [ ] **TESTS**: a regression test decompiles a `.pex` whose function name starts with an invalid UTF-8 byte and asserts a `DecompileError` (or clean classification), not a panic
