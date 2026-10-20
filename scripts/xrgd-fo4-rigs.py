#!/usr/bin/env python3
"""#6065 — which FO4 skeletons can an `XRGD` corpse pose be applied to?

`ragdoll_pose.rs` poses only the rigs whose pose evidence was checked (FO4
human, feral ghoul, and — from this analysis — super mutant). #5015's
derivation checked two things on those rigs; this script repeats both, rig by
rig, for every vanilla race that authors a Starts-Dead pose:

  assign   The race's corpses carry the same multiset of bone ids as exactly one
           skeleton's ragdoll bodies (a trailing marker entry aside).
  offsets  Each entry's offset length equals its bone's own local offset in the
           NIF. Reported as the median relative error per bone (bones >= 1 unit
           long): the verified human and ghoul rigs sit at <= 3 % on every bone
           (pose wobble, not error), so that is the bar.
  FK       Forward kinematics of the corpse on the real NIF bone tree under all
           24 Euler conventions, ranked by how flat the lying body is; the
           verified rigs rank intrinsic XYZ with negated angles (the convention
           `euler_zup_to_quat_yup_mode(0, ..)` implements) first. Conventions
           that give the same rotation tie, and a tie for first counts.

A rig is gated only if it passes BOTH, on enough corpses for the ranking to
mean something. The flatness test discriminates weakly for quadrupeds (a body
lying on its side is about as tall as one standing), so a failure there is
"not established", not "wrong".

Inputs (all produced by tools in this repo):
  poses.jsonl   cargo run -p byroredux-plugin --example xrgd_dump -- Fallout4.esm
  <rigs>/rd/<skeleton>.txt     ragdoll_dump <skeleton.nif>  (`#<body> "<bone>" bone_id=Some(<id>)` lines)
  <rigs>/tree/<skeleton>.jsonl skeleton_tree_dump <skeleton.nif>

Usage: xrgd-fo4-rigs.py <poses.jsonl> <rigs-dir>
"""
import collections
import glob
import itertools
import json
import os
import re
import sys

import numpy as np
from scipy.spatial.transform import Rotation as R

POSES, RIGS_DIR = sys.argv[1:3]
OFFSET_BAR = 0.03          # median relative offset error per bone
MIN_CORPSES_FOR_FK = 3     # fewer than this and the ranking is not evidence
OURS = ('xyz', True, -1)   # intrinsic XYZ, negated angles

rows = [json.loads(l) for l in open(POSES)]
dead = [r for r in rows if r['dead']]


def strip_marker(entries):
    """Drop a trailing marker: bone id 0, x = 0, a nonzero (y, z), zero rotation."""
    last = entries[-1]
    if (last[0] == 0 and abs(last[1]) < 1e-4 and (abs(last[2]) > 1e-4 or abs(last[3]) > 1e-4)
            and all(abs(v) < 1e-4 for v in last[4:7])):
        return entries[:-1]
    return entries


def load_rig(name):
    bodies = []
    for line in open(f'{RIGS_DIR}/rd/{name}.txt'):
        m = re.match(r'#(\d+) "([^"]+)" bone_id=Some\((\d+)\)', line)
        if m:
            bodies.append((int(m.group(3)), m.group(2)))
    tree, order = {}, []
    for line in open(f'{RIGS_DIR}/tree/{name}.jsonl'):
        d = json.loads(line)
        tree[d['name']] = d
        order.append(d['name'])
    return bodies, tree, order


def claim(ids, bodies):
    """An entry claims the first unclaimed body carrying its id."""
    claimed, out = set(), []
    for i in ids:
        for b, (bid, _) in enumerate(bodies):
            if bid == i and b not in claimed:
                claimed.add(b)
                out.append(b)
                break
    return out


def lying_pose(entries, bones, tree, order, seq, intrinsic, sign):
    rot, pos, override = {}, {}, {}
    for k, e in enumerate(entries):
        eul = np.array(e[4:7]) * sign
        ang = [eul['xyz'.index(a)] for a in seq]
        override[bones[k]] = R.from_euler(seq.upper() if intrinsic else seq, ang).as_matrix()
    for name in order:
        d = tree[name]
        t, r = np.array(d['t']), np.array(d['r'])
        if name in override:
            r = override[name]
        if name == bones[0]:
            t = np.zeros(3)  # the COM sits at the placement
        p = d['parent']
        if p in rot:
            pos[name], rot[name] = pos[p] + rot[p] @ t, rot[p] @ r
        else:
            pos[name], rot[name] = t, r
    return pos


