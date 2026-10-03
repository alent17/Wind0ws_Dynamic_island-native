"""Isolated legacy native countdown-window control smoke for manifest candidates."""
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'artifacts' / 'manifest-timer-controls'
OUT.mkdir(parents=True, exist_ok=True)
exe = Path(os.environ.get('ISLE_TEST_EXE', ROOT / 'target/release/isle-native.exe')).resolve()
config = OUT / 'settings.json'
report = OUT / 'snapshot.json'
config.write_text(json.dumps({'futureTimerTest': {'preserve': [1, None, 3]}}), encoding='utf-8')

u = c.WinDLL('user32', use_last_error=True)
u.EnumWindows.argtypes = [c.c_void_p, w.LPARAM]
u.EnumWindows.restype = w.BOOL
u.EnumChildWindows.argtypes = [w.HWND, c.c_void_p, w.LPARAM]
u.EnumChildWindows.restype = w.BOOL
u.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
u.GetWindowThreadProcessId.restype = w.DWORD
u.GetClassNameW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
u.GetClassNameW.restype = c.c_int
u.GetDlgCtrlID.argtypes = [w.HWND]
u.GetDlgCtrlID.restype = c.c_int
u.GetWindowTextW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
u.GetWindowTextW.restype = c.c_int
u.IsWindow.argtypes = [w.HWND]
u.IsWindow.restype = w.BOOL
u.IsWindowVisible.argtypes = [w.HWND]
u.IsWindowVisible.restype = w.BOOL
u.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.PostMessageW.restype = w.BOOL
u.SendMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.SendMessageW.restype = c.c_ssize_t

ENUM = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)


def class_name(hwnd):
    buf = c.create_unicode_buffer(256)
    u.GetClassNameW(hwnd, buf, len(buf))
    return buf.value


def top_windows(pid):
    found = []

    @ENUM
    def callback(hwnd, _):
        owner = w.DWORD()
        u.GetWindowThreadProcessId(hwnd, c.byref(owner))
        if owner.value == pid:
            found.append(hwnd)
        return True

    u.EnumWindows(callback, 0)
    return found


def find_unique(pid, wanted_class):
    found = [hwnd for hwnd in top_windows(pid) if class_name(hwnd) == wanted_class]
    if len(found) != 1:
        raise AssertionError(f'expected one owned {wanted_class} window for PID {pid}; found {found}')
    hwnd = found[0]
    owner = w.DWORD()
    u.GetWindowThreadProcessId(hwnd, c.byref(owner))
    if not u.IsWindow(hwnd) or owner.value != pid or class_name(hwnd) != wanted_class:
        raise AssertionError('owned window failed HWND/PID/class validation')
    return hwnd


def child_controls(parent, pid):
    found = []

    @ENUM
    def callback(hwnd, _):
        owner = w.DWORD()
        u.GetWindowThreadProcessId(hwnd, c.byref(owner))
        if owner.value == pid:
            found.append({'hwnd': hwnd, 'class': class_name(hwnd), 'controlId': u.GetDlgCtrlID(hwnd)})
        return True

    u.EnumChildWindows(parent, callback, 0)
    return found


def post(hwnd, message, wp=0, lp=0):
    if not u.PostMessageW(hwnd, message, wp, lp):
        raise OSError(c.get_last_error(), f'PostMessage failed: hwnd={hwnd} message={message:#x}')


def text(hwnd):
    buf = c.create_unicode_buffer(128)
    u.SendMessageW(hwnd, 0x000D, len(buf), c.cast(buf, c.c_void_p).value)  # WM_GETTEXT
    return buf.value


def fresh_snapshot(host, pid, previous_mtime):
    main = find_unique(pid, 'IsleNativePrototype')
    if main != host:
        raise AssertionError(f'owned main HWND changed: {host} -> {main}')
    post(host, 0x803C)
    for _ in range(100):
        if report.exists() and report.stat().st_mtime_ns != previous_mtime:
            try:
                data = json.loads(report.read_text(encoding='utf-8'))
            except json.JSONDecodeError:
                time.sleep(.02)
                continue
            if data.get('prototype') is True and data.get('renderer'):
                keys = set(data)
                if 'audioPolls' in keys or 'mediaPolls' in keys:
                    raise AssertionError('demo fixture unexpectedly created live audio/media services')
                return data
        time.sleep(.02)
    raise AssertionError('fresh isolated-demo snapshot did not arrive')


