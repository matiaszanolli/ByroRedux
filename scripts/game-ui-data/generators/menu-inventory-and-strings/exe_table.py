# usage: python3 -I exe_table.py <exe> <out.tsv> : every s*-named GMST default (name, exe default text) in the exe's setting table
import struct, sys, re
exe=open(sys.argv[1],'rb').read()
pe=struct.unpack_from('<I',exe,0x3c)[0]
nsec=struct.unpack_from('<H',exe,pe+6)[0]; optsz=struct.unpack_from('<H',exe,pe+20)[0]
base=struct.unpack_from('<I',exe,pe+24+28)[0]
secs=[];off=pe+24+optsz
for i in range(nsec):
    n=exe[off:off+8].rstrip(b'\0').decode(); vsz,va,rsz,rp=struct.unpack_from('<IIII',exe,off+8); secs.append((n,va,vsz,rp,rsz)); off+=40
def v2f(v):
    for n,va,vsz,rp,rs in secs:
        if base+va<=v<base+va+rs: return rp+v-base-va
def f2v(fo):
    for n,va,vsz,rp,rs in secs:
        if rp<=fo<rp+rs: return base+va+fo-rp
names={}
for m in re.finditer(rb'\0(s[A-Z][A-Za-z0-9_]{1,60})\0',exe):
    names[f2v(m.start()+1)]=m.group(1).decode()
res={}
for n,va,vsz,rp,rs in secs:
    if n not in ('.data','.rdata'): continue
    blob=exe[rp:rp+rs-(rs%4)]
    for i,(w,) in enumerate(struct.iter_unpack('<I',blob)):
        nm=names.get(w)
        if nm and i>=1:
            prev=struct.unpack_from('<I',blob,(i-1)*4)[0]
            vf=v2f(prev)
            if vf is None: continue
            e=exe.find(b'\0',vf)
            s=exe[vf:e]
            if 0<len(s)<300 and all(32<=c<127 or c in (9,10,13) for c in s):
                res.setdefault(nm,s.decode())
with open(sys.argv[2],'w',encoding='utf-8') as f:
    f.write('name\texe_default\n')
    for k in sorted(res,key=str.lower): f.write(f'{k}\t{res[k]}\n')
print(len(names),'candidate names;',len(res),'with resolved default text')
