"""Read-only native audio probe; never changes volume, mute or output device.

Samples visible/hidden for 60 seconds each and checks 12 page release cycles.
Reports counts only, without device names or identifiers.
"""
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import subprocess
import time
import psutil
from PIL import ImageGrab
from interaction import ROOT, OUT, wait_window, close, u

u.ShowWindow.argtypes = [w.HWND, c.c_int]
exe = ROOT / 'target/release/isle-native.exe'
report = OUT / 'live-audio-snapshot.json'
proc = subprocess.Popen([str(exe), '--live-audio', '--page', 'volume',
                         '--paused', '--reduced-motion', '--benchmark', '--log', str(report)])
hwnd = wait_window(proc)
tracked = psutil.Process(proc.pid)

def snap():
    before = report.stat().st_mtime_ns if report.exists() else 0
    u.PostMessageW(hwnd, 0x803c, 0, 0)
    for _ in range(150):
        if report.exists() and report.stat().st_mtime_ns != before:
            try:
                state = json.loads(report.read_text())
                assert state['audioWrites'] == 0, 'Read-only probe must never write audio state'
                return state
            except json.JSONDecodeError:
                pass
        time.sleep(.02)
    raise AssertionError('diagnostic timeout')

def expect(active):
    for _ in range(50):
        state = snap()
        if state['audioEndpointAlive'] == active:
            assert not state['audioError'], state
            return state
        time.sleep(.1)
    raise AssertionError(state)

def key(code):
    u.PostMessageW(hwnd, 0x100, code, 0)

def sample(seconds):
    rows = []
    last = time.monotonic()
    cpu = tracked.cpu_times()
    previous = cpu.user + cpu.system
    for _ in range(seconds):
        time.sleep(1)
        now = time.monotonic()
        cpu = tracked.cpu_times()
        total = cpu.user + cpu.system
        memory = tracked.memory_info()
        rows.append({'cpuMachinePercent': 100*(total-previous)/(now-last)/psutil.cpu_count(),
                     'privateMiB': memory.private/1024**2, 'workingSetMiB': memory.rss/1024**2,
                     'handles': tracked.num_handles()})
        last, previous = now, total
    return rows

try:
    time.sleep(5)
    initial = expect(True)
    assert initial['audioDevices'] > 0, initial
    for _ in range(11):
        key(9)  # Seven tools, Back, volume slider, mute, device menu.
    key(13)
    time.sleep(.3)
    menu = snap()
    assert menu['audioDeviceMenu'], menu
    bounds = w.RECT()
    u.GetWindowRect(hwnd, c.byref(bounds))
    ImageGrab.grab((bounds.left, bounds.top, bounds.right, bounds.bottom)).save(OUT/'audio-devices.png')
    key(27)
    time.sleep(.2)
    assert not snap()['audioDeviceMenu']
    ImageGrab.grab((bounds.left, bounds.top, bounds.right, bounds.bottom)).save(OUT/'audio-volume.png')
    visible = sample(60)
    visible_end = snap()
    print('visible sample complete', flush=True)
    u.ShowWindow(hwnd, 0)
    expect(False)
    time.sleep(.1)
    hidden_start = snap()
    hidden = sample(60)
    hidden_end = snap()
    assert hidden_end['audioPolls'] == hidden_start['audioPolls']
    assert hidden_end['frames'] == hidden_start['frames']
    assert not hidden_end['rendererAlive'] and hidden_end['livePages'] == 0
    assert hidden_end['timerIntervalMs'] == 0
    print('hidden sample complete', flush=True)
    u.ShowWindow(hwnd, 4)
    time.sleep(.2)
    key(13)  # Reopen to music.
    time.sleep(.2)
    cycles = []
    for _ in range(12):
        key(0x27); key(0x27); key(13)
        detail = expect(True)
        assert detail['livePages'] == 1
        key(27)
        music = expect(False)
        time.sleep(.15)
        assert snap()['audioPolls'] == music['audioPolls']
        cycles.append({'privateMiB': tracked.memory_info().private/1024**2,
                       'handles': tracked.num_handles(), 'audioPolls': music['audioPolls']})
    result = {'binarySha256': hashlib.sha256(exe.read_bytes()).hexdigest(),
              'initial': initial, 'menu': menu, 'visibleEnd': visible_end,
              'hiddenStart': hidden_start, 'hiddenEnd': hidden_end,
              'visibleSamples': visible, 'hiddenSamples': hidden, 'cycles': cycles,
              'children': [p.name() for p in tracked.children(recursive=True)],
              'final': snap(), 'audioCommandsSent': 0}
    (OUT/'live-audio-results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    for name, rows in [('visible', visible), ('hidden', hidden)]:
        print(name, {key: sum(r[key] for r in rows)/len(rows) for key in rows[0]}, flush=True)
    print('PASS: device navigation, endpoint release, no hidden polling/rendering, 12 cycles', flush=True)
finally:
    close(proc, hwnd)
