"""Native controls/settings integration, isolated config and no hardware input."""
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from PIL import ImageGrab
from interaction import ROOT, OUT, wait_window, close, u, ENUM

u.GetDlgItem.argtypes=[w.HWND,c.c_int];u.GetDlgItem.restype=w.HWND
u.GetClassNameW.argtypes=[w.HWND,w.LPWSTR,c.c_int]
u.GetWindowTextW.argtypes=[w.HWND,w.LPWSTR,c.c_int]
exe=Path(os.environ.get('ISLE_TEST_EXE', ROOT/'target/release/isle-native.exe')).resolve()

def window_dpi_context(window):
    """Read the owned window's current DPI-awareness context when supported."""
    try:
        get_context=u.GetWindowDpiAwarenessContext
        get_context.argtypes=[w.HWND];get_context.restype=c.c_void_p
        equal=u.AreDpiAwarenessContextsEqual
        equal.argtypes=[c.c_void_p,c.c_void_p];equal.restype=w.BOOL
        context=get_context(window)
        if not context:return {'available':True,'context':None,'perMonitorV2':False}
        return {'available':True,'context':hex(context),'perMonitorV2':bool(equal(context,c.c_void_p(-4)))}
    except (AttributeError,OSError):
        return {'available':False,'reason':'DPI-awareness context query API unavailable'}

