import re,glob,os
exec(open('gen_flags.py').read().split('# ---------------- Skyrim')[0])
# entity values (Oblivion XML docs, CS wiki)
ent=open('/mnt/data/src/reference/cs-uesp-wiki/(main)/Oblivion XML Reference/Entities.wiki').read()
E={n:int(v,16) for n,v,m in re.findall(r'\|&amp;(\w+);\n\|(0x[0-9A-Fa-f]+)\n\|([^\n]*)',ent)}
# NVSE name->code
nv=open('nvse_GameUI.cpp').read()
NV={k.lower():None for k in re.findall(r'\{"(\w+)", kMenuType',nv)}
vals={}
cur=None
body=open('nvse_GameUI.h').read()
body=body[body.index('kMenuType_Min'):body.index('kMenuType_Max =')]
for l in body.splitlines():
    m=re.match(r'\s*(kMenuType_\w+)\s*(?:=\s*([^,]+))?,',l)
    if not m: continue
    n,v=m.groups()
    if v:
        v=v.strip(); cur=int(v,16) if v.startswith('0x') else vals[v]
    else: cur+=1
    vals[n]=cur
for k,c in re.findall(r'\{"(\w+)", (kMenuType_\w+)\}',nv): NV[k.lower()]=vals[c]
def pmap(lst):
    m={}
    for l in open(lst):
        l=l.rstrip('\n')
        m[l.replace('\\','_')]=l
    return m
PM={'xfnv':pmap('fnv_menus_xml.txt'),'xfo3':pmap('fo3_menus_xml.txt'),'xobl':pmap('obl_menus_xml.txt')}
def scan(d,primary_only=True):
    out={}
    for p in sorted(glob.glob(d+'/*.xml')):
        if '_prefabs_' in p or 'copy of' in p or '(2)' in p: continue
        t=open(p,errors='replace').read()
        m=re.search(r'<menu\s+name="([^"]+)"',t)
        if not m: continue
        st=re.search(r'<stackingtype>\s*([^<]*?)\s*</stackingtype>',t)
        cl=re.search(r'<class>\s*([^<]*?)\s*</class>',t)
        cls=cl.group(1) if cl else '-'
        if primary_only and not cls.startswith('&') and cls!='-': continue
        stv=st.group(1).replace('&','').replace(';','') if st else '-'
        path=p.split('/',1)[1].replace('menus_','menus\\').replace('_','\\',0)
        out.setdefault(m.group(1),[]).append((cls,stv,PM[d].get(p.split('/',1)[1],p.split('/',1)[1])))
    return out
GECKMM='https://geck.uesp.net/wiki/MenuMode (local dump geck-uesp-wiki/(main)/MenuMode.wiki): "With no parameter, this block type will run whenever the game is NOT in gamemode -- whenever ANY menu is being displayed"'
CSMM='https://cs.uesp.net/wiki/MenuMode_(Function) (local dump cs-uesp-wiki/(main)/MenuMode (Function).wiki): "With no parameter, this function will return 1 whenever the game is NOT in gamemode"'
STK='stackingtype trait values: &no_click_past;=0x66, &does_not_stack;=0x1778, &click_past;=0x65, &mixed_menu;=0x67 (https://cs.uesp.net/wiki/Oblivion_XML_Reference/Entities; trait itself "To be documented" at .../Traits). Values read from the Oblivion wiki; the FO3/FNV values are not independently sourced.'
def row(menu,game,cls,stv,mm_src,code,fname,hud,arch):
    bits=f'stackingtype={stv}' if stv!='-' else 'stackingtype=(absent)'
    if hud=='main':
        c='no'; cn=f'HUD menu: the HUD is displayed during normal play (GameMode), and MenuMode (no argument) is true only when the game is NOT in GameMode (see source); not counted per PRD GUI-EVT-006.'
    elif stv=='does_not_stack':
        c='undocumented'; cn='does_not_stack overlay (HUD-like); whether it counts toward MenuMode is not documented.'
    else:
        c='yes (generic)'; cn='Generic doc statement only (MenuMode with no argument is true whenever ANY menu is displayed); this menu is not individually documented.'
    pause='undocumented'
    extra=''
    if menu=='VATSMenu':
        pause='no (policy)'; extra=' VATS: no source documents a pause bit; per docs/prd/vats.md VATS-UI-007 the menu never pauses the world on its own account.'
    cdesc=f'menu code {code}' if code else 'no menu code known from NVSE/OBSE/CS tables'
    src=f'{fname} `<stackingtype>` trait of the menu document ({arch}, read with crates/bsa example bsa_extract_one); {mm_src}; {STK}'
    return [menu,game,bits,bits.replace('=',': '),pause,c,'undocumented','yes',src,f'{cdesc}. pause_game / allows_save: no pause or save bit is documented for this game in any source found (GECK, CS, xNVSE/OBSE headers: Menu has no flag member). {cn}{extra}']
# FO3 / FNV
fnv=scan('xfnv'); fo3=scan('xfo3')
rows=[]
names=sorted(set(fnv)|set(fo3))
for n in names:
    a=fnv.get(n); b=fo3.get(n)
    ent_name=None
    if a: cls=a[0][0]; stv=a[0][1]; fn=a[0][2]
    else: cls=b[0][0]; stv=b[0][1]; fn=b[0][2]
    code=None
    key=cls.strip('&;').lower() if cls.startswith('&') else n.lower()
    if key in NV: code=NV[key]
    elif n.lower() in NV: code=NV[n.lower()]
    if a and b and a[0][1]==b[0][1]: g='FO3,FNV'
    elif a and b: g='FO3,FNV(differs)'
    elif a: g='FNV'
    else: g='FO3'
    fnames=('FNV '+a[0][2] if a else '')+(' / FO3 '+b[0][2] if b else '')
    r=row(n,g,cls,stv,GECKMM,code,fnames,'main' if n=='HUDMainMenu' else '','Fallout - Misc.bsa of each game')
    if a and b and a[0][1]!=b[0][1]: r[2]=f'FNV stackingtype={a[0][1]}; FO3 stackingtype={b[0][1]}'
    if code and n=='HUDMainMenu': r[9]=r[9]
    rows.append(r)
# fix header for this file: same header
w('menu-flags-fnv-fo3.tsv',rows)
print(len(rows))
# Oblivion
obl=scan('xobl')
rows=[]
for n in sorted(obl):
    cls,stv,fn=obl[n][0]
    key=cls.strip('&;') if cls.startswith('&') else n
    code=E.get(key)
    r=row(n,'oblivion',cls,stv,CSMM,code,fn,'main' if n=='HUDMainMenu' else '','Oblivion - Misc.bsa')
    r[9]=r[9].replace('xNVSE/OBSE','OBSE')
    rows.append(r)
w('menu-flags-oblivion.tsv',rows)
print(len(rows))
