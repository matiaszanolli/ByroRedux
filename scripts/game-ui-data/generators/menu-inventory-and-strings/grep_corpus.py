import sys,re
import os; sys.path.insert(0,os.path.dirname(os.path.abspath(__file__)))
from lookup import src
pat=re.compile(sys.argv[1],re.I)
lim=int(sys.argv[2]) if len(sys.argv)>2 else 12
for g in ('fo3_exe','fnv_esm','fo3_esm','sky_tr','fo4_tr'):
    hits=[(k,v) for k,v in src[g] if pat.search(v)]
    print(f'[{g}] {len(hits)}', hits[:lim])
