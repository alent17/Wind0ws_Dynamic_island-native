"""Native weather/settings integration. Only writes its own artifact configuration."""
import ctypes as c
from ctypes import wintypes as w
import hashlib
import os
import json
import subprocess
import time
import sys
import psutil
from PIL import ImageGrab
from interaction import ROOT, OUT, wait_window, close, u, ENUM

u.GetDlgItem.argtypes=[w.HWND,c.c_int];u.GetDlgItem.restype=w.HWND
u.GetClassNameW.argtypes=[w.HWND,w.LPWSTR,c.c_int]
u.GetWindowTextW.argtypes=[w.HWND,w.LPWSTR,c.c_int]
u.ShowWindow.argtypes=[w.HWND,c.c_int]
exe=ROOT/'target/release/isle-native.exe'
config=OUT/'weather-test-settings.json'
fixture={'playerOrderIds':['second','first'],'autoStart':True,'futureNativeTest':{'nested':[None,{'value':42}]}}
original=json.dumps(fixture).encode('utf-8')
config.write_bytes(original)
report=OUT/'weather-snapshot.json'
proc=subprocess.Popen([str(exe),'--page','weather','--paused','--reduced-motion','--benchmark',
                       '--settings-path',str(config),'--log',str(report)])
hwnd=wait_window(proc)
tracked=psutil.Process(proc.pid)
seconds=5 if '--quick' in sys.argv else 60
placements=[]

def check_monitor(window):
    rect=w.RECT();u.GetWindowRect(window,c.byref(rect))
    bounds=[rect.left,rect.top,rect.right,rect.bottom]
    if os.environ.get('ISLE_TEST_MONITOR') == r'\\.\DISPLAY2':
        assert -1920 <= rect.left < rect.right <= 0, bounds
        assert 0 <= rect.top < rect.bottom <= 1032, bounds
    placements.append(bounds)

def snap():
    before=report.stat().st_mtime_ns if report.exists() else 0
    u.PostMessageW(hwnd,0x803c,0,0)
    for _ in range(100):
        if report.exists() and report.stat().st_mtime_ns!=before:
            try:return json.loads(report.read_text())
            except json.JSONDecodeError:pass
        time.sleep(.02)
    raise AssertionError('diagnostic timeout')

def expect(predicate,seconds=25):
    end=time.monotonic()+seconds
    while time.monotonic()<end:
        state=snap()
        if predicate(state):return state
        time.sleep(.1)
    raise AssertionError(state)

def key(code):u.PostMessageW(hwnd,0x100,code,0)

def settings_window():
    found=[]
    @ENUM
    def cb(window,_):
        owner=w.DWORD();u.GetWindowThreadProcessId(window,c.byref(owner))
        text=c.create_unicode_buffer(128);u.GetClassNameW(window,text,128)
        if owner.value==proc.pid and text.value=='IsleNativeWeatherSettings':found.append(window)
        return True
    u.EnumWindows(cb,0)
    return found[0] if found else None

def open_settings():
    for _ in range(9):key(9)
    key(13)
    expect(lambda s:s['settingsWindowAlive'])
    window=settings_window();assert window
    check_monitor(window)
    return window

def screenshot(window,name):
    rect=w.RECT();u.GetWindowRect(window,c.byref(rect))
    ImageGrab.grab((rect.left,rect.top,rect.right,rect.bottom),all_screens=True).save(OUT/name)

def sample():
    rows=[];last=time.monotonic();cpu=tracked.cpu_times();previous=cpu.user+cpu.system
    for _ in range(seconds):
        time.sleep(1);now=time.monotonic();cpu=tracked.cpu_times();total=cpu.user+cpu.system;mem=tracked.memory_info()
        rows.append({'cpuMachinePercent':100*(total-previous)/(now-last)/psutil.cpu_count(),
                     'privateMiB':mem.private/1024**2,'workingSetMiB':mem.rss/1024**2,'handles':tracked.num_handles()})
        last,previous=now,total
    return rows