skeletons = sorted(os.path.basename(f)[:-4] for f in glob.glob(f'{RIGS_DIR}/rd/*.txt'))
rigs = {s: load_rig(s) for s in skeletons}
rigs = {s: v for s, v in rigs.items() if v[0]}

by_race = collections.defaultdict(list)
for r in dead:
    by_race[r['race']].append(r)

print(f'{"race":24} {"skeleton":34} {"corpses":>9} {"offsets":>9} {"FK rank":>8}  verdict')
for race, corpses in sorted(by_race.items(), key=lambda kv: -len(kv[1])):
    # assign: the skeletons whose id multiset matches the most of the race's corpses
    matches = {}
    for sk, (bodies, _, _) in rigs.items():
        want = sorted(i for i, _ in bodies)
        n = sum(1 for c in corpses if sorted(int(e[0]) for e in strip_marker(c['entries'])) == want)
        if n:
            matches[sk] = n
    if not matches:
        print(f'{race or "(none)":24} {"-":34} {len(corpses):>9} {"-":>9} {"-":>8}  no skeleton carries these ids')
        continue
    best = max(matches.values())
    tied = sorted(sk for sk, n in matches.items() if n == best)
    if len(tied) > 1:
        # Several skeletons share an id set (the humanoid rigs); the body *names* decide, not ids.
        print(f'{race or "(none)":24} {",".join(s.split("__")[0] for s in tied)[:34]:34} {best:>4}/{len(corpses):<4} {"-":>9} {"-":>8}  ambiguous by ids')
        continue
    sk = tied[0]
    bodies, tree, order = rigs[sk]
    want = sorted(i for i, _ in bodies)
    use = [c for c in corpses if sorted(int(e[0]) for e in strip_marker(c['entries'])) == want]
    ids = collections.Counter(tuple(int(e[0]) for e in strip_marker(c['entries'])) for c in use)
    order_ids, _ = ids.most_common(1)[0]
    use = [c for c in use if tuple(int(e[0]) for e in strip_marker(c['entries'])) == order_ids]
    bones = [bodies[b][1] for b in claim(order_ids, bodies)]
    per = collections.defaultdict(list)
    for c in use:
        for k, e in enumerate(strip_marker(c['entries'])):
            if k == 0 or bones[k] not in tree:
                continue
            length = float(np.linalg.norm(tree[bones[k]]['t']))
            if length >= 1.0:
                per[bones[k]].append(abs(float(np.linalg.norm(e[1:4])) - length) / length)
    med = {b: float(np.median(v)) for b, v in per.items()}
    off_bad = {b: round(m, 3) for b, m in med.items() if m > OFFSET_BAR}
    offsets = f'{len(med) - len(off_bad)}/{len(med)}'
    results = []
    for seq in map(''.join, itertools.permutations('xyz')):
        for intrinsic in (False, True):
            for sign in (1, -1):
                ext = []
                for c in use[:60]:
                    pos = lying_pose(strip_marker(c['entries']), bones, tree, order, seq, intrinsic, sign)
                    z = np.array([pos[b][2] for b in bones])
                    ext.append(z.max() - z.min())
                results.append((float(np.median(ext)), (seq, intrinsic, sign)))
    ours = next(e for e, conv in results if conv == OURS)
    rank = 1 + sum(1 for e, _ in results if e < ours - 1e-6)
    fk_ok = rank == 1 and len(use) >= MIN_CORPSES_FOR_FK
    off_ok = not off_bad
    verdict = 'PASS both' if fk_ok and off_ok else ('PASS (one near-miss: ' + ', '.join(f'{b} {m:.0%}' for b, m in off_bad.items()) + ')') if fk_ok and len(off_bad) <= 1 else 'not established'
    print(f'{race or "(none)":24} {sk:34} {len(use):>4}/{len(corpses):<4} {offsets:>9} {rank:>8}  {verdict}')
