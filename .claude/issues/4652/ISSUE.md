# PAR-D2-2026-09-21-03: MenuXml HUD load panics on a .fnt shorter than 12 bytes

Labels: high,bug,ui,import-pipeline,game:oblivion,game:fo3,game:fnv

## Description
`crates/menuxml/src/menu.rs:188` — `load_with_profile` computes the font's atlas name with `String::from_utf8_lossy(&fnt[12..])` **before** calling `Font::parse`. `Font::parse` has its own `fnt.len() < HEADER_LEN` -> `FontError::TruncatedHeader` check (`font.rs:74-76`), but it never runs because the slice panics first for any `.fnt` shorter than 12 bytes.

Owner note: `menu.rs` is `/audit-ui` territory. `/audit-ui`'s own `AUDIT_UI_2026-09-21.md` run confirmed this same panic and explicitly deferred filing it to `/audit-parsers` (this report), since it is a file-byte-reachable panic on the reader load path.

Verified unchanged at HEAD `ee6d3fb39`: `let name = String::from_utf8_lossy(&fnt[12..])` is still the first thing done with `fnt` in that closure, ahead of `Font::parse`.

## Evidence
Probe `menuxml-short-fnt` (8-byte font through the public API):

```
thread 'main' panicked at crates/menuxml/src/menu.rs:188:56:
range start index 12 out of range for slice of length 8
```

## Impact
Engine panic at `--hud` launch (main thread, no `catch_unwind`). Fonts come from the Misc BSA (Oblivion) or the texture BSA (FO3/FNV, `FontArchive::Textures`, user-selectable via `--hud-textures`). The rest of `load_with_profile` already degrades a failed font to `None` with a warning — this one site bypasses that degrade-gracefully path entirely.

## Related
PAR-D1-2026-09-21-05 (sibling MenuXml parse-path panic); co-owned with `/audit-ui`'s AUDIT_UI_2026-09-21.md, which confirmed this site without re-filing it

## Suggested Fix
Use `fnt.get(12..).unwrap_or_default()`, or read the name inside `Font::parse` after its length check, so a short font takes the existing warn-and-`None` path instead of panicking.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D2-2026-09-21-03)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix