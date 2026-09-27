"""Read-only live-session discovery, selection persistence and dialog lifetime checks.
Uses offline fixture IDs; never sends media control, volume or device commands.
"""
import ctypes as c
from ctypes import wintypes as w
import hashlib,json,os,subprocess,time
import psutil
from PIL import ImageGrab
from interaction import ROOT,OUT,wait_window,close,u,ENUM
u.GetDlgItem.argtypes=[w.HWND,c.c_int];u.GetDlgItem.restype=w.HWND
u.GetClassNameW.argtypes=[w.HWND,w.LPWSTR,c.c_int]
u.IsWindowEnabled.argtypes=[w.HWND]
exe=ROOT/'target/release/isle-native.exe';config=OUT/'native-players-test.json';report=OUT/'native-players-snapshot.json'
a,b='IsleTest.OfflineA','IsleTest.OfflineB'
config.write_text(json.dumps({'selectedPlayerIds':[],'playerOrderIds':[a,b],'futureTest':{'keep':True}}),encoding='utf-8')
def launch():
    proc=subprocess.Popen([str(exe),'--live-media','--page','music','--reduced-motion','--benchmark','--settings-path',str(config),'--log',str(report)])
    return proc,wait_window(proc)
proc,hwnd=launch();placements=[]
def snapshot():
    before=report.stat().st_mtime_ns if report.exists() else 0;u.PostMessageW(hwnd,0x803c,0,0)
    for _ in range(100):
        if report.exists() and report.stat().st_mtime_ns!=before:
            try:return json.loads(report.read_text())
            except json.JSONDecodeError:pass
        time.sleep(.02)
    raise AssertionError('snapshot timeout')
def expect(predicate):
    for _ in range(150):
        state=snapshot()
        if predicate(state):return state
        time.sleep(.03)
    raise AssertionError(state)
def window_class(name):
    found=[]
    @ENUM
    def cb(window,_):
        pid=w.DWORD();u.GetWindowThreadProcessId(window,c.byref(pid));text=c.create_unicode_buffer(128);u.GetClassNameW(window,text,128)
        if pid.value==proc.pid and text.value==name:found.append(window)
        return True
    u.EnumWindows(cb,0);assert len(found)==1;return found[0]
def bounds(window):
    rect=w.RECT();u.GetWindowRect(window,c.byref(rect));value=[rect.left,rect.top,rect.right,rect.bottom]
    if os.environ.get('ISLE_TEST_MONITOR')==r'\\.\DISPLAY2':assert -1920<=rect.left<rect.right<=0 and 0<=rect.top<rect.bottom<=1032,value
    placements.append(value);return value
def click(window,id):
    control=u.GetDlgItem(window,id);assert u.IsWindowEnabled(control)
    u.SendMessageW(control,0xf5,0,0)
def open_players(settings):
    click(settings,107);state=expect(lambda s:s['playerDialogAlive'] and s['playerListReady'])
    window=window_class('IsleNativePlayers');bounds(window);assert state['playerListRows']>=2;return window
def select(window,index):
    u.SendMessageW(u.GetDlgItem(window,302),0x186,index,0)
    u.PostMessageW(window,0x111,302|(1<<16),u.GetDlgItem(window,302));time.sleep(.1)
def save(window,predicate):
    click(window,307)
    for _ in range(100):
        data=json.loads(config.read_text(encoding='utf-8'));state=snapshot()
        if predicate(data) and not state['configurationSaving'] and state['playerListReady']:
            assert data['futureTest']=={'keep':True};return state
        time.sleep(.03)
    raise AssertionError('selection save timeout')
try:
    initial=expect(lambda s:s['mediaPolls']>0);bounds(hwnd)
    assert initial['mediaSession']==0 and initial['playerAllowedCount']==0 and not initial['playerSelectionAutomatic']
    u.PostMessageW(hwnd,0x100,0x77,0);expect(lambda s:s['settingsWindowAlive']);settings=window_class('IsleNativeWeatherSettings');bounds(settings)
    window=open_players(settings);discovered=snapshot()
    click(window,303);selected=save(window,lambda data:data['selectedPlayerIds']==[a]);assert selected['mediaSession']==0
    select(window,1);click(window,304);reordered=save(window,lambda data:data['playerOrderIds'][:2]==[b,a])
    ImageGrab.grab(bounds(window),all_screens=True).save(OUT/'native-players.png')
    select(window,1);click(window,303);empty=save(window,lambda data:data['selectedPlayerIds']==[])
    click(window,301);automatic=save(window,lambda data:data['selectedPlayerIds'] is None)
    if discovered['playerListRows']>2:
        automatic=expect(lambda s:s['mediaSession']>0)
    click(window,301);save(window,lambda data:data['selectedPlayerIds']==[])
    off=expect(lambda s:s['mediaSession']==0 and not s['artworkBusy'] and s['mediaPolls']>initial['mediaPolls'])
    cycles=[];tracked=psutil.Process(proc.pid)
    for _ in range(12):
        u.PostMessageW(window,0x10,0,0);expect(lambda s:not s['playerDialogAlive'])
        window=open_players(settings)
        cycles.append({'privateMiB':tracked.memory_info().private/1024**2,'handles':tracked.num_handles()})
    # Closing the parent destroys its player dialog and pending discovery too.
    click(window,306)
    u.PostMessageW(settings,0x10,0,0);closed=expect(lambda s:not s['settingsWindowAlive'] and not s['playerDialogAlive'])
    close(proc,hwnd);proc,hwnd=launch();restarted=expect(lambda s:s['mediaPolls']>0);bounds(hwnd)
    assert restarted['mediaSession']==0 and not restarted['playerSelectionAutomatic'] and restarted['playerAllowedCount']==0
    data=json.loads(config.read_text(encoding='utf-8'));assert data['playerOrderIds'][:2]==[b,a]
    result={'binarySha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'initial':initial,'discovered':discovered,'selected':selected,'reordered':reordered,'empty':empty,'automatic':automatic,'off':off,'closed':closed,'restarted':restarted,'cycles':cycles,'windowBounds':placements}
    (OUT/'native-players-results.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
    print('PASS: live read-only discovery, manual/all/none, offline IDs, ordering, restart and 12 window cycles')
finally:close(proc,hwnd)
