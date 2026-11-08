import re,glob,os,sys
rows={}
for g in ('xfnv','xfo3'):
    for p in sorted(glob.glob(g+'/*.xml')):
        t=open(p,errors='replace').read()
        m=re.search(r'<menu\s+name="([^"]+)"',t)
        if not m or '_prefabs_' in p: continue
        st=re.search(r'<stackingtype>\s*([^<]*?)\s*</stackingtype>',t)
        cl=re.search(r'<class>\s*([^<]*?)\s*</class>',t)
        rows[(g,m.group(1))]=(os.path.basename(p).replace('_','\\',1).replace('menus\\','menus\\'),(cl.group(1) if cl else '-'),(st.group(1) if st else '-'),p)
for k,v in sorted(rows.items()): print(k[0],k[1],v[1],v[2],v[3],sep='|')
