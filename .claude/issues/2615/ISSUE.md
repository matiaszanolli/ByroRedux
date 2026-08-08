# SF-D3-03: Archive::open reads entire multi-GB archive into RAM just to sample 4 magic bytes

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2615
**Finding ID**: SF-D3-03

**Severity**: HIGH
**Dimension**: 3 (CDB Material Database)
**Location**: `byroredux/src/asset_provider/archive.rs:10-27` (`Archive::open`), `byroredux/src/asset_provider/material.rs:194-205` (`build_material_provider`)
**Status**: NEW

## Description
`Archive::open` calls `std::fs::read(path)`, allocating and filling a
`Vec<u8>` the size of the *entire* archive file, purely to extract 4 magic
bytes for BSA-vs-BA2 dispatch. `Starfield - Meshes01.ba2`/`Meshes02.ba2` are
multi-GB; `Starfield - Materials.ba2` carries the ~105 MB CDB.
`build_material_provider`'s own comment claims the archive is "re-opened
here purely to read its file table (the entry data isn't touched)" — but
`Archive::open` reads all the entry data anyway via the full-file `fs::read`,
so each mesh archive is fully read into RAM **twice** per provider build,
from six call sites (`app_step.rs:462,523`, `scene.rs:355,395`,
`scene/nif_loader.rs:54`, `save_io.rs:851`, `debug_load.rs:125`), several of
which re-run on save-load/debug-load.

## Evidence
```rust
// byroredux/src/asset_provider/archive.rs:10-27
pub(crate) fn open(path: &str) -> Result<Self, String> {
    let magic = std::fs::read(path)   // <-- reads the WHOLE file
        .map_err(...)
        .and_then(|data| {
            if data.len() < 4 { Err(...) } else { Ok([data[0], data[1], data[2], data[3]]) }
        })?;
    ...
}
```

## Impact
On a `--bsa`-heavy Starfield invocation, several GB are transiently
allocated per archive and the page cache is thrashed before a single byte of
the file table is actually parsed — a real memory-pressure and I/O-time cost
on the largest archives in the game, paid twice.

## Suggested Fix
Replace the `fs::read` sniff with:
```rust
let mut m = [0u8; 4];
std::fs::File::open(path)?.read_exact(&mut m)?;
```
One-line-scoped, no behavioral change, and makes the "purely to read the
file table" comment true.

## Completeness Checks
- [ ] **TESTS**: A test asserting `Archive::open` reads at most a small fixed number of bytes before dispatching (e.g. via a byte-counting reader wrapper), not the whole file
