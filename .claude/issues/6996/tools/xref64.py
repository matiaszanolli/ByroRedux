import sys, struct; sys.path.insert(0,'.')
from pe64 import PE64
p=PE64(sys.argv[1]); targets=[int(a,16) for a in sys.argv[2:]]
n,va,vs,ro,rs=[s for s in p.secs if s[0]=='.text'][0]
t=p.b[ro:ro+rs]; base=p.base+va
for i in range(len(t)-5):
    if t[i] in (0xE8,0xE9):
        d=(base+i+5+struct.unpack_from('<i',t,i+1)[0])&0xffffffffffffffff
        if d in targets: print(hex(base+i), 'call' if t[i]==0xE8 else 'jmp', hex(d))
