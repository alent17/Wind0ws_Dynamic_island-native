import ctypes as c, time, subprocess, pathlib, json
from ctypes import wintypes as w
from PIL import ImageGrab
from interaction import find
ROOT=pathlib.Path(__file__).resolve().parents[1]
u=c.windll.user32
u.SetProcessDpiAwarenessContext(c.c_void_p(-4))
u.FindWindowW.argtypes=[w.LPCWSTR,w.LPCWSTR];u.FindWindowW.restype=w.HWND
u.GetWindowRect.argtypes=[w.HWND,c.POINTER(w.RECT)]
u.PostMessageW.argtypes=[w.HWND,w.UINT,w.WPARAM,w.LPARAM]
exe=ROOT/'target/release/isle-native.exe'
out=ROOT/'artifacts';out.mkdir(exist_ok=True)
proc=subprocess.Popen([str(exe),'--page','music','--attached','--reduced-motion','--paused','--exit-after','20','--log',str(out/'smoke.json')])
try:
 hwnd=0
 for _ in range(100):
  hwnd=find(proc.pid)
  if hwnd:break
  if proc.poll() is not None:raise RuntimeError('prototype exited early')
  time.sleep(.1)
 assert hwnd,'native HWND not created'
 time.sleep(1)
 bounds=w.RECT();u.GetWindowRect(hwnd,c.byref(bounds))
 ImageGrab.grab((bounds.left,bounds.top,bounds.right,bounds.bottom)).save(out/'music.png')
 # F7 changes to a long title; arrow keys focus volume then Enter.
 u.PostMessageW(hwnd,0x100,0x76,0);time.sleep(.2)
 ImageGrab.grab((bounds.left,bounds.top,bounds.right,bounds.bottom)).save(out/'long-title.png')
 u.PostMessageW(hwnd,0x100,0x27,0);u.PostMessageW(hwnd,0x100,0x0d,0);time.sleep(.3)
 ImageGrab.grab((bounds.left,bounds.top,bounds.right,bounds.bottom)).save(out/'volume.png')
 u.PostMessageW(hwnd,0x100,0x1b,0);u.PostMessageW(hwnd,0x100,0x1b,0);time.sleep(.3)
 ImageGrab.grab((bounds.left,bounds.top,bounds.right,bounds.bottom)).save(out/'compact.png')
 u.PostMessageW(hwnd,0x10,0,0)
 proc.wait(timeout=5)
 result=json.loads((out/'smoke.json').read_text())
 assert result['livePages']==0, 'Escape did not release the expanded page'
 assert result['pageGenerations']>=3, 'Navigation did not create expected pages'
 print(json.dumps(result))
finally:
 if proc.poll() is None:
  if hwnd:u.PostMessageW(hwnd,0x10,0,0)
  try:proc.wait(timeout=5)
  except subprocess.TimeoutExpired:proc.terminate()
