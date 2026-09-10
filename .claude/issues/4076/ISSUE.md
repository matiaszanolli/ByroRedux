# #4076 — ESM-2026-09-09-D1-01

`parse_wrld_group`'s world-children `sub_end` is the one production GRUP end still unclamped to its parent — the residue of #3721

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4076 --json state`).

---

- **Severity**: LOW
- **Dimension**: Header & GRUP Walk
- **Record / Sub-record**: `WRLD` GRUP type 1 (world children)
- **Location**: `crates/plugin/src/esm/cell/wrld.rs:28`
- **Status**: NEW (incomplete fix of #3721, CLOSED)
- **Description**: #3721 (`2ec58c43`, 2026-09-02) added a `parent_end` parameter to
  `EsmReader::bounded_group_content_end` and clamps the returned end with
  `.min(parent_end)`. Its commit message says "`end` was already in scope at all 13
  call sites … and threaded through unchanged otherwise". It did thread all 13
  *bounded* call sites (verified: 14 today, all pass `end`). It did **not** touch the
  one production site that still calls the **raw** `group_content_end` for a child
  group it then recurses into: `parse_wrld_group`.
- **Evidence**: current `crates/plugin/src/esm/cell/wrld.rs:25-42`:
  ```rust
  while reader.position() < end && reader.remaining() > 0 {
      if reader.is_group() {
          let sub_group = reader.read_group_header()?;
          let sub_end = reader.group_content_end(&sub_group);   // <-- not .min(end)
          match sub_group.group_type {
              1 => { … parse_wrld_children(reader, sub_end, cells, &mut persistent_cell, true)?; }
  ```
  `group_content_end` is `self.pos + group_content_len(header)` (`reader.rs:963-965`)
  with no upper bound. The two other raw call sites are benign: `records/mod.rs:293`
  is the non-recursive top-level loop (documented exemption), and the rest are test
  fixtures — `grep -rn "group_content_end" crates/plugin/src byroredux/src | grep -v bounded`
  returns exactly those.
  The top-level dispatcher does **not** re-seek after a walker returns
  (`records/mod.rs:285-516`: the `while reader.remaining() > 0` loop reads the next
  header at wherever the arm left the cursor), so an overrun here desynchronises the
  whole remaining top-level walk, not just this worldspace.
- **Impact**: a corrupt or hostile `WRLD` whose type-1 world-children group declares a
  `total_size` larger than the enclosing top-level `WRLD` GRUP's remaining content makes
  `parse_wrld_children` consume records belonging to the next top-level group and file
  them as exterior cells of this worldspace — silent mis-attribution, then a desynced
  top-level walk. Not memory-unsafe (every loop also tests `reader.remaining() > 0` and
  `read_record_header` returns `Err` on truncation). **No vanilla master triggers it** —
  an independent walker over all seven installed masters (see Dim 8) found zero
  overrunning nested groups. Same severity and same rationale as the original #3721,
  which was filed LOW.
- **Related**: #3721, #3503, #3237.
- **Suggested Fix**: `let sub_end = reader.group_content_end(&sub_group).min(end);` —
  `end` is already the function parameter one line up. Alternatively give
  `parse_wrld_group` a `depth` of its own and route it through
  `bounded_group_content_end` like every sibling walker, which also closes the
  (currently unreachable) recursion path.

> **Independently corroborated.** Dimension 5 reached this same site from the CELL/WRLD
> side and filed it as *D5-03*; the two runs did not share notes. Its extra observation is
> worth keeping: `reader.rs:958-960`'s doc-block explicitly whitelists `parse_wrld_group`
> for the raw `group_content_end`, but that whitelist is justified only by the
> **nesting-depth** half of the contract ("non-recursive top-level dispatch loops … enter a
> group exactly once"), which is orthogonal to the **how-far** half #3721 added. The
> whitelist wording is what makes the site look exempt when it is not, so the docblock
> should be amended alongside the call.

---
