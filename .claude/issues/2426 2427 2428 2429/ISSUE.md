# Issue batch: 2426, 2427, 2428, 2429 (TD8 dead-dependency cleanup)

## #2426 — TD8-001 (LOW, tech-debt)
`thiserror` declared but unused in 4 crates: `crates/bsa/Cargo.toml:8`,
`crates/spt/Cargo.toml:18`, `crates/papyrus/Cargo.toml:8`,
`crates/nif/Cargo.toml:9`. Remove all four lines.

## #2427 — TD8-002 (LOW, tech-debt)
`crates/debug-ui/Cargo.toml` declares `byroredux-renderer` (:8),
`egui-ash-renderer` (:11), `anyhow` (:13) with zero source references.
Remove all three lines.

## #2428 — TD8-003 (LOW, tech-debt)
`crates/ui/Cargo.toml` declares `byroredux-core` and `ruffle_render` unused
(confirmed via grep). `image` is a softer call (transitively reachable via
`ruffle_render_wgpu`) — leave it, per issue's own guidance.

## #2429 — TD8-004 (LOW, tech-debt)
`crates/platform/Cargo.toml` declares `byroredux-core` unused. Remove the
line.

All four: verify via grep before removing, then `cargo check` (scoped +
full workspace) to confirm no accidental breakage.
