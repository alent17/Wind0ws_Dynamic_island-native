"""Capture two DPI/work-area cases against the separate neutral probe window."""
import ctypes as c
from ctypes import wintypes as w
import subprocess
import sys
import time
from interaction import (ROOT, OUT, wait_window, close, u, test_monitor_rect,
                         wait_window_on_monitor, save_test_screenshot)

background=subprocess.Popen([sys.executable,str(ROOT/'scripts/interaction.py'),'--probe'])
monitor=test_monitor_rect()
behind=wait_window(background,'IsleCrossProcessProbe')
try:
    for name,options in [
        ('music-150pct',['--page','music','--test-dpi','144','--attached']),
        ('weather-small-work-area',['--page','weather','--test-dpi','192','--test-work-area','320x240','--edge','right','--attached']),
    ]:
        proc=subprocess.Popen([str(ROOT/'target/release/isle-native.exe'),'--paused','--reduced-motion']+options)
        hwnd=wait_window(proc)
        try:
            r=wait_window_on_monitor(hwnd,monitor,name)
            time.sleep(.8)
            u.SetWindowPos(behind,w.HWND(-1),r.left,r.top,r.right-r.left,r.bottom-r.top,0x10)
            u.SetWindowPos(hwnd,w.HWND(-1),0,0,0,0,0x13)
            time.sleep(.15)
            save_test_screenshot(OUT/(name+'.png'),r)
        finally:close(proc,hwnd)
finally:close(background,behind)
print('DPI and small-work-area screenshots captured')
