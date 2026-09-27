"""Startup protection checks against isolated fixtures; never uses installed settings."""
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import os
import subprocess
from interaction import ROOT, OUT, wait_window, close, u

exe=ROOT/'target/release/isle-native.exe'
folder=OUT/'configuration-faults'
folder.mkdir(exist_ok=True)
cases={'malformed':b'{broken', 'wrong-type':b'{"showClockTool":"yes"}',
       'oversized':b' '*1048577, 'valid-unused':b'{"future":{"a":[1,null,3]},"autoStart":true}'}
results=[]
for name,original in cases.items():
    path=folder/f'{name}.json';path.write_bytes(original)
    report=folder/f'{name}-snapshot.json'
    proc=subprocess.Popen([str(exe),'--page','weather','--paused','--reduced-motion','--benchmark',
                           '--settings-path',str(path),'--log',str(report),'--exit-after','1'])
    hwnd=wait_window(proc)
    try:
        rect=w.RECT();u.GetWindowRect(hwnd,c.byref(rect))
        bounds=[rect.left,rect.top,rect.right,rect.bottom]
        if os.environ.get('ISLE_TEST_MONITOR')==r'\\.\DISPLAY2':
            assert -1920<=rect.left<rect.right<=0 and 0<=rect.top<rect.bottom<=1032,bounds
        assert proc.wait(timeout=8)==0
        state=json.loads(report.read_text())
        assert state['configurationValid']==(name=='valid-unused'),state
        assert not state['configurationSaving'] and state['weatherRequests']==0,state
        assert path.read_bytes()==original
        results.append({'case':name,'unchanged':True,'windowBounds':bounds,'snapshot':state})
    finally:close(proc,hwnd)
(OUT/'configuration-fault-results.json').write_text(json.dumps({
    'binarySha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'cases':results},indent=2),encoding='utf-8')
print('PASS: malformed, wrong type, oversized and unused configuration remain unchanged')
