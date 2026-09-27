"""Native controls/settings integration, isolated config and no hardware input."""
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import os
import subprocess
import time
from PIL import ImageGrab
from interaction import ROOT, OUT, wait_window, close, u, ENUM

u.GetDlgItem.argtypes=[w.HWND,c.c_int];u.GetDlgItem.restype=w.HWND
u.GetClassNameW.argtypes=[w.HWND,w.LPWSTR,c.c_int]
exe=ROOT/'target/release/isle-native.exe'
config=OUT/'native-settings-test.json'
config.write_text('{"futureTest":{"keep":[1,null,3]},"autoStart":true}',encoding='utf-8')
report=OUT/'native-settings-snapshot.json'
placements=[]
def launch():
    proc=subprocess.Popen([str(exe),'--page','music','--paused','--benchmark','--settings-path',str(config),'--log',str(report)])
    return proc,wait_window(proc)
proc,hwnd=launch()
def snapshot():
    before=report.stat().st_mtime_ns if report.exists() else 0
    u.PostMessageW(hwnd,0x803c,0,0)
    for _ in range(100):
        if report.exists() and report.stat().st_mtime_ns!=before:
            try:return json.loads(report.read_text())
            except json.JSONDecodeError:pass
        time.sleep(.02)
    raise AssertionError('snapshot timeout')
def expect(predicate):
    for _ in range(100):
        state=snapshot()
        if predicate(state):return state
        time.sleep(.02)
    raise AssertionError(state)
def check_bounds(window):
    rect=w.RECT();u.GetWindowRect(window,c.byref(rect));bounds=[rect.left,rect.top,rect.right,rect.bottom]
    if os.environ.get('ISLE_TEST_MONITOR')==r'\\.\DISPLAY2':
        assert -1920<=rect.left<rect.right<=0 and 0<=rect.top<rect.bottom<=1032,bounds
    placements.append(bounds);return bounds
def open_settings():
    u.PostMessageW(hwnd,0x100,0x77,0)
    expect(lambda s:s['settingsWindowAlive'])
    found=[]
    @ENUM
    def cb(window,_):
        pid=w.DWORD();u.GetWindowThreadProcessId(window,c.byref(pid))
        name=c.create_unicode_buffer(128);u.GetClassNameW(window,name,128)
        if pid.value==proc.pid and name.value=='IsleNativeWeatherSettings':found.append(window)
        return True
    u.EnumWindows(cb,0);assert len(found)==1;check_bounds(found[0]);return found[0]
def check(window,id,value):
    control=u.GetDlgItem(window,id)
    actual=u.SendMessageW(control,0xf0,0,0)==1
    if actual!=value:u.SendMessageW(control,0xf5,0,0)
    assert (u.SendMessageW(control,0xf0,0,0)==1)==value
def apply(window,tools):
    u.SendMessageW(u.GetDlgItem(window,105),0xf5,0,0)
    state=expect(lambda s:not s['configurationSaving'] and s['visibleTools']==tools)
    # A fast save may complete before the first snapshot; confirm persisted values too.
    assert state['configurationValid'];return state
try:
    initial=snapshot();check_bounds(hwnd)
    window=open_settings()
    for i in range(7):check(window,201+i,i in [0,5,6])
    check(window,208,False)
    sparse=apply(window,[0,5,6]);assert sparse['reducedMotion']
    saved=json.loads(config.read_text(encoding='utf-8'));assert saved['autoStart'] and saved['futureTest']=={'keep':[1,None,3]}
    assert saved['showTimerTool'] and saved['showClockTool'] and not saved['showSettingsTool']
    ImageGrab.grab(check_bounds(window),all_screens=True).save(OUT/'native-settings.png')
    u.PostMessageW(window,0x10,0,0);expect(lambda s:not s['settingsWindowAlive'])
    # The settings tool itself is disabled; F8 must still reopen it.
    window=open_settings();check(window,200,False);empty=apply(window,[])
    u.PostMessageW(window,0x10,0,0);expect(lambda s:not s['settingsWindowAlive'])
    for key in [0x70,0x71,0x72,0x73]:
        u.PostMessageW(hwnd,0x100,key,0);time.sleep(.1);check_bounds(hwnd)
    window=open_settings()
    check(window,200,True)
    # Closing without Apply discards the draft.
    u.PostMessageW(window,0x10,0,0);expect(lambda s:not s['settingsWindowAlive'])
    assert snapshot()['visibleTools']==[]
    window=open_settings();assert u.SendMessageW(u.GetDlgItem(window,200),0xf0,0,0)==0
    check(window,200,True);check(window,208,True);check(window,209,True)
    restored=apply(window,[0,5,6]);assert restored['reducedMotion']
    clock_cases=[]
    for index in [1,2,3,4,5,0,5]:
        u.SendMessageW(u.GetDlgItem(window,210),0x14e,index,0)
        u.SendMessageW(u.GetDlgItem(window,105),0xf5,0,0)
        state=expect(lambda s:not s['configurationSaving'] and s['clockZoneIndex']==index)
        assert state['clockZoneSupported']
        clock_cases.append(state)

    u.PostMessageW(window,0x10,0,0);expect(lambda s:not s['settingsWindowAlive'])
    for _ in range(2):u.PostMessageW(hwnd,0x100,0x27,0)
    u.PostMessageW(hwnd,0x100,13,0);time.sleep(.3)
    ImageGrab.grab(check_bounds(hwnd),all_screens=True).save(OUT/'native-clock.png')
    close(proc,hwnd);proc,hwnd=launch()
    restarted=expect(lambda s:s['visibleTools']==[0,5,6]);check_bounds(hwnd)
    assert restarted['reducedMotion'] and restarted['weatherRequests']==0
    assert restarted['clockZoneIndex']==5 and json.loads(config.read_text(encoding='utf-8'))['clockTimeZone']=='UTC'
    result={'binarySha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'initial':initial,'sparse':sparse,'empty':empty,'restored':restored,'restarted':restarted,'windowBounds':placements,'clockCases':clock_cases}
    (OUT/'native-settings-results.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
    print('PASS: sparse tools, all-off recovery, F8, discard, animation settings, restart, all six timezones and secondary-screen bounds')
finally:close(proc,hwnd)