def snapshot(host, pid):
    before = report.stat().st_mtime_ns if report.exists() else 0
    return fresh_snapshot(host, pid, before)


def wait_snapshot(host, pid, predicate, message, timeout=3.0):
    deadline = time.monotonic() + timeout
    state = None
    while time.monotonic() < deadline:
        state = snapshot(host, pid)
        if predicate(state):
            return state
        time.sleep(.025)
    raise AssertionError(f'{message}; latest fresh snapshot={state}')


def wait_until(predicate, message, timeout=3.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(.025)
    raise AssertionError(message)


binary_sha = hashlib.sha256(exe.read_bytes()).hexdigest()
proc = subprocess.Popen([
    str(exe), '--demo', '--page', 'music', '--paused', '--open-timer',
    '--settings-path', str(config), '--log', str(report),
])
print(f'OWNED_PID={proc.pid}', flush=True)
owned_processes = []
result = None
failure = None
graceful_close_posted = False
shutdown_wait_ms = None
try:
    deadline = time.monotonic() + 8
    host = timer = None
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            raise AssertionError(f'owned app exited during startup: {proc.returncode}')
        wins = top_windows(proc.pid)
        main_wins = [h for h in wins if class_name(h) == 'IsleNativePrototype']
        timer_wins = [h for h in wins if class_name(h) == 'IsleNativeTimerWindow']
        if len(main_wins) == 1 and len(timer_wins) == 1:
            host, timer = main_wins[0], timer_wins[0]
            break
        time.sleep(.025)
    if host is None:
        raise AssertionError(f'owned host/timer windows not ready; PID={proc.pid}; topWindows={[(h, class_name(h)) for h in top_windows(proc.pid)]}')
    for window, expected in ((host, 'IsleNativePrototype'), (timer, 'IsleNativeTimerWindow')):
        owner = w.DWORD()
        u.GetWindowThreadProcessId(window, c.byref(owner))
        if not u.IsWindow(window) or owner.value != proc.pid or class_name(window) != expected:
            raise AssertionError(f'window ownership/class validation failed for {window}')

    initial = snapshot(host, proc.pid)
    if initial.get('configurationValid') is not True:
        raise AssertionError('isolated timer configuration did not load')
    controls = child_controls(timer, proc.pid)
    edits = [row for row in controls if row['class'] == 'Edit' and row['controlId'] == 300]
    if len(edits) != 1:
        raise AssertionError(f'expected a unique owned timer Edit control ID 300; children={controls}')
    edit = edits[0]['hwnd']
    if not u.IsWindow(edit) or class_name(edit) != 'Edit' or not u.IsWindow(timer):
        raise AssertionError('timer/Edit control became invalid')
    initial_text = text(edit)
    if initial_text != '30' or u.IsWindowVisible(edit):
        raise AssertionError(f'custom edit initial state mismatch: text={initial_text!r}, visible={bool(u.IsWindowVisible(edit))}')

    # Open the custom-time row on the owned timer HWND, then exercise its real EDIT child.
    post(timer, 0x0202, 0, (317 << 16) | 150)  # WM_LBUTTONUP, custom duration row
    wait_until(lambda: bool(u.IsWindowVisible(edit)), 'custom-time Edit did not become visible')
    stable_text_buffer = c.create_unicode_buffer('25')
    set_text_result = u.SendMessageW(edit, 0x000C, 0, c.cast(stable_text_buffer, c.c_void_p).value)  # WM_SETTEXT
    if text(edit) != '25':
        raise AssertionError(f'custom-time Edit value did not update: {text(edit)!r}')

    post(timer, 0x0100, 0x20, 0)  # WM_KEYDOWN / Space => toggle countdown
    state = wait_snapshot(host, proc.pid,
                          lambda data: data.get('timerRunning') and 1495 <= data.get('timerLeft', 0) <= 1500,
                          'custom 25-minute countdown did not start within the expected 1495–1500 second range')
    started = state
    wait_until(lambda: not bool(u.IsWindowVisible(edit)), 'active countdown did not hide the custom-time Edit')

    post(timer, 0x0100, 0x20, 0)
    paused = wait_snapshot(host, proc.pid,
                           lambda data: not data.get('timerRunning') and 1495 <= data.get('timerLeft', 0) <= 1500,
                           'countdown did not pause near its selected 25-minute duration')

    post(timer, 0x0100, 0x52, 0)  # R => reset
    wait_until(lambda: bool(u.IsWindowVisible(edit)), 'reset did not restore the custom-time Edit')
    reset = snapshot(host, proc.pid)
    if reset.get('timerRunning') or abs(reset.get('timerLeft', 0) - 1500) > .001:
        raise AssertionError(f'countdown reset did not restore exactly 1500 seconds: {reset}')
    if text(edit) != '25':
        raise AssertionError(f'timer edit lost selected custom value after reset: {text(edit)!r}')

    timer_controls_after = child_controls(timer, proc.pid)
    if timer_controls_after != controls:
        raise AssertionError(f'timer native child-control set changed unexpectedly: {timer_controls_after}')
    result = {
        'binarySha256': binary_sha,
        'ownedPid': proc.pid,
        'mainHwnd': int(host),
        'timerHwnd': int(timer),
        'timerClass': class_name(timer),
        'initialTimerChildren': [{'class': row['class'], 'controlId': row['controlId']} for row in controls],
        'customEditInitialText': initial_text,
        'customEditVisibleWhenSelected': True,
        'customEditTextAfterSet': text(edit),
        'crossProcessWmSetTextResult': set_text_result,
        'customDurationStarted': {'timerRunning': started.get('timerRunning'), 'timerLeft': started.get('timerLeft')},
        'paused': {'timerRunning': paused.get('timerRunning'), 'timerLeft': paused.get('timerLeft')},
        'reset': {'timerRunning': reset.get('timerRunning'), 'timerLeft': reset.get('timerLeft')},
        'demoServiceFieldsAbsent': True,
        'configurationValid': initial.get('configurationValid'),
    }
except Exception as exc:
    failure = {
        'binarySha256': binary_sha,
        'ownedPid': proc.pid,
        'exception': repr(exc),
        'mainHwnd': int(host) if 'host' in locals() and host else None,
        'timerHwnd': int(timer) if 'timer' in locals() and timer else None,
        'forcedKill': False,
    }
    raise
finally:
    if proc.poll() is None:
        if 'timer' in locals() and timer and u.IsWindow(timer):
            owner = w.DWORD()
            u.GetWindowThreadProcessId(timer, c.byref(owner))
            if owner.value == proc.pid and class_name(timer) == 'IsleNativeTimerWindow':
                post(timer, 0x0010)  # WM_CLOSE timer auxiliary window
        if 'host' in locals() and host and u.IsWindow(host):
            owner = w.DWORD()
            u.GetWindowThreadProcessId(host, c.byref(owner))
            if owner.value == proc.pid and class_name(host) == 'IsleNativePrototype':
                post(host, 0x0010)  # WM_CLOSE owned main HWND; never terminate a process
                graceful_close_posted = True
        started_close = time.monotonic()
        try:
            proc.wait(timeout=5)
            shutdown_wait_ms = round((time.monotonic() - started_close) * 1000)
        except subprocess.TimeoutExpired:
            failure = {'pid': proc.pid, 'mainHwnd': int(host) if 'host' in locals() and host else None,
                       'timerHwnd': int(timer) if 'timer' in locals() and timer else None,
                       'gracefulClosePosted': True, 'shutdownWaitMs': 5000, 'forcedKill': False,
                       'binarySha256': binary_sha}
            (OUT / 'timeout.json').write_text(json.dumps(failure, indent=2), encoding='utf-8')
            raise AssertionError('owned timer UI did not close within 5000 ms; no forced termination was issued')
    if proc.poll() is not None and not owned_processes:
        owned_processes.append({'pid': proc.pid, 'returncode': proc.returncode,
                                'forcedKill': False, 'gracefulClose': graceful_close_posted,
                                'shutdownWaitMs': shutdown_wait_ms})
    if proc.returncode not in (None, 0):
        failure = failure or {'binarySha256': binary_sha, 'ownedPid': proc.pid,
                              'forcedKill': False}
        failure['ownedExitCode'] = proc.returncode
        failure['reason'] = 'owned process returned nonzero exit code after cleanup'
    if result is not None and proc.returncode == 0:
        result['ownedProcesses'] = owned_processes
        (OUT / 'results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    else:
        failure = failure or {'binarySha256': binary_sha, 'ownedPid': proc.pid,
                              'reason': 'test did not complete', 'forcedKill': False}
        failure['ownedProcesses'] = owned_processes
        (OUT / 'last-failure.json').write_text(json.dumps(failure, indent=2), encoding='utf-8')

if result is None:
    raise AssertionError('timer UI smoke did not complete')
print('PASS: owned native timer EDIT, custom duration, start/pause/reset, isolated demo and graceful exit')