try:
    check_monitor(hwnd)
    initial=snap();assert not initial['weatherConfigured'] and initial['weatherRequests']==0
    screenshot(hwnd,'weather-empty.png')
    window=open_settings()
    query=c.create_unicode_buffer('上海')
    u.SendMessageW(u.GetDlgItem(window,101),0xc,0,c.cast(query,c.c_void_p).value)
    u.SendMessageW(u.GetDlgItem(window,102),0xf5,0,0)
    end=time.monotonic()+25
    while time.monotonic()<end:
        count=u.SendMessageW(u.GetDlgItem(window,103),0x18b,0,0)
        if count>0:break
        time.sleep(.1)
    else:
        text=c.create_unicode_buffer(256);u.GetWindowTextW(u.GetDlgItem(window,106),text,256)
        raise AssertionError(text.value)
    screenshot(window,'weather-settings.png')
    u.SendMessageW(u.GetDlgItem(window,104),0xf5,0,0)
    ready=expect(lambda s:s['weatherData'] and not s['weatherBusy'] and not s['settingsWindowAlive'])
    assert ready['weatherDays']==3 and not ready['weatherError']
    saved=json.loads(config.read_text(encoding='utf-8'))
    assert saved['weatherLocation']['name']
    assert all(saved[k]==v for k,v in fixture.items())
    assert config.with_suffix('.previous.json').read_bytes()==original
    screenshot(hwnd,'weather-live.png')
    time.sleep(5)
    visible=sample();visible_end=snap();print('visible sample complete',flush=True)
    u.ShowWindow(hwnd,0)
    hidden_start=expect(lambda s:not s['weatherBusy'] and s['livePages']==0)
    hidden=sample();hidden_end=snap()
    assert hidden_end['weatherRequests']==hidden_start['weatherRequests']
    assert hidden_end['frames']==hidden_start['frames'] and not hidden_end['rendererAlive']
    print('hidden sample complete',flush=True)
    u.ShowWindow(hwnd,4);time.sleep(.2);key(13);time.sleep(.2)
    cycles=[]
    for _ in range(12):
        for _ in range(7):key(0x27)
        key(13)
        state=expect(lambda s:s['weatherData'] and not s['weatherBusy'])
        window=open_settings();u.PostMessageW(window,0x10,0,0)
        expect(lambda s:not s['settingsWindowAlive'] and not s['weatherBusy'])
        assert settings_window() is None
        assert snap()['weatherRequests']==visible_end['weatherRequests']
        key(27);time.sleep(.1)
        cycles.append({'privateMiB':tracked.memory_info().private/1024**2,'handles':tracked.num_handles()})
    result={'binarySha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'sampleSeconds':seconds,
            'initial':initial,'ready':ready,'visibleEnd':visible_end,'visibleSamples':visible,
            'hiddenStart':hidden_start,'hiddenEnd':hidden_end,'hiddenSamples':hidden,
            'cycles':cycles,'windowBounds':placements,'final':snap(),'children':[p.name() for p in tracked.children(recursive=True)]}
    close(proc,hwnd)
    before_restart=config.read_bytes()
    proc=subprocess.Popen([str(exe),'--page','music','--paused','--reduced-motion','--benchmark',
                           '--settings-path',str(config),'--log',str(report)])
    hwnd=wait_window(proc)
    check_monitor(hwnd)
    result['restarted']=expect(lambda s:s['weatherConfigured'] and s['configurationValid'])
    assert result['restarted']['weatherRequests']==0 and config.read_bytes()==before_restart
    (OUT/('live-weather-quick.json' if seconds==5 else 'live-weather-results.json')).write_text(json.dumps(result,indent=2),encoding='utf-8')
    for label,rows in [('visible',visible),('hidden',hidden)]:print(label,{k:sum(r[k] for r in rows)/len(rows) for k in rows[0]},flush=True)
    print('PASS: Chinese search, preserving save, backup, restart, forecast, cache, hide and 12 settings-window cycles',flush=True)
finally:close(proc,hwnd)
