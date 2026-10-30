import re, glob, statistics, sys, os
base = sys.argv[1]
keys = ["main_render","svgf","shadow_filter","sky_cube","cluster_cull","composite","bloom","taa","presentation","volumetrics","volumetrics_inject","volumetrics_integrate"]
scenes = {}
for f in sorted(glob.glob(os.path.join(base, "*_r[0-9].log"))):
    name = re.sub(r"_r\d+\.log$", "", os.path.basename(f))
    line = None
    for l in open(f, errors="replace"):
        if l.startswith("bench:"): line = l
    if not line: continue
    d = dict(re.findall(r"(\w+)=([^\s]+)", line))
    scenes.setdefault(name, []).append(d)
print("scene\tpass\tmedian_ms\tmin_ms\tmax_ms\truns")
for s, runs in scenes.items():
    for k in keys:
        v = [float(r["gpu_"+k]) for r in runs if "gpu_"+k in r]
        print(f"{s}\t{k}\t{statistics.median(v):.3f}\t{min(v):.3f}\t{max(v):.3f}\t{len(v)}")
    r0 = runs[0]
    print(f"{s}\t#meta\tsim_time_s={r0.get('sim_time_s')}\tcamera_pos={r0.get('camera_pos')}\tfroxel={r0.get('froxel_x')}x{r0.get('froxel_y')}x{r0.get('froxel_z')}\ttransport_armed={r0.get('transport_armed')} state_hash={[r.get('state_hash') for r in runs]}")
