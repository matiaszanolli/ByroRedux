# #1484 — REN-RENDERER-DOCROT-2026-06-09: Stale renderer comments

_Snapshot as filed 2026-06-09 from docs/audits/AUDIT_RENDERER_2026-06-09.md. GitHub is authoritative for live state._

**Severity**: LOW (documentation rot — no runtime effect)
**Dimension**: Renderer (multiple) — consolidated doc-rot pass
**Source**: `docs/audits/AUDIT_RENDERER_2026-06-09.md`
**Status**: NEW

Four stale comments found during the 2026-06-09 renderer audit, bundled into one cleanup pass (per the report's Prioritized Fix Order). All four are comments that misdescribe correct code — they actively mislead a future maintainer. **No code change required beyond the comments.**

---

### REN-D4-NEW-01 — stale `debug_assert!` claim for the mesh-ID overrun guard
- `crates/renderer/src/vulkan/context/helpers.rs` (~`:96`) — "guarded by the `debug_assert!` in `draw.rs::draw_frame` (#647 / RP-1)".
- `crates/renderer/src/vulkan/scene_buffer/constants.rs` (~`:104`) — "The `gpu_instances.len() <= MAX_INSTANCES` debug_assert in `vulkan::context::draw::draw_frame` enforces this contract".
- **Reality**: that assert was deliberately removed under #992/#956 (it leaked the in-flight command buffer on unwind). The guard is now a one-shot `log::error!` + clamp (`draw.rs:1858-1869` + `upload.rs:477-484`). The stale comment invites a maintainer to "restore" the leak.
- **Fix**: describe the warn-once + clamp guard, not a `debug_assert!`.

### REN-D4-NEW-02 — stale attachment count + stale `triangle.frag:980` line ref
- The G-buffer / render-pass comments say "6 color attachments"; the actual count is **7**.
- `helpers.rs` references the `ALPHA_BLEND_NO_HISTORY` flag at `triangle.frag:980`; the actual write is at `triangle.frag:1531`.
- **Fix**: correct the count to 7 and the line reference to 1531.

### REN-D9-NEW-08 — GI-bounce escape comment cites stale "3000u"
- `crates/renderer/shaders/triangle.frag:3590` — "Ray escaped (no geometry within 3000u)".
- **Reality**: the `giRQ` `tMax` was raised to `6000.0` (`:3537`) and `giFade` ends at 6000 (`:3483`). Code is correct; only the comment is stale.
- **Fix**: change "within 3000u" to "within 6000u". *(While here: `GLASS_RAY_BUDGET` is `1048576` in `shader_constants_data.rs` but the in-shader comment at `triangle.frag:2305` still says `8192` — same doc-rot class, fix in the same pass.)*

### REN-D23-DOC-01 — `gpu_timers.rs` header doc-table stale
- `crates/renderer/src/vulkan/gpu_timers.rs:5,77-78,158-159,233,239` — prose says "14 TIMESTAMP queries", "twelve brackets … currently 7", "all three ms fields", "6-query pool".
- **Reality**: `QUERIES_PER_FRAME = 24`, 12 bracket pairs, 12 active bits. Struct/const are correct; comments drifted across the Phase-6/7 expansions.
- **Fix**: update to 24 queries / 12 brackets / 12 active bits.

---

## Completeness Checks
- [ ] **SIBLING**: while editing each comment, scan its file for other stale counts/line-refs from the same refactor wave.
- [ ] **TESTS**: N/A (comment-only). Consider whether any of these warrant a pin (e.g. the attachment-count is already structurally enforced; the `GLASS_RAY_BUDGET` value could be asserted via the generated-header test — see REN-D16-NEW-01).
- [ ] **UNSAFE / DROP / LOCK_ORDER / FFI / CANONICAL-BOUNDARY**: N/A.
