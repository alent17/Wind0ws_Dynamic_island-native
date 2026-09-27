"""Read-only GSMTC/lifecycle probe. Never sends media playback commands.

60-second visible and hidden samples, then 12 page release/reacquire cycles.
Song names and source application identities are not written to the report.
"""
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import subprocess
import time
import psutil
from interaction import ROOT, OUT, wait_window, close, u

u.ShowWindow.argtypes = [w.HWND, c.c_int]
exe = ROOT / 'target/release/isle-native.exe'
report = OUT / 'live-media-snapshot.json'
proc = subprocess.Popen([str(exe), '--live-media', '--page', 'music', '--reduced-motion', '--benchmark', '--log', str(report)])
hwnd = wait_window(proc)
tracked = psutil.Process(proc.pid)

def snap():
    before = report.stat().st_mtime_ns if report.exists() else 0
    u.PostMessageW(hwnd, 0x803c, 0, 0)
    for _ in range(150):
        if report.exists() and report.stat().st_mtime_ns != before:
            try:
                return json.loads(report.read_text())
            except json.JSONDecodeError:
                pass
        time.sleep(.02)
    raise AssertionError('diagnostic timeout')

def expect(active):
    for _ in range(50):
        state = snap()
        if state['mediaActive'] == active and state['mediaManagerAlive'] == active:
            assert not state['mediaError'], state
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
        rows.append({'cpuMachinePercent': 100 * (total-previous)/(now-last)/psutil.cpu_count(),
                     'privateMiB': memory.private/1024**2, 'workingSetMiB': memory.rss/1024**2,
                     'handles': tracked.num_handles()})
        last, previous = now, total
    return rows

try:
    time.sleep(5)
    initial = expect(True)
    visible = sample(60)
    visible_end = snap()
    print('visible complete', visible_end, flush=True)
    u.ShowWindow(hwnd, 0)
    hidden_start = expect(False)
    hidden = sample(60)
    hidden_end = snap()
    assert hidden_end['mediaPolls'] == hidden_start['mediaPolls']
    assert hidden_end['frames'] == hidden_start['frames']
    assert not hidden_end['rendererAlive'] and hidden_end['livePages'] == 0
    assert hidden_end['timerIntervalMs'] == 0
    u.ShowWindow(hwnd, 4)
    expect(True)
    key(0x0d)  # Expand compact island; no media control.
    time.sleep(.2)
    cycles = []
    for _ in range(12):
        key(0x27); key(0x27); key(0x0d)  # Focus and open volume tool.
        detail = expect(False)
        time.sleep(.15)
        assert snap()['mediaPolls'] == detail['mediaPolls']
        key(0x1b)
        music = expect(True)
        cycles.append({'detailPolls': detail['mediaPolls'], 'musicPolls': music['mediaPolls'],
                       'privateMiB': tracked.memory_info().private/1024**2, 'handles': tracked.num_handles()})
    result = {'binarySha256': hashlib.sha256(exe.read_bytes()).hexdigest(),
              'initial': initial, 'visibleEnd': visible_end, 'hiddenStart': hidden_start,
              'hiddenEnd': hidden_end, 'visibleSamples': visible, 'hiddenSamples': hidden,
              'cycles': cycles, 'children': [p.name() for p in tracked.children(recursive=True)],
              'playbackCommandsSent': 0}
    (OUT / 'live-media-results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    for name, rows in [('visible', visible), ('hidden', hidden)]:
        print(name, {key: sum(r[key] for r in rows)/len(rows) for key in rows[0]}, flush=True)
    print('PASS: media session release/reacquire, no hidden polling or rendering, 12 cycles', flush=True)
finally:
    close(proc, hwnd)
