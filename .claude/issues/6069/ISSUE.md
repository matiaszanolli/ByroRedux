# #6069 (snapshot as filed)

Split from #5015.

**Split from #5015** (Skyrim half).

## Description
Skyrim creature corpses are not posed: the entry-to-body order and rotation frames are established only for the 18-body humanoid rig (`HUMANOID_RAGDOLL_BODIES`). Entry count equalling body count does not establish the order, so the gate refuses every creature skeleton.

## Evidence (Skyrim.esm, 1,077 Starts-Dead poses)
Unposed, largest first: DwarvenSpiderRace 65 (entry counts 22/30/34/58 vs 30 bodies), SkeeverRace 54 (21 entries = 21 bodies on all 54, order unverified), Deer 31 (21/24/25 vs 24), Goat 16, Horse 14, Wolf 18, FrostbiteSpider 12 (58 vs 48 bodies), DwarvenSphereCenturion 20 (14/15 vs 14), plus a tail of 1–7 each (Chaurus, Fox, Werewolf, Horker, Troll, Mudcrab, Dragon, Bear, ...). Skyrim has no per-body bone id in its classic ragdoll bodies to map by (unlike FO4), so the order has to be derived.

## Approach
Per skeleton, as for the humanoid: recover the body order from the NIF `bhkRigidBody` order and bone-length agreement with the ragdoll `hkaSkeleton` (`decode_skeletons`, second skeleton), confirm the Euler mode by floor contact, and add the verified rig's body-name set to the gate. Start with the skeever (count already matches on all 54).

## Related
#5015, `scripts/xrgd-analysis.py`.

## Completeness Checks
- [ ] **SIBLING**: each rig verified individually; no widening of the name gate on a count match
- [ ] **TESTS**: body-set test and census pin per rig
