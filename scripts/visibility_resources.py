"""One exploratory visibility run: 10s visible, 60s hidden, 10s restored."""
import json
import os
import subprocess
import time
from pathlib import Path
import ctypes as c
from ctypes import wintypes as w
from interaction import ROOT, OUT, wait_window, close, u, ProcessMonitor
u.ShowWindow.argtypes=[w.HWND,c.c_int]
report=OUT/'visibility-snapshot.json'
exe=Path(os.environ.get('ISLE_TEST_EXE', ROOT/'target/release/isle-native.exe')).resolve()
proc=subprocess.Popen([str(exe),'--page','music','--long-title','--benchmark','--log',str(report)])
hwnd=wait_window(proc)
tracked=ProcessMonitor(proc)
rows=[]
def snap():
    prior=report.stat().st_mtime_ns if report.exists() else 0
    u.PostMessageW(hwnd,0x803c,0,0)
    for _ in range(100):
        if report.exists() and report.stat().st_mtime_ns!=prior:
            try:return json.loads(report.read_text())
            except json.JSONDecodeError:pass
        time.sleep(.02)
    raise AssertionError('snapshot timeout')
def sample(scene,seconds):
    before=snap()
    cpu=tracked.cpu_times();last_cpu=cpu.user+cpu.system;last=time.monotonic()
    for _ in range(seconds):
        time.sleep(1)
        now=time.monotonic();cpu=tracked.cpu_times();total=cpu.user+cpu.system
        mem=tracked.memory_info()
        rows.append({'scene':scene,'cpuMachinePercent':100*(total-last_cpu)/(now-last)/(os.cpu_count() or 1),
                     'privateMiB':mem.private/1024**2,'workingSetMiB':mem.rss/1024**2,'handles':tracked.num_handles()})
        last=now;last_cpu=total
    after=snap()
    if scene=='hidden':
        assert before['frames']==after['frames'],(before,after)
        assert not after['rendererAlive'] and after['livePages']==0 and after['timerIntervalMs']==0,after
    return {'before':before,'after':after}
try:
    time.sleep(5)
    visible=sample('visible',10)
    u.ShowWindow(hwnd,0);time.sleep(1)
    hidden=sample('hidden',60)
    u.ShowWindow(hwnd,4);time.sleep(2)
    restored=sample('restored-compact',10)
    result={'samples':rows,'visible':visible,'hidden':hidden,'restored':restored}
    (OUT/'visibility-resources.json').write_text(json.dumps(result,indent=2))
    for scene in ['visible','hidden','restored-compact']:
        records=[r for r in rows if r['scene']==scene]
        print(scene,{key:sum(r[key] for r in records)/len(records) for key in ['cpuMachinePercent','privateMiB','workingSetMiB']})
finally:close(proc,hwnd)
