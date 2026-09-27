"""Synthetic DPI/work-area tests plus actual HWND hide/minimize/restore.
Does not modify desktop resolution, monitor layout or Windows DPI preferences.
"""
import ctypes as c
from ctypes import wintypes as w
import json
import subprocess
import time
from interaction import ROOT, OUT, wait_window, close, u, g

u.ShowWindow.argtypes = [w.HWND, c.c_int]
u.IsWindowVisible.argtypes = [w.HWND]
u.GetWindowRgn.argtypes = [w.HWND, w.HRGN]
g.CreateRectRgn.restype = w.HRGN
g.DeleteObject.argtypes = [w.HANDLE]

def ready(hwnd):
    region = g.CreateRectRgn(0,0,0,0)
    try:
        for _ in range(100):
            if u.GetWindowRgn(hwnd, region)>1 and u.IsWindowVisible(hwnd):
                time.sleep(.05)
                return
            time.sleep(.05)
        raise AssertionError('first frame timeout')
    finally:
        g.DeleteObject(region)

def snapshot(hwnd, path):
    previous=path.stat().st_mtime_ns if path.exists() else 0
    u.PostMessageW(hwnd,0x803c,0,0)
    for _ in range(100):
        if path.exists() and path.stat().st_mtime_ns!=previous:
            try:return json.loads(path.read_text())
            except json.JSONDecodeError:pass
        time.sleep(.03)
    raise AssertionError('diagnostic snapshot timeout')

results=[]
exe=str(ROOT/'target/release/isle-native.exe')
for dpi in [96,120,144,192]:
    for small in [False,True]:
        for edge in ['top','right','bottom','left']:
            report=OUT/f'display-{dpi}-{small}-{edge}.json'
            args=[exe,'--page','music','--attached','--edge',edge,'--paused','--reduced-motion','--test-dpi',str(dpi),'--log',str(report)]
            if small:args+=['--test-work-area','320x240']
            proc=subprocess.Popen(args);hwnd=wait_window(proc)
            try:
                ready(hwnd)
                rect=w.RECT();u.GetWindowRect(hwnd,c.byref(rect))
                actual=rect.right-rect.left
                assert actual == (240 if small else dpi*5), (dpi,small,actual)
                data=snapshot(hwnd,report)
                assert abs(data['scale']-actual/480)<.0001, data
                assert data['livePages']==1 and data['rendererAlive'],data
                results.append({'dpi':dpi,'smallWorkArea':small,'edge':edge,'hostPixels':actual,'report':data})
            finally:close(proc,hwnd)

# Copy WM_DPICHANGED's suggested RECT while the message is valid, then resize
# the native render target and region together. This is still synthetic DPI.
dpi_report=OUT/'dpi-transition.json'
proc=subprocess.Popen([exe,'--page','music','--paused','--reduced-motion','--log',str(dpi_report)])
hwnd=wait_window(proc)
transitions=[]
try:
    ready(hwnd)
    for dpi in [120,144,192,96]:
        current=w.RECT();u.GetWindowRect(hwnd,c.byref(current))
        suggested=w.RECT(current.left,current.top,current.left+dpi*5,current.top+dpi*5)
        u.SendMessageW(hwnd,0x2e0,dpi|(dpi<<16),c.addressof(suggested))
        time.sleep(.15)
        data=snapshot(hwnd,dpi_report)
        assert abs(data['scale']-dpi/96)<.0001,data
        assert data['rendererAlive'] and data['livePages']==1,data
        transitions.append(data)
finally:close(proc,hwnd)

report=OUT/'hidden-state.json'
proc=subprocess.Popen([exe,'--page','timer','--test-countdown-ms','2000','--log',str(report)])
hwnd=wait_window(proc)
try:
    ready(hwnd)
    u.ShowWindow(hwnd,0)
    time.sleep(.1)
    hidden=snapshot(hwnd,report)
    assert hidden['suspended'] and not hidden['rendererAlive'] and hidden['livePages']==0, hidden
    frames=hidden['frames']
    time.sleep(2.4)
    completed=snapshot(hwnd,report)
    assert completed['frames']==frames and completed['pendingCompletion'],completed
    assert not completed['timerRunning'] and completed['livePages']==0,completed
    u.ShowWindow(hwnd,4);time.sleep(.3)
    restored=snapshot(hwnd,report)
    assert restored['rendererAlive'] and restored['livePages']==1 and not restored['pendingCompletion'],restored
    u.ShowWindow(hwnd,6);time.sleep(.1)
    minimized=snapshot(hwnd,report)
    assert minimized['suspended'] and not minimized['rendererAlive'],minimized
    u.ShowWindow(hwnd,9);time.sleep(.3)
    final=snapshot(hwnd,report)
    assert not final['suspended'] and final['rendererAlive'],final
finally:close(proc,hwnd)
(OUT/'display-lifecycle-results.json').write_text(json.dumps({'layouts':results,'dpiTransitions':transitions,'hidden':hidden,'completed':completed,'restored':restored,'minimized':minimized,'final':final},indent=2))
print('32 synthetic DPI/layout cases and hide/countdown/minimize/restore checks passed')
