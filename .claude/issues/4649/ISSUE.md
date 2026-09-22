# PAR-D1-2026-09-21-01: HKX materialises one on-disk string once per pointer to it: memory is quadratic in file size, with no cap

Labels: high,bug,import-pipeline,game:skyrim

## Description
`crates/hkx/src/packfile.rs:172-193` (`Packfile::parse`'s virtual-fixup loop), `crates/hkx/src/animation.rs:290-307` (`decode_skeleton`'s bone-name loop) and `crates/hkx/src/animation.rs:518-577` (`read_annotations`) each materialise one owned `String` copy per pointer to a shared on-disk string, with no length cap and no de-duplication.

- **Virtual fixups.** `Packfile::parse` turns every virtual-fixup entry into `(offset, read_cstr(..).to_owned())`. The entry count is bounded only by the table size, which is file-sized.
- **Bone names.** `decode_skeleton` copies each bone name via `.to_owned()`.
- **Annotations.** `read_annotations` copies each annotation text and clones the track name per annotation.
- The existing count caps (4,096 bones, 65,536 annotations, `MAX_TRANSFORM_SAMPLES` = 16,000,000 samples) bound the number of *objects*, not the bytes each one owns. N pointers to one M-byte shared string cost N·M bytes of real (written) memory, and `read_cstr` also rescans the M bytes on every pointer.

Verified unchanged at HEAD `ee6d3fb39`: no string-length cap, no dedup, in any of the three sites.

## Evidence
Probe results (`hkx-bones` / `hkx-vfix`, builder follows `packfile::fixtures::PackfileBuilder`'s 64-bit layout):

```
hkaSkeleton file 368,929 B (4096 bones -> one 65,536 B name): decode Ok in 0.80 s;
    owned name bytes 268,435,456; VmHWM 3,916 kB -> 266,544 kB
packfile 256,561 B (20,000 virtual fixups -> one 16,384 B class name): decode_skeleton -> Err(MissingClass)
    after VmHWM 3,896 kB -> 322,844 kB (the copies happen inside Packfile::parse, before any class lookup)
```

With N = S/24 fixups and an S/2-byte name, a file of S bytes allocates about S²/48. That is roughly 21 GB for a 1 MB file and 83 GB for a 2 MB file. The annotation route reaches 65,536 × M.

## Impact
OOM kill or `handle_alloc_error` abort. Neither is interceptable.

- `decode_skeleton` / `decode_spline_animation` run on the main thread from `byroredux/src/asset_provider/animation.rs`: `skeleton.hkx`, the cart-idle family, `1hm_walkforward.hkx`, and the draugr rig plus three clips.
- Lookup goes through `TextureProvider::extract_mesh`, where the last-listed archive wins (#3637 precedence). A mod BSA that overrides `skeleton.hkx` (a very common kind of mod) is enough.
- Skyrim only (HKX is Skyrim's animation format in this workspace).

## Related
#3011 (sample-count bomb, closed; counts only), #4332 (layout gates), PAR-D1-2026-09-21-02 (companion Size Discipline finding, same crate)

## Suggested Fix
- Resolve virtual-fixup class names by reference (store the name offset, or compare in place against the few classes the crate looks up) instead of owning a copy per entry.
- Cap name and annotation string length (Havok names are short, e.g. 256 bytes).
- Budget total owned string bytes per decode relative to `bytes.len()`.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D1-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix