"""Read-only output-loopback probe. Does not play sound or control any player.

Same-binary static UI baseline, live spectrum, hidden: 60 seconds each.
Only normalized peaks/counters are reported, never audio samples or device IDs.
"""
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import subprocess
import time
import sys
import psutil
from PIL import ImageGrab
from interaction import ROOT, OUT, wait_window, close, u

u.ShowWindow.argtypes = [w.HWND, c.c_int]
exe = ROOT/'target/release/isle-native.exe'
seconds = 5 if '--quick' in sys.argv else 60
results = {'binarySha256': hashlib.sha256(exe.read_bytes()).hexdigest(), 'sampleSeconds': seconds}

for mode in ['baseline', 'spectrum']:
    report = OUT/('spectrum-' + mode + '-snapshot.json')
    args = [str(exe), '--page', 'music', '--paused', '--benchmark', '--log', str(report)]
    if mode == 'spectrum':
        args += ['--live-spectrum']
    proc = subprocess.Popen(args)
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
        for _ in range(80):
            state = snap()
            if state['spectrumCaptureAlive'] == active and state['spectrumActive'] == active:
                assert not state['spectrumError'], state
                return state
            time.sleep(.1)
        raise AssertionError(state)

    def key(code):
        u.PostMessageW(hwnd, 0x100, code, 0)

    def sample():
        rows = []
        previous_time = time.monotonic()
        cpu = tracked.cpu_times()
        previous_cpu = cpu.user + cpu.system
        for _ in range(seconds):
            time.sleep(1)
            now = time.monotonic()
            cpu = tracked.cpu_times()
            total = cpu.user + cpu.system
            mem = tracked.memory_info()
            rows.append({'cpuMachinePercent': 100*(total-previous_cpu)/(now-previous_time)/psutil.cpu_count(),
                         'privateMiB': mem.private/1024**2, 'workingSetMiB': mem.rss/1024**2,
                         'handles': tracked.num_handles()})
            state = snap()  # Identical diagnostic I/O in baseline and capture samples.
            if mode == 'spectrum':
                rows[-1]['spectrumPeak'] = state['spectrumPeak']
            previous_time, previous_cpu = now, total
        return rows

    try:
        time.sleep(5)
        initial = expect(True) if mode == 'spectrum' else snap()
        visible = sample()
        visible_end = snap()
        results[mode] = {'initial': initial, 'visibleSamples': visible, 'visibleEnd': visible_end}
        print(mode, {key: sum(row[key] for row in visible)/len(visible) for key in visible[0]}, flush=True)
        if mode == 'baseline':
            continue
        assert visible_end['spectrumAnalyses'] > initial['spectrumAnalyses'], 'No audio packets; playback needed to validate real capture'
        bounds = w.RECT()
        u.GetWindowRect(hwnd, c.byref(bounds))
        ImageGrab.grab((bounds.left,bounds.top,bounds.right,bounds.bottom)).save(OUT/'spectrum-live.png')
        u.ShowWindow(hwnd,0)
        expect(False)
        hidden_start = snap()
        hidden = sample()
        hidden_end = snap()
        for field in ['spectrumPackets', 'spectrumAnalyses', 'frames']:
            assert hidden_start[field] == hidden_end[field], field
        assert hidden_end['livePages'] == 0 and not hidden_end['rendererAlive']
        assert hidden_end['timerIntervalMs'] == 0
        assert hidden_end['spectrumPeak'] == 0
        results['hidden'] = {'start': hidden_start, 'end': hidden_end, 'samples': hidden}
        print('hidden', {key: sum(row[key] for row in hidden)/len(hidden) for key in hidden[0]}, flush=True)
        u.ShowWindow(hwnd,4)
        expect(True)
        key(13)  # Expand compact island; no playback controls.
        time.sleep(.2)
        cycles = []
        for _ in range(12):
            key(0x27); key(0x27); key(13)
            detail = expect(False)
            time.sleep(.1)
            assert snap()['spectrumPackets'] == detail['spectrumPackets']
            key(27)
            music = expect(True)
            cycles.append({'starts': music['spectrumStarts'], 'privateMiB': tracked.memory_info().private/1024**2,
                           'handles': tracked.num_handles()})
        # Rapidly change visible/hidden state; finish hidden and ensure no stale work.
        for _ in range(10):
            u.ShowWindow(hwnd,0); time.sleep(.01); u.ShowWindow(hwnd,4); time.sleep(.01)
        u.ShowWindow(hwnd,0)
        settled = expect(False)
        time.sleep(.3)
        final = snap()
        assert final['spectrumPackets'] == settled['spectrumPackets'] and final['spectrumPeak'] == 0
        results['cycles'] = cycles
        results['final'] = final
        results['children'] = [p.name() for p in tracked.children(recursive=True)]
        results['nonzeroSignalObserved'] = any(row['spectrumPeak'] > 0 for row in visible)
        results['playbackCommandsSent'] = 0
        print('PASS: loopback capture, page/hidden release, 12 cycles, rapid visibility changes', flush=True)
    finally:
        close(proc,hwnd)
(OUT/('live-spectrum-quick.json' if seconds == 5 else 'live-spectrum-results.json')).write_text(json.dumps(results,indent=2),encoding='utf-8')
