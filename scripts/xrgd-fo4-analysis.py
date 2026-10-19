#!/usr/bin/env python3
"""#5015 — recover the FO4 XRGD corpse-pose layout from the vanilla corpus.

Inputs (all produced by tools in this repo; see byroredux/src/ragdoll_pose.rs):
  poses.jsonl   cargo run -p byroredux-plugin --example xrgd_dump -- Fallout4.esm
  skel.jsonl    cargo run -p byroredux-nif --example skeleton_tree_dump -- skeleton.nif
  ragdoll.txt   cargo run -p byroredux-nif --example ragdoll_dump -- skeleton.nif
                (the `#<body> "<bone>" bone_id=Some(<id>)` lines)

Usage: xrgd-fo4-analysis.py <poses.jsonl> <skel.jsonl> <ragdoll.txt> <race>

Part 1 maps each corpse's entries to the skeleton's ragdoll bodies by bone id
(an entry claims the first unclaimed body carrying its id; two bodies can share
one) and checks the claim against the id multisets. Part 2 does forward
kinematics of the corpse on the real NIF bone tree under all 24 Euler
conventions and ranks them by how flat the lying body is; the winner on both
the FO4 human and the feral ghoul is intrinsic XYZ with negated angles, the same
convention the Skyrim derivation found (`euler_zup_to_quat_yup_mode(0, ..)`).
"""
import collections
import itertools
import json
import re
import sys

import numpy as np
from scipy.spatial.transform import Rotation as R

POSES, SKEL, RAGDOLL, RACE = sys.argv[1:5]

bodies = []  # (bone_id, bone name) in body (blob) order
for line in open(RAGDOLL):
    m = re.match(r'#(\d+) "([^"]+)" bone_id=Some\((\d+)\)', line)
    if m:
        bodies.append((int(m.group(3)), m.group(2)))
tree, order = {}, []
for line in open(SKEL):
    d = json.loads(line)
    tree[d['name']] = d
    order.append(d['name'])

rows = [json.loads(l) for l in open(POSES)]
dead = [r for r in rows if r['dead'] and r['race'] == RACE and len(r['entries']) >= len(bodies)]
ids_seen = collections.Counter(tuple(int(e[0]) for e in r['entries'][:len(bodies)]) for r in dead)
print(f'{len(dead)} {RACE} corpses; entry-id orders:', ids_seen.most_common(3))
body_ids = sorted(b[0] for b in bodies)
for ids, n in ids_seen.most_common(3):
    print(' ', n, 'corpses: id multiset equals the blob\'s:', sorted(ids) == body_ids)

def mapping(ids):
    claimed, out = set(), []
    for i in ids:
        for b, (bid, _) in enumerate(bodies):
            if bid == i and b not in claimed:
                claimed.add(b)
                out.append(b)
                break
    return out

order_ids = ids_seen.most_common(1)[0][0]
dead = [r for r in dead if tuple(int(e[0]) for e in r['entries'][:len(bodies)]) == order_ids]
bone_of_entry = [bodies[b][1] for b in mapping(order_ids)]
print('entry -> bone:', bone_of_entry)
ragdoll_bones = set(bone_of_entry)

def world(entries, seq, intrinsic, sign):
    rot, pos, override = {}, {}, {}
    for k, e in enumerate(entries[:len(bodies)]):
        eul = np.array(e[4:7]) * sign
        ang = [eul['xyz'.index(a)] for a in seq]
        override[bone_of_entry[k]] = R.from_euler(seq.upper() if intrinsic else seq, ang).as_matrix()
    for name in order:
        d = tree[name]
        t, r = np.array(d['t']), np.array(d['r'])
        if name in override:
            r = override[name]
        if name == bone_of_entry[0]:
            t = np.zeros(3)  # the COM sits at the placement
        p = d['parent']
        if p in rot:
            pos[name], rot[name] = pos[p] + rot[p] @ t, rot[p] @ r
        else:
            pos[name], rot[name] = t, r
    return pos

results = []
for seq in map(''.join, itertools.permutations('xyz')):
    for intrinsic in (False, True):
        for sign in (1, -1):
            ext = []
            for r in dead[:150]:
                z = np.array([world(r['entries'], seq, intrinsic, sign)[b][2] for b in ragdoll_bones])
                ext.append(z.max() - z.min())
            results.append((np.median(ext), seq, intrinsic, sign))
results.sort()
print('\nlying-body vertical extent, median over corpses (lower = flatter):')
for ext, seq, intrinsic, sign in results[:6]:
    print(f'  {ext:6.1f}  seq={seq} intrinsic={intrinsic} sign={sign}')
print('  ...')
for ext, seq, intrinsic, sign in results[-2:]:
    print(f'  {ext:6.1f}  seq={seq} intrinsic={intrinsic} sign={sign}')