def loaded_comctl32_path(pid):
    """Read the module path in the owned process; do not inject or modify it."""
    process=None
    try:
        kernel32=c.WinDLL('kernel32',use_last_error=True)
        psapi=c.WinDLL('psapi',use_last_error=True)
        kernel32.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD]
        kernel32.OpenProcess.restype=w.HANDLE
        kernel32.CloseHandle.argtypes=[w.HANDLE]
        kernel32.CloseHandle.restype=w.BOOL
        process=kernel32.OpenProcess(0x0410,False,pid)  # QUERY_INFORMATION | VM_READ
        if not process:
            return {'available':False,'reason':f'OpenProcess failed: {c.get_last_error()}'}
        modules=(c.c_void_p*1024)()
        needed=w.DWORD()
        enum=psapi.EnumProcessModulesEx
        enum.argtypes=[w.HANDLE,c.POINTER(c.c_void_p),w.DWORD,c.POINTER(w.DWORD),w.DWORD]
        enum.restype=w.BOOL
        if not enum(process,modules,c.sizeof(modules),c.byref(needed),0x03):
            return {'available':False,'reason':f'EnumProcessModulesEx failed: {c.get_last_error()}'}
        get_name=psapi.GetModuleBaseNameW
        get_name.argtypes=[w.HANDLE,c.c_void_p,w.LPWSTR,w.DWORD]
        get_name.restype=w.DWORD
        get_path=psapi.GetModuleFileNameExW
        get_path.argtypes=[w.HANDLE,c.c_void_p,w.LPWSTR,w.DWORD]
        get_path.restype=w.DWORD
        count=min(needed.value//c.sizeof(c.c_void_p),len(modules))
        for module in modules[:count]:
            name=c.create_unicode_buffer(260)
            if get_name(process,module,name,len(name)) and name.value.lower()=='comctl32.dll':
                path=c.create_unicode_buffer(32768)
                if get_path(process,module,path,len(path)):
                    normalized=path.value.lower().replace('/','\\')
                    return {'available':True,'path':path.value,
                            'sideBySideV6Path':'\\winsxs\\' in normalized and 'common-controls_' in normalized}
        return {'available':True,'path':None,'sideBySideV6Path':False}
    except (AttributeError,OSError) as exc:
        return {'available':False,'reason':str(exc)}
    finally:
        if process:kernel32.CloseHandle(process)

config=OUT/'native-settings-test.json'
config.write_text('{"futureTest":{"keep":[1,null,3]},"autoStart":true,"alwaysOnTop":true,"floatingFillColor":"#102030","floatingUseAlbumColor":true}',encoding='utf-8')
report=OUT/'native-settings-snapshot.json'
placements=[]
runtimeProbe={'settingsDpiContext':None}
ownedExits=[]
def launch():
    proc=subprocess.Popen([str(exe),'--page','music','--paused','--benchmark','--settings-path',str(config),'--log',str(report)])
    return proc,wait_window(proc)

def close_owned(proc,window):
    if proc.poll() is not None:raise AssertionError(f'owned process {proc.pid} exited before graceful close: {proc.returncode}')
    pid=proc.pid
    close(proc,window)
    code=proc.poll()
    if code is None:raise AssertionError(f'owned process {pid} remained alive after WM_CLOSE')
    ownedExits.append({'pid':pid,'returncode':code,'forcedKill':False,'gracefulClose':True})
    if code!=0:raise AssertionError(f'owned process {pid} exited with {code}')
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
    u.EnumWindows(cb,0);assert len(found)==1;check_bounds(found[0])
    if runtimeProbe['settingsDpiContext'] is None:runtimeProbe['settingsDpiContext']=window_dpi_context(found[0])
    return found[0]
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
    runtimeProbe['hostDpiContext']=window_dpi_context(hwnd)
    runtimeProbe['comctl32']=loaded_comctl32_path(proc.pid)
    initial=snapshot();check_bounds(hwnd)
    assert initial['islandAlwaysOnTop'] and initial['islandTopmostStyle']
    window=open_settings()
    check(window,217,True)
    for i in range(7):check(window,201+i,i in [0,5,6])
    check(window,208,False)
    check(window,217,False)
    sparse=apply(window,[0,5,6]);assert sparse['reducedMotion']
    assert not sparse['islandAlwaysOnTop'] and not sparse['islandTopmostStyle']
    saved=json.loads(config.read_text(encoding='utf-8'));assert saved['autoStart'] and saved['futureTest']=={'keep':[1,None,3]}
    assert saved['alwaysOnTop'] is False and saved['floatingWindowAlwaysOnTop'] is True
    assert saved['showTimerTool'] and saved['showClockTool'] and not saved['showSettingsTool']
    if '--no-screenshots' not in sys.argv:
        ImageGrab.grab(check_bounds(window),all_screens=True).save(OUT/'native-settings.png')
    u.PostMessageW(window,0x10,0,0);expect(lambda s:not s['settingsWindowAlive'])
    # The settings tool itself is disabled; F8 must still reopen it.
    window=open_settings();check(window,200,False);empty=apply(window,[])
    u.PostMessageW(window,0x10,0,0);expect(lambda s:not s['settingsWindowAlive'])
    for key in [0x70,0x71,0x72,0x73]:
        u.PostMessageW(hwnd,0x100,key,0);time.sleep(.1);check_bounds(hwnd)
    window=open_settings()
    color=c.create_unicode_buffer(16);u.GetWindowTextW(u.GetDlgItem(window,215),color,16)
    assert color.value=='#102030' and u.SendMessageW(u.GetDlgItem(window,216),0xf0,0,0)==1
    check(window,200,True)
    # Closing without Apply discards the draft.
    u.PostMessageW(window,0x10,0,0);expect(lambda s:not s['settingsWindowAlive'])
    assert snapshot()['visibleTools']==[]
    window=open_settings();assert u.SendMessageW(u.GetDlgItem(window,200),0xf0,0,0)==0
    check(window,200,True);check(window,208,True);check(window,209,True);check(window,217,True)
    restored=apply(window,[0,5,6]);assert restored['reducedMotion']
    assert restored['islandAlwaysOnTop'] and restored['islandTopmostStyle']
    clock_cases=[]
    for index in [1,2,3,4,5,0,5]:
        u.SendMessageW(u.GetDlgItem(window,210),0x14e,index,0)
        u.SendMessageW(u.GetDlgItem(window,105),0xf5,0,0)
        state=expect(lambda s:not s['configurationSaving'] and s['clockZoneIndex']==index)
        assert state['clockZoneSupported']
        clock_cases.append(state)

    # Native appearance controls apply immediately and preserve along-edge placement.
    u.SendMessageW(u.GetDlgItem(window,211),0x14e,1,0)  # attached style
    u.SendMessageW(u.GetDlgItem(window,213),0x14e,1,0)  # right edge
    u.SendMessageW(u.GetDlgItem(window,214),0x14e,73,0)
    shape=[140,12,48,64]
    for index,value in enumerate(shape):
        track=u.GetDlgItem(window,220+index)
        u.SendMessageW(track,0x405,1,value)  # TBM_SETPOS
        u.SendMessageW(window,0x114,5<<16,track)  # WM_HSCROLL / thumb-track
    names=['收起长度','收起凹肩','展开凹肩','展开圆角']
    for index,value in enumerate(shape):
        label=c.create_unicode_buffer(64)
        u.GetWindowTextW(u.GetDlgItem(window,230+index),label,64)
        assert label.value==f'{names[index]}：{value} px',label.value
    check(window,216,False)
    draft=snapshot()
    assert draft.get('settingsDraftPosition')==73 and draft.get('settingsDraftShape')==shape,draft
    assert draft.get('settingsDraftFillColor')=='#102030' and not draft.get('settingsDraftAlbumColor'),draft
    if '--no-screenshots' not in sys.argv:
        ImageGrab.grab(check_bounds(window),all_screens=True).save(OUT/'native-appearance.png')
    u.SendMessageW(u.GetDlgItem(window,212),0xf5,0,0)
    placed=expect(lambda s:not s['configurationSaving'] and s['islandAttached'] and s['islandEdge']=='Right' and s['islandEdgePosition']==73 and s['compactLength']==140 and s['collapsedShoulderRadius']==12 and s['expandedShoulderRadius']==48 and s['expandedCornerRadius']==64 and s['backgroundColor']=='#102030' and not s['albumColorEnabled'])
    bounds=check_bounds(hwnd)
    if '--no-screenshots' not in sys.argv:
        ImageGrab.grab(bounds,all_screens=True).save(OUT/'native-appearance-island.png')
    assert bounds[2]==0 and bounds[1]>0
    saved=json.loads(config.read_text(encoding='utf-8'))
    assert saved['islandStyle']=='edge' and saved['islandEdge']=='right' and saved['islandEdgePosition']==73
    assert [saved['compactLength'],saved['collapsedEdgeShoulderRadius'],saved['expandedEdgeShoulderRadius'],saved['expandedCornerRadius']]==shape
    assert saved['floatingFillColor']=='#102030' and saved['floatingUseAlbumColor'] is False

    u.PostMessageW(window,0x10,0,0);expect(lambda s:not s['settingsWindowAlive'])
    for _ in range(2):u.PostMessageW(hwnd,0x100,0x27,0)
    u.PostMessageW(hwnd,0x100,13,0);time.sleep(.3)
    if '--no-screenshots' not in sys.argv:
        ImageGrab.grab(check_bounds(hwnd),all_screens=True).save(OUT/'native-clock.png')
    close_owned(proc,hwnd);proc,hwnd=launch()
    runtimeProbe['restartedHostDpiContext']=window_dpi_context(hwnd)
    runtimeProbe['restartedComctl32']=loaded_comctl32_path(proc.pid)
    restarted=expect(lambda s:s['visibleTools']==[0,5,6] and s['islandAlwaysOnTop'] and s['islandTopmostStyle'] and s['islandAttached'] and s['islandEdge']=='Right' and s['islandEdgePosition']==73 and s['compactLength']==140 and s['collapsedShoulderRadius']==12 and s['expandedShoulderRadius']==48 and s['expandedCornerRadius']==64 and s['backgroundColor']=='#102030' and not s['albumColorEnabled']);bounds=check_bounds(hwnd)
    assert bounds[2]==0 and restarted['reducedMotion'] and restarted['weatherRequests']==0
    assert restarted['clockZoneIndex']==5 and json.loads(config.read_text(encoding='utf-8'))['clockTimeZone']=='UTC'
    window=open_settings()
    color=c.create_unicode_buffer(16);u.GetWindowTextW(u.GetDlgItem(window,215),color,16)
    assert color.value=='#102030' and u.SendMessageW(u.GetDlgItem(window,216),0xf0,0,0)==0
    assert [u.SendMessageW(u.GetDlgItem(window,220+i),0x400,0,0) for i in range(4)]==[140,12,48,64]
    assert u.SendMessageW(u.GetDlgItem(window,211),0x147,0,0)==1
    assert u.SendMessageW(u.GetDlgItem(window,213),0x147,0,0)==1
    assert u.SendMessageW(u.GetDlgItem(window,214),0x147,0,0)==73
    u.PostMessageW(window,0x10,0,0);expect(lambda s:not s['settingsWindowAlive'])
    close_owned(proc,hwnd)
    result={'binarySha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'runtimeActivationEvidence':runtimeProbe,'ownedProcesses':ownedExits,'initial':initial,'sparse':sparse,'empty':empty,'restored':restored,'appearanceApplied':placed,'restarted':restarted,'windowBounds':placements,'clockCases':clock_cases}
    (OUT/'native-settings-results.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
    print('PASS: topmost, tools, animation, timezone, edge placement, shape and color settings, restart restoration and secondary-screen bounds')
finally:
    if proc.poll() is None:close_owned(proc,hwnd)
