"""V2 native integration with isolated settings and fixture data.

Uses window messages against this script's own processes, never hardware input
or installed settings. Screenshots are a separate visual acceptance step.
"""
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
from interaction import ROOT, OUT, wait_window, u, ENUM

u.GetDlgItem.argtypes = [w.HWND, c.c_int]
u.GetDlgItem.restype = w.HWND
u.GetClassNameW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
u.SetWindowTextW.argtypes = [w.HWND, w.LPCWSTR]
u.SetWindowTextW.restype = w.BOOL
u.InvalidateRect.argtypes = [w.HWND, c.c_void_p, w.BOOL]
u.UpdateWindow.argtypes = [w.HWND]
u.UpdateWindow.restype = w.BOOL
u.GetGuiResources.argtypes = [w.HANDLE, w.DWORD]
u.GetGuiResources.restype = w.DWORD
exe = Path(os.environ.get('ISLE_TEST_EXE', ROOT / 'target/release/isle-native.exe')).resolve()
folder = Path(os.environ.get('ISLE_TEST_OUTPUT_DIR', OUT / 'ui-v2')).resolve()
try:
    folder.relative_to(OUT.resolve())
except ValueError as error:
    raise RuntimeError('ISLE_TEST_OUTPUT_DIR must stay under native/artifacts') from error
folder.mkdir(exist_ok=True)
config = folder / 'settings.json'
report = folder / 'snapshot.json'
fixture = {'future': {'keep': [1, None, 3]}, 'compactLength': 100,
           'floatingFillColor': '#102030', 'widgetShelf': [
               {'id': 'music', 'order': 0, 'span': 1, 'enabled': True},
               {'id': 'future-widget', 'order': 50, 'span': 8,
                'enabled': True, 'future': {'version': 2}}]}
results = []


def launch(*extra):
    proc = subprocess.Popen([str(exe), '--ui-v2', '--test-fixture', '--page', 'music',
                             '--paused', '--reduced-motion', '--benchmark',
                             '--settings-path', str(config), '--log', str(report), *extra])
    return proc, wait_window(proc)


def close_owned(proc, hwnd):
    """Close only this run's verified main HWND; never force-terminate a test PID."""
    if proc.poll() is not None:
        raise AssertionError(
            f'owned process {proc.pid} exited before graceful WM_CLOSE: {proc.returncode}')
    owner = w.DWORD()
    u.GetWindowThreadProcessId(hwnd, c.byref(owner))
    class_name = c.create_unicode_buffer(256)
    u.GetClassNameW(hwnd, class_name, len(class_name))
    if owner.value != proc.pid or class_name.value != 'IsleNativePrototype':
        raise AssertionError((
            'refusing to close a window without exact owned PID/class',
            hwnd, owner.value, class_name.value, proc.pid))
    if not u.PostMessageW(hwnd, 0x0010, 0, 0):
        raise c.WinError(c.get_last_error())
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired as error:
        timeout = folder / f'timeout-owned-{proc.pid}-{time.strftime("%Y%m%d-%H%M%S")}.json'
        timeout.write_text(json.dumps({
            'pid': proc.pid, 'hwnd': int(hwnd), 'className': class_name.value,
            'wmClosePosted': True, 'waitSeconds': 5, 'forcedKill': False,
        }, indent=2), encoding='utf-8')
        raise AssertionError(
            f'owned process {proc.pid} did not exit after WM_CLOSE; no kill was issued') from error
    if proc.returncode != 0:
        raise AssertionError(f'owned process {proc.pid} exited with {proc.returncode}')


def snapshot(hwnd):
    before = report.stat().st_mtime_ns if report.exists() else 0
    u.PostMessageW(hwnd, 0x803c, 0, 0)
    for _ in range(100):
        if report.exists() and report.stat().st_mtime_ns != before:
            try:
                return json.loads(report.read_text(encoding='utf-8'))
            except json.JSONDecodeError:
                pass
        time.sleep(.01)
    raise AssertionError('diagnostic snapshot timeout')


def expect(hwnd, predicate):
    state = None
    for _ in range(200):
        state = snapshot(hwnd)
        if predicate(state):
            return state
        time.sleep(.01)
    raise AssertionError(state)


def settings_window(proc, hwnd):
    u.PostMessageW(hwnd, 0x100, 0x77, 0)
    expect(hwnd, lambda state: state['settingsWindowAlive'])
    found = []

    @ENUM
    def callback(window, _):
        pid = w.DWORD()
        u.GetWindowThreadProcessId(window, c.byref(pid))
        name = c.create_unicode_buffer(128)
        u.GetClassNameW(window, name, 128)
        if pid.value == proc.pid and name.value == 'IsleNativeSettingsV2':
            found.append(window)
        return True

    u.EnumWindows(callback, 0)
    assert len(found) == 1, found
    return found[0]


def slider(window, control_id, value, end=False):
    control = u.GetDlgItem(window, control_id)
    assert control, control_id
    u.SendMessageW(control, 0x405, 1, value)
    u.SendMessageW(window, 0x114, 8 if end else 5, control)


def button(window, control_id):
    control = u.GetDlgItem(window, control_id)
    assert control, control_id
    u.SendMessageW(control, 0xf5, 0, 0)


