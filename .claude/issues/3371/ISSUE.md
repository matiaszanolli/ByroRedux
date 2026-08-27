# SKY-2026-08-27-D7-03: `EmissiveSource::None`'s doc asserts the exact behaviour #2591 removed, and the BGEM merge is a fourth, ungated writer the helper's doc claims to cover

Labels: low,nifal,documentation,doc-rot,game:skyrim,legacy-compat

- **Severity**: LOW
- **Confidence**: CONFIRMED (code read + `git log` for the fix that made the doc stale)
- **Location**: `crates/core/src/ecs/components/material.rs:591-601` (stale doc),
  `crates/core/src/ecs/components/material.rs:620-640`
  (`emissive_contribution_is_authored`, claims "all three set-sites"),
  `byroredux/src/asset_provider/material.rs:1716-1718` (the ungated writer)
- **Description**: Two related contradictions on the emissive discriminator.

  (a) `EmissiveSource::None`'s doc says:

  > *"All three writers (`dedicated_shader.rs`, `legacy_properties.rs`,
  > `asset_provider/material.rs`) set their variant unconditionally once their
  > property class is bound — there is no non-zero-emissive gate, so e.g. a
  > `BSLightingShaderProperty` with `emissive_multiple == 0.0` still reports
  > `Lighting`, not `None` (#2641)."*

  Commit `aedde151` (*Fix #2589 #2590 #2591*) added exactly that gate. The three
  NIF-side sites (`dedicated_shader.rs:315` Lighting, `dedicated_shader.rs:446`
  Effect, `legacy_properties.rs:155` Material) now all guard on
  `emissive_contribution_is_authored`, and the helper's own doc 20 lines below
  says so. The `None` doc is the pre-#2591 text, still describing the behaviour
  the fix removed.

  (b) `emissive_contribution_is_authored`'s doc says it is *"Shared by all three
  `EmissiveSource::{Material,Lighting,Effect}` set-sites (#2591 / SKY-D7-03)"*.
  There are four writers, and `asset_provider/material.rs`'s BGEM merge is not
  one of them:

  ```rust
  material.emissive_color = bgem.base_color;
  material.emissive_mult = bgem.base_color_scale;
  material.emissive_source =
      byroredux_core::ecs::components::material::EmissiveSource::Effect;
  ```

  A BGEM with `base_color == [0,0,0]` or `base_color_scale == 0.0` is still
  tagged `Effect`, which is the exact degeneration ("has an effect shader"
  rather than "authored an emissive") that #2591 fixed on the other three.
- **Evidence**: `git log -S emissive_contribution_is_authored` →
  `aedde151 Fix #2589 #2590 #2591: … unconditional EmissiveSource::Lighting tag`;
  the four set-sites listed above; the two doc blocks quoted.
- **Impact**: Zero on Skyrim (no BGEM/BGEM merge on that title, and my census
  shows Skyrim's discriminator behaves per the post-#2591 rule). The behavioural
  half reaches FO4+ only, and nothing in the render path reads `emissive_source`
  today, so the practical cost is that two adjacent doc blocks on the canonical
  enum state opposite rules, and the discriminator is not uniformly meaningful
  across producers.
- **Suggested Fix**: delete/replace the stale `None` paragraph (its #2641 citation
  no longer describes HEAD); route the BGEM merge's tag through
  `emissive_contribution_is_authored` like the other three, or amend the helper
  doc to say three of four writers use it and why the fourth does not.
- **Related**: #2591 (CLOSED, the fix that made this stale), #2641 (cited by the
  stale text), #3337 (OPEN, a different claim in `nifal.md` §4).

---

---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (`/audit-skyrim`, 7 dimensions),
verified against HEAD `558af58c` on a full vanilla Skyrim SE install.*
