import ctypes as c,ctypes.util,time,json,statistics,os
from pathlib import Path
x=c.CDLL(ctypes.util.find_library('X11'));t=c.CDLL(ctypes.util.find_library('Xtst'))
x.XOpenDisplay.argtypes=[c.c_char_p];x.XOpenDisplay.restype=c.c_void_p
d=x.XOpenDisplay(None);assert d,os.environ.get('DISPLAY')
x.XDefaultRootWindow.argtypes=[c.c_void_p];x.XDefaultRootWindow.restype=c.c_ulong
w=x.XDefaultRootWindow(d)
x.XGetImage.argtypes=[c.c_void_p,c.c_ulong,c.c_int,c.c_int,c.c_uint,c.c_uint,c.c_ulong,c.c_int];x.XGetImage.restype=c.c_void_p
x.XGetPixel.argtypes=[c.c_void_p,c.c_int,c.c_int];x.XGetPixel.restype=c.c_ulong
x.XDestroyImage.argtypes=[c.c_void_p]
x.XWarpPointer.argtypes=[c.c_void_p,c.c_ulong,c.c_ulong,c.c_int,c.c_int,c.c_uint,c.c_uint,c.c_int,c.c_int]
x.XFlush.argtypes=[c.c_void_p];x.XSync.argtypes=[c.c_void_p,c.c_int]
t.XTestFakeButtonEvent.argtypes=[c.c_void_p,c.c_uint,c.c_int,c.c_ulong]
def pixel(px,py):
 i=x.XGetImage(d,w,px,py,1,1,0xffffffff,2);assert i
 v=x.XGetPixel(i,0,0);x.XDestroyImage(i);return v
def click(px,py):
 x.XWarpPointer(d,0,w,0,0,0,0,px,py);x.XSync(d,0);time.sleep(.002);t.XTestFakeButtonEvent(d,1,1,0);t.XTestFakeButtonEvent(d,1,0,0);x.XSync(d,0)
# All three tabs have already loaded histories. Choose Conversation before sampling.
click(895,176);time.sleep(.3)
click(105,103);time.sleep(.3)
selected=pixel(28,95);plain=pixel(240,95);assert selected!=plain,(selected,plain)
samples=[]
for i in range(30):
 target_b=i%2==0;px=28 if target_b else 240
 start=time.monotonic_ns();click(294 if target_b else 105,103)
 deadline=time.monotonic()+2
 while pixel(px,95)!=plain:
  assert time.monotonic()<deadline,('frame did not select',i)
  time.sleep(.002)
 samples.append((time.monotonic_ns()-start)/1e6);time.sleep(.06)
pids=[2676487,2465129,2465154,2480644,2480658]
rss={str(pid):next(line for line in Path(f'/proc/{pid}/status').read_text().splitlines() if line.startswith('VmRSS:')) for pid in pids}
out={'display':os.environ['DISPLAY'],'build':'debug, unoptimized','samples_ms':samples,'median_ms':statistics.median(samples),'p95_ms':sorted(samples)[28],'max_ms':max(samples),'rss':rss,'method':'XTest click to selected tab background in XGetImage, 2ms sampling; warmed 2 Codex workspace views, 3 open tabs; no Paseo baseline, no content loading latency claim'}
Path('/tmp/fp-tabs-real/latency.json').write_text(json.dumps(out,indent=2));print(json.dumps(out))
