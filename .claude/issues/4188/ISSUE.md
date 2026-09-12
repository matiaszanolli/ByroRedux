# CONC-D6-02: The teardown doc's "four load-bearing orderings" list includes one the code does not implement and does not need

Labels: low,sync,doc-rot,documentation

**Description**: The corrected teardown header names ordering (b) as "placeholders after the passes whose descriptors name them," alongside three orderings that are real VUID-level constraints. The code does not implement it: `placeholder_ao`/`placeholder_caustic_sink` are destroyed before composite/caustic/volumetrics/bloom/water_caustic_accum/svgf/taa/gbuffer — several of which hold descriptor sets naming those placeholder views. The site comment actually justifies the position by `device_wait_idle`, not by ordering — consistent with the corrected model that Vulkan imposes no such cross-subsystem ordering once `device_wait_idle` has run.

**Evidence**:
`crates/renderer/src/vulkan/context/teardown.rs`: placeholder destroys precede pass destroys; the site's own comment contradicts the header's claim.

**Impact**: No runtime hazard. The cost is to the invariant's credibility: a documented "load-bearing ordering" the code visibly violates trains readers (and the next audit) to discount the whole list, including the three entries that *are* load-bearing.

**Related**: #3658/CONC-D6-2026-08-30-02 (the correction that produced this list), #2141/#2142 (the placeholders).

**Suggested Fix**: Demote (b) from the header list to "placeholders are allocator-backed, so must be destroyed before `self.allocator.take()`; their position relative to the passes naming them is unconstrained post-`device_wait_idle`," leaving three genuinely load-bearing orderings; reword the site comment to match.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_CONCURRENCY_2026-09-11.md`.*
