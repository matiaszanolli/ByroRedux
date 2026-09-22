# PAR-D1-2026-09-21-05: MenuXml scanner panics when a non-ASCII character follows a comment, and silently drops a byte when it is ASCII

Labels: high,bug,ui,game:oblivion,game:fo3,game:fnv

## Description
`crates/menuxml/src/parse.rs`: `take_text` (`:499-504` region of `parse_element_content`), `skip_trivia` (`:315-327`, comment-skip via `skip_ws`), and the two "drop one byte to guarantee progress" fallbacks (`:552-554`, `:595-598`) combine into a panic on valid UTF-8 input.

- `take_text` stops at the `<` of a comment. The following `skip_trivia` skips the comment and lands on body text. `take_element()` then returns `None` because the text does not start with `<`.
- The "drop one byte to guarantee progress" fallback (`scanner.pos += 1`) advances one **byte**. The next `self.src[self.pos..]` slice (inside `skip_ws`, line 327) panics when that byte sits inside a multi-byte character — `self.src[self.pos..]` panics on a non-char-boundary index.
- When the character is ASCII, the byte is silently lost instead, which corrupts the trait value.

Verified unchanged at HEAD `ee6d3fb39`: `skip_ws`'s `while let Some(c) = self.src[self.pos..].chars().next()` is still at the exact line (327) the original probe's panic trace names.

## Evidence
Probe `menuxml-midchar`:

```
<string>abc<!-- note -->été</string>   -> panicked at parse.rs:327:37: start byte index 55 is not a char boundary; it is inside 'é'
<rect>junk<!-- note --> über<x>1</x>  -> same panic (parse_element_content path), inside 'ü'
<string>plain<!-- note -->ascii</string> -> Str("plainscii")
```

Vanilla scan (probe `menuxml-scan`) of 309 vanilla menu XMLs (Oblivion 89, FNV 121, FO3 99): 0 `text<!--c-->text` sites. Their only non-ASCII bytes are leading UTF-8 BOMs on 6 FNV and 6 FO3 prefabs, which `take_text` consumes harmlessly. Vanilla is unaffected.

## Impact
Engine panic at `--hud` launch (main thread, no `catch_unwind`) from a modified or replaced Misc BSA, or silent value corruption with ASCII input. Same bug class as #3391 (`&str` byte-slicing of disk-derived text).

## Related
#3391 (same bug class — byte-slicing disk-derived text), PAR-D1-2026-09-21-04, PAR-D2-2026-09-21-03

## Suggested Fix
Advance by `self.src[self.pos..].chars().next().map_or(1, char::len_utf8)`, or better, consume the stray text with `take_text` so it is kept rather than dropped. Add the three probe strings as unit tests.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D1-2026-09-21-05)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix