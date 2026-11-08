import re,glob,os,json
OUT='/mnt/data/src/gamebyro-redux/docs/data/game-ui/'
HDR=['menu','game','flags_raw','named_bits','pause_game','counts_toward_menu_mode','allows_save','engine_default_applied','source','note']
def w(fn,rows):
    with open(OUT+fn,'w',encoding='utf-8') as f:
        f.write('\t'.join(HDR)+'\n')
        for r in rows:
            assert len(r)==len(HDR),r
            f.write('\t'.join(str(x).replace('\t',' ').replace('\n',' ') for x in r)+'\n')
BITS=['PausesGame','AlwaysOpen','UsesCursor','UsesMenuContext','Modal','FreezeFrameBackground','OnStack','DisablePauseMenu','RequiresUpdate','TopmostRenderedMenu','UpdateUsesCursor','AllowSaving','RendersOffscreenTargets','InventoryItemMenu','DontHideCursorWhenTopmost','CustomRendering','AssignCursorToRenderer','ApplicationMenu','HasButtonBar','IsTopButtonBar','AdvancesUnderPauseMenu','RendersUnderPauseMenu','UsesBlurredBackground','CompanionAppAllowed','FreezeFramePause','SkipRenderDuringFreezeFrameScreenshot','LargeScaleformRenderCacheMode','UsesMovementToDirection']
def names(v,skip=()):
    return '|'.join('k'+BITS[i] for i in range(28) if v>>i&1 and BITS[i] not in skip) or 'kNone'
def yn(b): return 'yes' if b else 'no'

# ---------------- Skyrim ----------------
CL='https://github.com/Ryan-rsm-McKenzie/CommonLibSSE/blob/3282565a2c86535036f5a556758fb1cf8ab85225/include/RE/'
SK='https://github.com/ianpatt/skse64/blob/25b72352adb6543fa6d0bd3795780672b2e238e0/skse64/GameMenus.h'
CK='https://ck.uesp.net/wiki/UI_Script'
cl={}
b2={n:1<<int(i) for n,i in re.findall(r'(k\w+)\s*=\s*1 << (\d+)',open('cls/include/RE/I/IMenu.h').read())}
for p in sorted(glob.glob('cls/include/RE/*/*.h')):
    t=open(p).read()
    f=re.search(r'//\s*flags\s*=\s*(.*)',t); n=re.search(r'MENU_NAME\s*=\s*"([^"]*)"',t)
    if not (f and n): continue
    dep=re.search(r'//\s*menuDepth\s*=\s*(\d+)',t)
    v=0
    for x in f.group(1).split('|'):
        x=x.strip()
        if x!='kNone': v|=b2[x]
    cl[n.group(1)]=(v,p.replace('cls/include/RE/',''),dep.group(1) if dep else '-')
ck_names=['BarterMenu','Book Menu','Console','Console Native UI Menu','ContainerMenu','Crafting Menu','Credits Menu','Cursor Menu','Debug Text Menu','Dialogue Menu','Fader Menu','FavoritesMenu','GiftMenu','HUD Menu','InventoryMenu','Journal Menu','Kinect Menu','LevelUp Menu','Loading Menu','Lockpicking Menu','MagicMenu','Main Menu','MapMenu','MessageBoxMenu','Mist Menu','Overlay Interaction Menu','Overlay Menu','Quantity Menu','RaceSex Menu','Sleep/Wait Menu','StatsMenu','TitleSequence Menu','Top Menu','Training Menu','Tutorial Menu','TweenMenu']
extra=['Creation Club Menu','Mod Manager Menu','SafeZoneMenu']
skse={'Console':(0x807,'Console'),'HUD Menu':(0x18902,'HUDMenu'),'Dialogue Menu':(0x4400,'DialogueMenu'),'Main Menu':(0x581,'MainMenu'),'MagicMenu':(0xA489,'MagicMenu'),'InventoryMenu':(0x4400,'InventoryMenu'),'BarterMenu':(0xA489,'BarterMenu'),'RaceSex Menu':(0x709,'RaceSexMenu'),'MapMenu':(0x9005,'MapMenu')}
BASE='UI::GameIsPaused() = numPausesGame > 0 (CommonLibSSE src/RE/U/UI.cpp); numPausesGame += 1 if flags & 0x1 (SKSE64 MenuManager); CK defines menu mode as the game being paused (IsInMenuMode - Utility)'
rows=[]
for n in ck_names+extra:
    if n in cl:
        v,path,dep=cl[n]
        note=f'menuDepth={dep}. Counts toward menu mode = PausesGame bit (derived: {BASE}).'
        src=f'{CL}{path} (comment `// flags =`)'
        if n in skse:
            sv=skse[n][0]
            if sv==v: src+=f'; agrees with SKSE64 `// Flags - {sv:#x}` ({SK})'
            else:
                src+=f'; SKSE64 gives {sv:#x} ({SK})'
                note+=f' CONFLICT: SKSE64 comment {sv:#x} vs CommonLibSSE {v:#x}'+(' (diff = kApplicationMenu 0x20000, a later addition)' if v^sv==0x20000 else ' - SKSE64 InventoryMenu comment equals its DialogueMenu comment and looks stale; CommonLibSSE value kept.')
        if n=='HUD Menu': note+=' HUD: no PausesGame, so it does not count toward menu mode; kAlwaysOpen (0x2; FO4 CK name for the bit: DoNotDeleteOnClose).'
        if n=='Dialogue Menu': note+=' Dialogue does not pause (CK IsInMenuMode: "Dialogue does not pause the game").'
        rows.append((n,'skyrim-se',f'{v:#x}',names(v),yn(v&1),yn(v&1),yn(v&0x800),'no',src,note))
    elif n=='Console':
        v=0x807
        rows.append((n,'skyrim-se',f'{v:#x}',names(v),'yes','yes','yes','no',f'{SK} (`class Console`: `// Flags - 0x807`)',f'Only SKSE64 documents it; CommonLibSSE has no Console menu header. Counts toward menu mode = PausesGame bit (derived: {BASE}).'))
    else:
        rows.append((n,'skyrim-se','-','-','undocumented','undocumented','undocumented','yes',f'{CK} (name only); no flag source found in SKSE64 GameMenus.h or CommonLibSSE include/RE','No source documents this menu\'s flags. Only documented baseline: IMenu ctor flags = kNone (SKSE64 `flags // 1C - init\'d to 0`, CommonLibSSE `menuFlags{ Flag::kNone }`). Default to apply is the orchestrator\'s decision (PRD GUI-REG-004).'))
w('menu-flags-skyrim.tsv',rows)

# ---------------- Fallout 4 ----------------
F4='https://github.com/ianpatt/f4se/blob/6f6a7caa9aeaebc957a70d934b6ab6b80eb768b2/f4se/GameMenus.h'
F4CK='https://falloutck.uesp.net/wiki/MenuData_Struct_-_UI'
f4=json.load(open('f4_flags.json'))
rows=[]
seen=set()
for n,v,d,c in f4:
    seen.add(n)
    pause=v&1
    note=f'depth={d} context={c}. Source word is a bit-per-column snapshot of a live menu (0x40 kOnStack is runtime state; 0x04 kUsesCursor is set at runtime when kUpdateUsesCursor 0x400 is set, cf. CommonLibSSE src/RE/I/IMenu.cpp RefreshPlatform). Decoded MSB-first, columns A-D = bytes 3..0; validated against the CK-documented MessageBoxMenu 0x8018499 (+0x40 +0x04 = 0x080184dd). Counts toward menu mode = PausesGame bit (derived: F4SE UI::numPauseGame "isInMenuMode", FO4 CK IsInMenuMode - Utility).'
    eng='no'
    if n=='VATSMenu': note='OVERRIDE: the PausesGame bit is clear in the source word, and in any case the VATS menu never pauses the world on its own account (docs/prd/vats.md VATS-UI-007: world time during VATS comes from VatsState::world_time_scale alone). '+note
    if n=='MessageBoxMenu': note='Also CK-documented: combined menu flags 0x8018499 (falloutck MenuData Struct - UI, Notes). '+note
    if n in('HUDMenu','PowerArmorHUDMenu'): note='HUD: no PausesGame, so it does not count toward menu mode. '+note
    rows.append((n,'fallout4',f'{v:#010x}',names(v,skip=('OnStack',)),yn(pause),yn(pause),yn(v&0x800),eng,f'{F4} (`class IMenu`, comment table of per-menu flag words); bit names {F4} enum IMenu::kFlag_*; FO4 CK names for several bits {F4CK}',note))
tmpl=['ConsoleNativeUIMenu','CreditsMenu','GenericMenu','LooksMenu','MainMenu','MultiActivateMenu','ScopeMenu','TerminalHolotapeMenu','TerminalMenu','Workshop_CaravanMenu','HolotapeMenu']
for n in tmpl:
    src='https://falloutck.uesp.net/wiki/Template:Menus (name; local dump falloutck-uesp-wiki/Template/Menus.wiki)' if n!='HolotapeMenu' else 'https://github.com/libxse/commonlibf4/blob/7c8c6f8a349ac85abf6fc8a7e5a71732d9339954/include/RE/H/HolotapeMenu.h (MENU_NAME only)'
    rows.append((n,'fallout4','-','-','undocumented','undocumented','undocumented','yes',src+'; no flag source found in F4SE GameMenus.h, CommonLibF4 or libxse/commonlibf4','Not in the F4SE comment table. Only documented value: the F4SE custom-menu MenuData default MenuFlags = 0x801849D (FO4 CK MenuData Struct - UI) - a custom-menu default, not a vanilla value. Default to apply is the orchestrator\'s decision (PRD GUI-REG-004).'))
w('menu-flags-fallout4.tsv',rows)
print(len(rows))
