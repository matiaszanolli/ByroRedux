# #3898 — investigation

## The filed premise needed correcting before it could be fixed

The issue said the `greyscale_lut.is_none()` gate "discards" the BGSM's enable
bit and framed that as straightforwardly wrong. Tracing it showed the gate is
doing **two** jobs, and only one of them is a defect:

1. **Precedence among BGSMs in the template chain** — `merge_external_material`
   walks `resolved.walk()` closest-first, and `fill` is first-non-empty-wins.
   Checking `is_none()` before the `fill` is what makes the enable bit come from
   the *same* BGSM that supplies the winning texture. That is correct and is
   exactly what #2108 intended.

2. **Accidentally excluding a NIF-filled slot** — #2997 made the NIF's own
   slot 3 populate `greyscale_lut` on FO4, so the gate became permanently false
   on the affected meshes and no BGSM in the chain ever got to speak.

So the naive fix (delete the `is_none()` check, or OR unconditionally) would
have **regressed #2108**: an ancestor BGSM could then enable a remap that a
closer BGSM had deliberately authored off.

## The coherence trap

There is a second-order problem the issue text did not anticipate. When the NIF
has already filled the role, the BGSM's greyscale *texture* loses — `fill` will
not overwrite it. Taking the enable bit from that BGSM means the enable comes
from a source whose texture lost, while the LUT actually sampled is the NIF's.

Resolved by separating the two concepts explicitly:

- the **texture** is a resource — closest non-empty source wins, unchanged
- the **enable bit** is a statement about the material — either source may
  assert it, neither may silently clear the other's

Hence the three-way split now in the code, keyed on `nif_supplied_greyscale_lut`
(captured before the walk) rather than on `is_some()`:

| situation | enable bit |
|---|---|
| this BGSM wins the slot | assignment — it is authoritative for both (#2108, unchanged) |
| the NIF won the slot | `\|=` — OR the BGSM's bit onto the NIF's SLSF1 bit (#3897) |
| a closer BGSM won the slot | untouched — an ancestor's bit stays irrelevant (#2108) |

`bgsm_winning_the_slot_still_authors_the_enable_bit_off` pins row 1,
`bgsm_palette_enable_survives_a_nif_supplied_greyscale_lut` row 2, and
`bgsm_without_palette_bit_does_not_disable_a_nif_enabled_remap` pins that the OR
is one-way.

## Relationship to #3897

The audit called these "two independent gates" and said closing either alone
changes nothing on screen. That is right, but the split of work is uneven:
#3897 (the missing NIF-side SLSF1 producer) covers the bulk of the measured
population — 30,155 of 30,166 FO4 properties carry the SLSF1 bit *and* a
populated slot 3. #3898 covers the residual where the BGSM asks for the remap
and the NIF does not.

Practical consequence: **#3897 alone is enough to see the feature come back on
most FO4 content**. If a future bisect makes it look like #3898 "did nothing
visible", that is expected — it is a correctness completion, not the bulk fix.

## Not changed

The `bgsm_`-prefixed field names (`bgsm_greyscale_lut_enabled` / `_color` /
`_is_alpha`) now carry values that may originate from the NIF rather than a
BGSM, so the prefix is a misnomer. Renaming touches many call sites and is
orthogonal to the bug; the doc comments at both producers say the triple is
source-agnostic instead. Worth a follow-up if the names cause confusion.