def settings_resize_paint_close(proc, hwnd, iteration):
    window = settings_window(proc, hwnd)
    for nav_id in [303, 305, 301, 300]:
        button(window, nav_id)

    original = w.RECT()
    u.GetWindowRect(window, c.byref(original))
    width = original.right - original.left
    height = original.bottom - original.top
    assert u.SetWindowPos(window, w.HWND(0), original.left, original.top,
                          width + 16, height + 12, 0x0014), iteration
    resized = w.RECT()
    u.GetWindowRect(window, c.byref(resized))
    assert resized.right - resized.left == width + 16, (iteration, resized)
    state = snapshot(hwnd)  # diagnostic after the real WM_SIZE
    assert state['settingsWindowAlive'], (iteration, state)

    assert u.SetWindowPos(window, w.HWND(0), original.left, original.top,
                          width, height, 0x0014), iteration
    u.InvalidateRect(window, None, True)
    u.UpdateWindow(window)  # synchronous WM_PAINT
    state = snapshot(hwnd)  # diagnostic after WM_PAINT
    assert state['settingsWindowAlive'], (iteration, state)

    u.PostMessageW(window, 0x10, 0, 0)
    state = expect(hwnd, lambda value: not value['settingsWindowAlive'])
    return state


config.write_text(json.dumps(fixture), encoding='utf-8')
proc, hwnd = launch()
try:
    state = expect(hwnd, lambda state: state['uiV2'] and state['expanded'])
    window = settings_window(proc, hwnd)
    for control_id in [104, 105, 212]:
        assert not u.GetDlgItem(window, control_id), ('V2 has an Apply button', control_id)
    button(window, 301)
    for value in range(101, 181):
        slider(window, 220, value)
    state = expect(hwnd, lambda state: state['compactLength'] == 180)
    # Runtime must reflect the final drag before debounce commits it.
    assert state['runtimeRevision'] >= state['persistedRevision']
    state = expect(hwnd, lambda state: not state['configurationDirty'])
    saved = json.loads(config.read_text(encoding='utf-8'))
    assert saved['compactLength'] == 180
    assert saved['future'] == fixture['future']
    assert saved['widgetShelf'][1] == fixture['widgetShelf'][1]
    assert config.with_suffix('.previous.json').exists()
    results.append({'case': 'coalesced-runtime-save-unknown-preservation', 'snapshot': state})

    button(window, 240)
    expect(hwnd, lambda state: state['inspectionLocked'])
    button(window, 300)
    expect(hwnd, lambda state: not state['inspectionLocked'])
    results.append({'case': 'inspection-unlocks-on-leaving-appearance', 'pass': True})

    # External edits must remain intact even while Runtime continues changing.
    external = b'{"externalChange":{"preserve":true},"compactLength":92}'
    config.write_bytes(external)
    button(window, 301)
    slider(window, 220, 190)
    state = expect(hwnd, lambda state: state['configurationSaveError'])
    assert state['compactLength'] == 190 and config.read_bytes() == external
    button(window, 305)
    button(window, 108)
    expect(hwnd, lambda state: state['configurationSaveError'])
    assert config.read_bytes() == external
    button(window, 109)
    state = expect(hwnd, lambda state: not state['configurationDirty'])
    assert state['compactLength'] == 180 and config.read_bytes() == external
    results.append({'case': 'external-conflict-visible-retry-safe-revert-runtime', 'snapshot': state})
finally:
    close_owned(proc, hwnd)
assert proc.returncode == 0, proc.returncode

# Closing while the last slider edit is still debouncing flushes that edit.
config.write_text(json.dumps(fixture), encoding='utf-8')
proc, hwnd = launch()
try:
    window = settings_window(proc, hwnd)
    button(window, 301)
    slider(window, 220, 207)
    expect(hwnd, lambda state: state['compactLength'] == 207)
finally:
    close_owned(proc, hwnd)
assert proc.returncode == 0, proc.returncode
assert json.loads(config.read_text(encoding='utf-8'))['compactLength'] == 207
proc, hwnd = launch()
try:
    state = expect(hwnd, lambda state: state['compactLength'] == 207)
    results.append({'case': 'exit-flush-restart-consistent', 'snapshot': state})
finally:
    close_owned(proc, hwnd)
assert proc.returncode == 0, proc.returncode

# Reopen the real V2 settings window, change pages, force real WM_SIZE / WM_PAINT,
# and verify each cycle remains diagnosable and releases its GDI resources.
config.write_text(json.dumps(fixture), encoding='utf-8')
proc, hwnd = launch()
try:
    expect(hwnd, lambda state: state['uiV2'] and state['rendererAlive'])
    handle = w.HANDLE(proc._handle)
    warmup_gdi = []
    for iteration in range(5):
        settings_resize_paint_close(proc, hwnd, f'warmup-{iteration}')
        warmup_gdi.append(u.GetGuiResources(handle, 0))
    baseline_gdi = warmup_gdi[-1]
    closed_samples = []
    for iteration in range(30):
        state = settings_resize_paint_close(proc, hwnd, iteration)
        closed_samples.append(u.GetGuiResources(handle, 0))

    assert max(closed_samples) - min(closed_samples) <= 4, closed_samples
    assert max(abs(count - baseline_gdi) for count in closed_samples) <= 4, (
        baseline_gdi, closed_samples)
    results.append({
        'case': 'settings-open-page-resize-paint-close-30x-gdi-stability',
        'cycles': len(closed_samples), 'initialGdiHandles': baseline_gdi,
        'warmupGdiHandles': warmup_gdi, 'closedGdiHandles': closed_samples,
        'steadyRange': max(closed_samples) - min(closed_samples),
        'settingsWindowAliveAfterLastClose': state['settingsWindowAlive'],
    })
finally:
    close_owned(proc, hwnd)
assert proc.returncode == 0, proc.returncode

(folder / 'results.json').write_text(json.dumps({
    'binarySha256': hashlib.sha256(exe.read_bytes()).hexdigest(),
    'cases': results, 'screenshotsVerified': False,
}, indent=2), encoding='utf-8')
print('PASS: V2 live save, conflict recovery, inspection, exit flush and 30 settings lifecycle cycles')
