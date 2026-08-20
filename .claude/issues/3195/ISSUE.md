# #3195 — SPT-D4-2026-08-20-01: #3078's recoverable-placeholder fix landed on the cell route only

- **Filed**: 2026-08-20 (`/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3195
- **Labels**: `low,legacy-compat,bug`
- **Source report**: `docs/audits/AUDIT_SPEEDTREE_2026-08-20.md`
- **HEAD at audit**: `bb0b92f2`

---

**Severity**: LOW
**Dimension**: Per-Game Variants & Route Divergence
**Source**: `docs/audits/AUDIT_SPEEDTREE_2026-08-20.md` (`SPT-D4-2026-08-20-01`) — HEAD `bb0b92f2`

## Status

NEW — a **partial fix** of the CLOSED **#3078**, not a regression.

## Location

- `byroredux/src/scene/nif_loader.rs` — the loose `--tree` / `--mesh foo.spt` `.spt` branch
- `byroredux/src/cell_loader/references/import.rs` — `parse_and_import_spt`'s error arm
- `byroredux/src/cell_loader/spawn/mesh_instance.rs` — the `SpeedTreeWind` attach

## Description

**#3078** established the contract that a malformed `.spt` parameter section must not erase the tree,
since the placeholder needs nothing from the parse except an optional relative tag-4003 path.

The **cell** route now honours it:

```rust
Err(e) => {
    log::warn!("Failed to parse SPT '{}': {}", label, e);
    // TREE metadata is sufficient for the placeholder. A malformed
    // parameter section must not erase the REFR (#3078).
    byroredux_spt::SptScene::default()
}
```

The **loose** `--tree` / `--mesh foo.spt` visualiser route does not:

```rust
Err(e) => {
    log::error!("Failed to parse SPT '{}': {}", label, e);
    return None;
}
```

Both routes still call `parse_spt` + `import_spt_scene`, so the parse-parity requirement holds; it is the
**error arm** that diverged.

The loose route is also now the *less* capable of the two in a second way: `SpeedTreeWind` is attached
only from `cached.speedtree_wind` (`mesh_instance.rs`), which is a cell-loader concept. The loose route
builds no `CachedNifImport`, so a `--tree`-loaded `.spt` gets its `Billboard` but **never a
`SpeedTreeWind`**, and consequently cannot exercise the wind path this delta added at all.

## Evidence

The two error arms above, confirmed at HEAD. `grep -rn "SpeedTreeWind" byroredux/src` shows **no
`nif_loader.rs` hit**.

## Impact

The `.spt` visualiser — the tool a developer reaches for when a tree is wrong — is the one path that
still **fails closed** on a malformed file, and is structurally unable to reproduce the wind behaviour it
would be used to debug. Dev-workflow only; no shipped-content impact.

Given that three of this cycle's SpeedTree findings are about the wind path, the inability to inspect a
single tree under wind in the visualiser is a concrete obstacle to fixing them.

## Suggested Fix

- Mirror the cell route's `SptScene::default()` fallback in `byroredux/src/scene/nif_loader.rs`,
  downgrading the `error!` to `warn!`.
- Attach a default `SpeedTreeWind::new(1.0, 0.0)` on the loose `.spt` branch so `--tree` exercises the
  same system the cell route does.

## Related

- **#3078** (CLOSED) — the contract this half-implements.
- **#3076** (CLOSED, verified fixed at HEAD) — the per-mesh `Billboard` attach that the loose route
  already mirrors correctly.
- **#3079** (OPEN) — the `is_spt` dispatch lives in `synth_child.rs` / `nif_loader.rs`, not
  `references/mod.rs`; re-confirmed true at HEAD.
- **#3190** / **#3191** / **#3194** — the wind findings this route cannot reproduce.

## Completeness Checks

- [ ] **SIBLING**: both `.spt` dispatch sites (`cell_loader/references/import.rs` and
      `scene/nif_loader.rs`) end up with the same error arm — a third route added later must inherit it
- [ ] **TESTS**: a guard that a malformed `.spt` still yields a placeholder on the **loose** route, the
      mirror of the existing cell-route guard
