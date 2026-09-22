# PAR-D2-2026-09-21-01: Every archive-extract consumer discards non-NotFound errors, so a corrupt entry reads as "missing" and silently falls back to a lower-precedence archive

Labels: medium,bug,import-pipeline

## Description
`byroredux/src/asset_provider/texture.rs:79-83`, `:108-113`, `:129-133`, `:175-179` and `byroredux/src/asset_provider/material/provider.rs:316-320`, `:357-361`, `:577-590` all use the same shape:

```rust
for archive in self.texture_archives.iter().rev() {
    if let Ok(data) = archive.extract(normalized.as_ref()) {
        return Some(data);
    }
}
```

All six loops build labelled errors on the reader side and drop every one of them without a log, whatever the cause:
- `InvalidData` from the #3410 decompression-bomb rejection;
- the #352/#586 size guards;
- an LZ4/zlib body error;
- `UnexpectedEof` from a truncated or replaced archive.

The loop then tries the next, earlier-listed archive. A corrupt last-listed override therefore silently resolves to the vanilla copy underneath it, inverting #3637 precedence for that one entry, or ends as `None` if nothing else has it. `None` becomes the checkerboard; for BGEM, the warning "BGEM not found in any loaded archive" (`provider.rs:588`), which is misleading for a present-but-corrupt file; for BGSM templates, `ResolveError::NotFound`.

Verified unchanged at HEAD `ee6d3fb39`: all cited sites still use `if let Ok(data) = archive.extract(..)`.

## Evidence
```rust
for archive in self.texture_archives.iter().rev() {
    if let Ok(data) = archive.extract(normalized.as_ref()) {
        return Some(data);
    }
}
```

## Impact
Corrupt mod content is invisible in logs and can silently revert to vanilla. `tex.missing` reports "missing" for a present-but-corrupt texture, which is the wrong first diagnostic for whoever is debugging it.

## Related
#3637 (the precedence this can invert), #3410, #586, AUDIT_STARFIELD_2026-09-05 (quoted this loop for ordering only, did not flag the error-discard)

## Suggested Fix
`match` the result. Continue silently only on `ErrorKind::NotFound`. Otherwise `warn!` once per (archive, path) with the error and archive path, then choose the fall-through policy explicitly. Mirror this in `MaterialProvider`.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D2-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix