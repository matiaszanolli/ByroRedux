# #1890 (DELTA-01) + #1891 (DELTA-02) — follow-up trackers from AUDIT_INCREMENTAL_2026-07-05

Both are follow-ups to changes I made earlier this session (#1889 VWD marker,
a8d65d6c; #1837 insert_resource poison fix, aedcba12). Both LOW.

## #1890 — VWD marker write-only + missing positive spawn-path test
Two loose ends:
1. **No reader (by design).** The `VisibleWhenDistant` marker is a parse→spawn
   hook the deferred full-model LOD cull will read once the full-detail radius
   is decoupled from the streaming ring; the conservative ring (#1866) makes an
   active cull unnecessary today. **Left as-is** — this is documented durably on
   the component's own doc comment (`components.rs:118-126`), the permanent home
   the issue itself points to, so closing the tracker loses nothing. Wiring a
   reader now would be premature (needs real-game visual validation).
2. **Missing positive spawn-path test.** The record→flag step was pinned by
   #1889 (`addn_stat.rs::parse_stat_with_vwd_flag_sets_visible_when_distant`),
   but the binary-side flag→marker spawn plumbing (`references/mod.rs:755`,
   `if stat.visible_when_distant { world.insert(root, VisibleWhenDistant) }`)
   was an untestable one-liner buried in the spawn loop.

**Fix**: extracted the one-liner into `stamp_visible_when_distant(world, root,
flag: bool)` (so it's testable without the Vulkan spawn path) and added
`stamp_visible_when_distant_marks_only_flagged_roots` — a flagged root gets the
marker, an unflagged one does not. Passing the `bool` (not the plugin-crate
`StaticObject`, which isn't reachable from a binary test) is the right seam: it
pins exactly the binary-side half, and the plugin-crate half is already pinned
in `addn_stat.rs`. Chain now fully covered: header flag (reader.rs) → StaticObject
field (addn_stat.rs) → ECS marker (new test).

## #1891 — insert_resource doc omits the panic-on-poison contract
#1837 made `insert_resource` re-panic (via `resource_lock_poisoned::<R>()`) on a
poisoned prior-value lock instead of swallowing `None`, but the rustdoc had no
`# Panics` note. Per the issue's "do it crate-wide in one pass", added a poison
`# Panics` note to **all eight** poison-propagating resource methods:
`insert_resource` + `remove_resource` (added a `# Panics` section — they had
none) and `resource` / `resource_mut` / `resource_2_mut` / `try_resource` /
`try_resource_mut` / `try_resource_2_mut` (appended a poison line to their
existing `# Panics`). The `try_*` notes emphasise the prefix is about existence,
not poison recovery — poison still panics, not `None`. Doc-only; no behaviour
change.

## Domain / verification
#1891 → `byroredux-core` (world.rs); #1890 → `byroredux` (references/mod.rs).
2 files. core tests green (523); the new spawn-path test passes; `cargo doc
-p byroredux-core` adds no new warnings (all listed warnings are pre-existing
doc-rot); full workspace green, no new warnings.
