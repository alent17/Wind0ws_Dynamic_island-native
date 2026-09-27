"""Exercise process/window teardown and the static scheduler without changing OS settings."""
import json
import subprocess
import time
from interaction import ROOT, OUT, wait_window, close, u

results = []
for index in range(30):
    report = OUT / f'lifecycle-{index}.json'
    page = ['music', 'volume', 'timer', 'clock', 'weather'][index % 5]
    proc = subprocess.Popen([str(ROOT/'target/release/isle-native.exe'),
                             '--page', page, '--paused', '--reduced-motion', '--log', str(report)])
    hwnd = wait_window(proc)
    try:
        time.sleep(.3)
        if page != 'music':
            u.PostMessageW(hwnd, 0x100, 0x1b, 0)
        u.PostMessageW(hwnd, 0x100, 0x1b, 0)
        time.sleep(.05)
    finally:
        close(proc, hwnd)
    assert proc.returncode == 0
    data = json.loads(report.read_text())
    assert data['livePages'] == 0, (index, data)
    assert data['timerIntervalMs'] == 0, (index, data)
    assert data['fontFamily'] == 'MiSans', data
    results.append({'iteration': index, 'exitCode': proc.returncode, 'report': data})

# Clock owns a minute-boundary timer, never a continuous frame timer at rest.
report = OUT/'clock-lifecycle.json'
proc = subprocess.Popen([str(ROOT/'target/release/isle-native.exe'), '--page', 'clock',
                         '--paused', '--reduced-motion', '--log', str(report)])
hwnd = wait_window(proc)
try:
    time.sleep(1)
    u.PostMessageW(hwnd, 0x1e, 0, 0)  # WM_TIMECHANGE re-arms the boundary.
    time.sleep(.1)
finally:
    close(proc, hwnd)
clock = json.loads(report.read_text())
assert 1 <= clock['timerIntervalMs'] <= 60000, clock
assert clock['intervalSamples'] == 0, clock
(OUT/'lifecycle-results.json').write_text(json.dumps({'windows': results, 'clock': clock}, indent=2))
print('30 process/window closes released pages and timers; MiSans and clock timer verified')
