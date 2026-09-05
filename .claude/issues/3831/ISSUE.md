# #3831: REN-2026-09-05-DOC-02: volumetrics_inject.comp documents sun_color.a as "unused" while the same file reads it as the cluster-far basis

Filed from `docs/audits/AUDIT_RENDERER_2026-09-05.md` (REN-2026-09-05-DOC-02) via `/audit-publish`, 2026-09-05.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3831 --json state`.

---

**Severity**: LOW
**Dimension**: Volumetrics (in-code doc-rot)
**Source**: `docs/audits/AUDIT_RENDERER_2026-09-05.md` (`REN-2026-09-05-DOC-02`)

## Location

`crates/renderer/shaders/volumetrics_inject.comp` — the `VolumetricsParams` UBO block's `sun_color` declaration comment, vs the `clusterFar` computation in the local-light loop ~2570 lines later in the same file.

## Description

The UBO block comment reads `// rgb = sun radiance (already multiplied by intensity), a = unused`. The local-light clustered-lookup code then reads `params.sun_color.a` as the basis for `clusterFar`:

```glsl
float clusterFar = params.sun_color.a > 1.0 ? max(params.sun_color.a, CLUSTER_FAR_FLOOR) : CLUSTER_FAR_FALLBACK;
```

with its own adjacent comment explaining why — the cell's fog-far, plumbed to match `screen.w` as the identical basis for the exponential depth-slice distribution. The Rust-side struct doc in `volumetrics.rs` correctly documents `.a` as the cell's XCLL fog-far distance. **Only the GLSL declaration-site comment is stale.**

## Evidence

`grep -n "a = unused\|clusterFar = params.sun_color.a" crates/renderer/shaders/volumetrics_inject.comp` → the stale comment and the live read, in the same file. Re-verified at publish time.

## Impact

None on rendering — the code path is correct and matches both the Rust doc and the later in-file comment. The risk is a maintainer reading the UBO block top-to-bottom, trusting "a = unused", and repurposing the field.

## Suggested Fix

Update the declaration comment to match the Rust-side doc and the actual use, e.g. `a = the cell's fog-far distance (see the local-light loop below; matches screen.w's cluster basis)`.

## Completeness Checks

- [ ] **SIBLING**: Other `= unused` / `reserved` field comments in the same UBO block verified against actual reads
