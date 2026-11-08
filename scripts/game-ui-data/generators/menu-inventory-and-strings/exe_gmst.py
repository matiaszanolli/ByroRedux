# usage: python3 -I exe_gmst.py <exe> <strings.tsv> -> prints name, in_exe, default text (if the layout resolves)
import struct, sys, re
exe = open(sys.argv[1],'rb').read()
pe = struct.unpack_from('<I', exe, 0x3c)[0]
nsec = struct.unpack_from('<H', exe, pe+6)[0]
optsz = struct.unpack_from('<H', exe, pe+20)[0]
base = struct.unpack_from('<I', exe, pe+24+28)[0]
secs=[]
off = pe+24+optsz
for i in range(nsec):
    name = exe[off:off+8].rstrip(b'\0').decode('latin1')
    vsz, va, rsz, rptr = struct.unpack_from('<IIII', exe, off+8)
    secs.append((name, va, vsz, rptr, rsz)); off+=40
def f2v(fo):
    for n,va,vsz,rp,rs in secs:
        if rp <= fo < rp+rs: return base+va+(fo-rp)
def v2f(v):
    for n,va,vsz,rp,rs in secs:
        if base+va <= v < base+va+max(vsz,rs) and v-base-va < rs: return rp+(v-base-va)
    return None
def cstr(fo):
    e = exe.index(b'\0', fo); return exe[fo:e].decode('latin1')
print('#sections', [(s[0],hex(s[1]),hex(s[2])) for s in secs], file=sys.stderr)
print('name\tin_exe_string\tsetting_object\texe_default')
for line in open(sys.argv[2], encoding='utf-8').read().splitlines()[1:]:
    ent = line.split('\t')[0]
    name = ent[2:-1]   # strip '&-' and ';'
    pat = b'\0'+name.encode()+b'\0'
    hits=[m.start()+1 for m in re.finditer(re.escape(pat), exe)]
    if not hits:
        print(f'{name}\tno\t\t'); continue
    vas=[f2v(h) for h in hits]
    res=''
    obj=''
    for va in vas:
        ptr = struct.pack('<I', va)
        for m in re.finditer(re.escape(ptr), exe):
            fo = m.start()
            if fo < 8: continue
            # candidate: value pointer 4 bytes before the name pointer
            vp = struct.unpack_from('<I', exe, fo-4)[0]
            vf = v2f(vp)
            if vf is not None:
                s = cstr(vf)
                if s and all(32<=ord(c)<127 or c in '\t\n' for c in s) and len(s)<200:
                    res = s; obj=hex(f2v(fo-8)); break
        if res: break
    print(f'{name}\tyes\t{obj}\t{res}')
