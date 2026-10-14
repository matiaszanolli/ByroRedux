#!/usr/bin/env python3
"""#5015 — recover the XRGD pose layout from the vanilla Skyrim corpus.

Inputs (all produced by tools in this repo; see byroredux/src/ragdoll_pose.rs):
  poses.jsonl     cargo run -p byroredux-plugin --example xrgd_dump -- Skyrim.esm
  skel.jsonl      cargo run -p byroredux-nif --example skeleton_tree_dump -- skeleton.nif
  hkx_ragdoll.txt cargo run -p byroredux-hkx --example skeletons_dump -- skeleton.hkx | grep '^1 '

Usage: xrgd-analysis.py <poses.jsonl> <skel.jsonl> <hkx_ragdoll.txt>

Part 1 ranks the 24 Euler conventions by how well they lay a corpse on the
floor (forward kinematics over the 18 humanoid entries; the winner is the
Gamebryo XYZ product with negated angles). Part 2 measures each entry's
rotation against the hkx ragdoll skeleton's and the NIF bone's own rest
rotation (arm sides, and the clavicle-parented upper arm).
"""
import sys
POSES, SKEL, HKX = sys.argv[1:4]
import json, itertools, numpy as np
from scipy.spatial.transform import Rotation as R
rows=[json.loads(l) for l in open(POSES)]
dead=[r for r in rows if r['dead'] and len(r['entries'])==18 and not r['creature']]
print(len(dead),'humanoid 18-entry corpses')
parent={1:0,2:1,3:2,4:0,5:4,6:5,7:0,8:7,9:8,10:9,11:10,12:9,13:12,14:13,15:9,16:15,17:16}
def fk(entries, seq, intrinsic, sign):
    P=[None]*18; Rw=[None]*18
    for i,e in enumerate(entries):
        pos=np.array(e[1:4]); eul=np.array(e[4:7])*sign
        ang=[eul['xyz'.index(a)] for a in seq]
        rot=R.from_euler(seq.upper() if intrinsic else seq, ang).as_matrix()
        if i==0: P[0]=np.zeros(3); Rw[0]=rot
        else:
            p=parent[i]; P[i]=P[p]+Rw[p]@pos; Rw[i]=Rw[p]@rot
    return np.array(P)
res=[]
for seq in map(''.join, itertools.permutations('xyz')):
    for intr in (False,True):
        for sign in (1,-1):
            mins=[];ext=[];hz=[]
            for r in dead:
                P=fk(r['entries'],seq,intr,sign)
                mins.append(P[:,2].min()); ext.append(P[:,2].max()-P[:,2].min()); hz.append(np.ptp(P[:,:2],axis=0).max())
            res.append((np.median(ext),np.median(mins),np.median(hz),seq,intr,sign))
res.sort()
for x in res[:10]: print(f"ext_med={x[0]:.1f} min_med={x[1]:.1f} horiz_med={x[2]:.1f} seq={x[3]} intr={x[4]} sign={x[5]}")
print('...'); 
for x in res[-3:]: print(f"ext_med={x[0]:.1f} min_med={x[1]:.1f} horiz_med={x[2]:.1f} seq={x[3]} intr={x[4]} sign={x[5]}")

print("\n--- rest-pose proximity with the winning convention (intrinsic XYZ, negated) ---")
import re
hk={}
for line in open(HKX):
    m=re.match(r'1\s+(\d+)\s+"Ragdoll_(.*?)".*rotation: \[(.*?)\]',line)
    if m: hk[m.group(2)]=np.array([float(x) for x in m.group(3).split(',')])
names=['COM','NPC L Thigh [LThg]','NPC L Calf [LClf]','NPC L Foot [Lft ]','NPC R Thigh [RThg]','NPC R Calf [RClf]','NPC R Foot [Rft ]','NPC Spine [Spn0]','NPC Spine1 [Spn1]','NPC Spine2 [Spn2]','NPC Neck [Neck]','NPC Head [Head]']
armR=['NPC R UpperArm [RUar]','NPC R Forearm [RLar]','NPC R Hand [RHnd]']; armL=['NPC L UpperArm [LUar]','NPC L Forearm [LLar]','NPC L Hand [LHnd]']
def q_of(e):
    eul=-np.array(e[4:7]); return R.from_euler('XYZ',eul).as_quat()
def dist(q1,q2):
    return np.degrees(2*np.arccos(min(1.0,abs(np.dot(q1,q2)))))
def med(idx,name):
    return np.median([dist(q_of(r['entries'][idx]),hk[name]) for r in dead])
for i,n in enumerate(names[1:],1): print(f"entry {i:2} {n:24} median dist to rest {med(i,n):6.1f} deg")
for assign,(a,b) in {'12-14=R,15-17=L':(armR,armL),'12-14=L,15-17=R':(armL,armR)}.items():
    tot=[med(12+k,a[k]) for k in range(3)]+[med(15+k,b[k]) for k in range(3)]
    print(assign,[round(x,1) for x in tot],'sum',round(sum(tot),1))

print("\n--- distance of XRGD local rotation to the NIF bone's own local rest rotation ---")
nif={}
for l in open(SKEL):
    d=json.loads(l); nif[d['name']]=np.array(d['r'])
def dist_m(Ma,Mb):
    c=(np.trace(Ma.T@Mb)-1)/2; return np.degrees(np.arccos(np.clip(c,-1,1)))
allnames=names+armR+armL
order=[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17]
for i,n in zip(range(1,18), names[1:]+armR+armL):
    d=[dist_m(R.from_euler('XYZ',-np.array(r['entries'][i][4:7])).as_matrix(), nif[n]) for r in dead]
    print(f"entry {i:2} {n:24} median dist to NIF local rest {np.median(d):6.1f} deg  (p10 {np.percentile(d,10):5.1f})")
