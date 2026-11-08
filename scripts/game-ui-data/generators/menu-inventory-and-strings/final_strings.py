# usage: python3 -I final_strings.py <game> <exe> <strings.tsv> <out.tsv> [hint_exe]
import struct, sys, re
game, exe_path, tsv, out = sys.argv[1:5]
hint_path = sys.argv[5] if len(sys.argv) > 5 else None
def load(p):
    exe = open(p,'rb').read()
    pe = struct.unpack_from('<I', exe, 0x3c)[0]
    nsec = struct.unpack_from('<H', exe, pe+6)[0]
    optsz = struct.unpack_from('<H', exe, pe+20)[0]
    base = struct.unpack_from('<I', exe, pe+24+28)[0]
    secs=[]; off=pe+24+optsz
    for i in range(nsec):
        vsz, va, rsz, rptr = struct.unpack_from('<IIII', exe, off+8); secs.append((va,vsz,rptr,rsz)); off+=40
    return exe, base, secs
def f2v(base,secs,fo):
    for va,vsz,rp,rs in secs:
        if rp <= fo < rp+rs: return base+va+(fo-rp)
def v2f(base,secs,v):
    for va,vsz,rp,rs in secs:
        if base+va <= v < base+va+rs: return rp+(v-base-va)
def default_text(p, name):
    """-> (exe_spelling or None, default text or None)"""
    exe, base, secs = load(p)
    spell=None
    for m in re.finditer(rb'\0('+re.escape(name.encode())+rb')\0', exe, re.I):
        spell = m.group(1).decode()
        va = f2v(base,secs,m.start()+1)
        for q in re.finditer(re.escape(struct.pack('<I', va)), exe):
            fo = q.start()
            if fo < 8: continue
            vp = struct.unpack_from('<I', exe, fo-4)[0]
            vf = v2f(base,secs,vp)
            if vf is None: continue
            e = exe.index(b'\0', vf); s = exe[vf:e].decode('latin1')
            if s and len(s) < 200 and all(32<=ord(c)<127 for c in s): return spell, s
    return spell, None
rows=[l.split('\t') for l in open(tsv,encoding='utf-8').read().splitlines()[1:]]
hdr = ['name','entity','occurrences','documents','example_document','gmst_match','gmst_value','exe_builtin','exe_default_value','exe_default_source','note','source']
res=[]
other_exe = hint_path
for ent,occ,docs,ex,gm,exact,val in rows:
    name = ent[2:-1]
    spell, txt = default_text(exe_path, name)
    src = 'esm' if gm=='yes' else ''
    hint = ''
    if txt is not None:
        dsrc = 'own exe .data setting table'
    elif other_exe:
        s2, t2 = default_text(other_exe, name)
        if t2 is not None: txt, dsrc = t2, 'FO3 exe (hint only; FNV exe value not readable)'
        else: dsrc=''
    else: dsrc=''
    note=''
    if gm=='yes' and exact=='no': note='case differs from ESM EditorID'
    ebuilt = 'no' if spell is None else ('yes' if spell==name else 'yes-case-differs:'+spell)
    if spell is not None and spell!=name: note=(note+'; ' if note else '')+'exe spelling '+spell
    res.append([name,ent,occ,docs,ex,gm,val if gm=='yes' else '',ebuilt,txt or '',dsrc,note])
# deterministic order: by name case-insensitive
res.sort(key=lambda r:r[0].lower())
srctag = f"menu_gmst_census strings (Fallout - Misc.bsa x {game} master ESM); py/final_strings.py (exe: {exe_path.split('/')[-1]})"
with open(out,'w',encoding='utf-8') as f:
    f.write('\t'.join(hdr)+'\n')
    for r in res: f.write('\t'.join(r+[srctag])+'\n')
print(game, len(res))
