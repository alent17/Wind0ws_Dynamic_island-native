"""Isolated V2 Settings placement, DPI viewport, and persistence regression.

All changes are confined to per-case fixture JSON and owned test processes.
The synthetic 200% message is a viewport exercise, not a hardware DPI test.
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

exe = Path(os.environ.get('ISLE_TEST_EXE', ROOT / 'target/release/isle-native.exe')).resolve()
folder = OUT / 'ui-v2-placement'
folder.mkdir(parents=True, exist_ok=True)
WM_CLOSE = 0x0010
WM_KEYDOWN = 0x0100
WM_KEYUP = 0x0101
WM_SIZE = 0x0005
WM_DPICHANGED = 0x02E0
WM_GETMINMAXINFO = 0x0024
WM_ENTERSIZEMOVE = 0x0231
WM_EXITSIZEMOVE = 0x0232
WM_HSCROLL = 0x0114
BM_CLICK = 0x00F5
CB_GETCURSEL = 0x0147
SB_RIGHT = 7
VK_END = 0x23
SWP_NOZORDER = 0x0004
SWP_NOACTIVATE = 0x0010
WINDOW_CLASS = 'IsleNativeSettingsV2'

u.GetDlgItem.argtypes = [w.HWND, c.c_int]
u.GetDlgItem.restype = w.HWND
u.GetClassNameW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
u.GetClassNameW.restype = c.c_int
u.GetWindowTextW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
u.GetWindowTextW.restype = c.c_int
u.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
u.GetWindowRect.restype = w.BOOL
u.GetClientRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
u.GetClientRect.restype = w.BOOL
u.ClientToScreen.argtypes = [w.HWND, c.POINTER(w.POINT)]
u.ClientToScreen.restype = w.BOOL
u.SetWindowPos.argtypes = [w.HWND, w.HWND, c.c_int, c.c_int, c.c_int, c.c_int, w.UINT]
u.SetWindowPos.restype = w.BOOL
u.SendMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.SendMessageW.restype = c.c_ssize_t
u.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.PostMessageW.restype = w.BOOL
u.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
u.GetWindowThreadProcessId.restype = w.DWORD
u.GetDpiForWindow.argtypes = [w.HWND]
u.GetDpiForWindow.restype = w.UINT
u.MonitorFromWindow.argtypes = [w.HWND, w.DWORD]
u.MonitorFromWindow.restype = w.HANDLE
u.GetMonitorInfoW.argtypes = [w.HANDLE, c.c_void_p]
u.GetMonitorInfoW.restype = w.BOOL
u.IsWindowVisible.argtypes = [w.HWND]
u.IsWindowVisible.restype = w.BOOL
u.IsWindowEnabled.argtypes = [w.HWND]
u.IsWindowEnabled.restype = w.BOOL
u.GetGUIThreadInfo.argtypes = [w.DWORD, c.c_void_p]
u.GetGUIThreadInfo.restype = w.BOOL
u.GetDlgCtrlID.argtypes = [w.HWND]
u.GetDlgCtrlID.restype = c.c_int
u.IsChild.argtypes = [w.HWND, w.HWND]
u.IsChild.restype = w.BOOL
gdi = c.windll.gdi32
gdi.GetObjectW.argtypes = [w.HANDLE, c.c_int, c.c_void_p]
gdi.GetObjectW.restype = c.c_int


class MonitorInfo(c.Structure):
    _fields_ = [('cbSize', w.DWORD), ('rcMonitor', w.RECT),
                ('rcWork', w.RECT), ('dwFlags', w.DWORD)]


class Point(c.Structure):
    _fields_ = [('x', c.c_int), ('y', c.c_int)]


class MinMaxInfo(c.Structure):
    _fields_ = [('ptReserved', Point), ('ptMaxSize', Point),
                ('ptMaxPosition', Point), ('ptMinTrackSize', Point),
                ('ptMaxTrackSize', Point)]


class GuiThreadInfo(c.Structure):
    _fields_ = [('cbSize', w.DWORD), ('flags', w.DWORD), ('hwndActive', w.HWND),
                ('hwndFocus', w.HWND), ('hwndCapture', w.HWND),
                ('hwndMenuOwner', w.HWND), ('hwndMoveSize', w.HWND),
                ('hwndCaret', w.HWND), ('rcCaret', w.RECT)]


class LogFontW(c.Structure):
    _fields_ = [('lfHeight', c.c_long), ('lfWidth', c.c_long),
                ('lfEscapement', c.c_long), ('lfOrientation', c.c_long),
                ('lfWeight', c.c_long), ('lfItalic', c.c_ubyte),
                ('lfUnderline', c.c_ubyte), ('lfStrikeOut', c.c_ubyte),
                ('lfCharSet', c.c_ubyte), ('lfOutPrecision', c.c_ubyte),
                ('lfClipPrecision', c.c_ubyte), ('lfQuality', c.c_ubyte),
                ('lfPitchAndFamily', c.c_ubyte), ('lfFaceName', w.WCHAR * 32)]


results = []
sequence = 0


def rect(hwnd):
    value = w.RECT()
    if not u.GetWindowRect(hwnd, c.byref(value)):
        raise c.WinError(c.get_last_error())
    return (value.left, value.top, value.right, value.bottom)


def rect_dict(value):
    left, top, right, bottom = value
    return {'left': left, 'top': top, 'right': right, 'bottom': bottom,
            'width': right - left, 'height': bottom - top}


def overlap(first, second):
    return (max(first[0], second[0]) < min(first[2], second[2]) and
            max(first[1], second[1]) < min(first[3], second[3]))


def monitor_work(hwnd):
    monitor = u.MonitorFromWindow(hwnd, 2)  # MONITOR_DEFAULTTONEAREST
    info = MonitorInfo()
    info.cbSize = c.sizeof(info)
    if not u.GetMonitorInfoW(monitor, c.byref(info)):
        raise c.WinError(c.get_last_error())
    r = info.rcWork
    return (r.left, r.top, r.right, r.bottom)


def within(inner, outer):
    return (inner[0] >= outer[0] and inner[1] >= outer[1] and
            inner[2] <= outer[2] and inner[3] <= outer[3])


def client_screen_rect(hwnd):
    value = w.RECT()
    if not u.GetClientRect(hwnd, c.byref(value)):
        raise c.WinError(c.get_last_error())
    top_left = w.POINT(value.left, value.top)
    bottom_right = w.POINT(value.right, value.bottom)
    if not u.ClientToScreen(hwnd, c.byref(top_left)) or \
            not u.ClientToScreen(hwnd, c.byref(bottom_right)):
        raise c.WinError(c.get_last_error())
    return (top_left.x, top_left.y, bottom_right.x, bottom_right.y)


def fresh_snapshot(hwnd, report):
    before = report.stat().st_mtime_ns if report.exists() else 0
    u.PostMessageW(hwnd, 0x803C, 0, 0)
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        if report.exists() and report.stat().st_mtime_ns != before:
            try:
                return json.loads(report.read_text(encoding='utf-8'))
            except json.JSONDecodeError:
                pass
        time.sleep(.01)
    raise AssertionError(f'diagnostic snapshot timeout: {report}')


def expect(hwnd, report, predicate, description):
    deadline = time.monotonic() + 5
    state = None
    while time.monotonic() < deadline:
        state = fresh_snapshot(hwnd, report)
        if predicate(state):
            return state
        time.sleep(.02)
    raise AssertionError((description, state))


def find_settings(proc):
    found = []

    @ENUM
    def callback(window, _):
        owner = w.DWORD()
        u.GetWindowThreadProcessId(window, c.byref(owner))
        if owner.value == proc.pid:
            name = c.create_unicode_buffer(128)
            if u.GetClassNameW(window, name, len(name)) and name.value == WINDOW_CLASS:
                found.append(window)
        return True

    u.EnumWindows(callback, 0)
    if len(found) != 1:
        raise AssertionError(('owned Settings HWND count', proc.pid, found))
    return found[0]


def open_settings(proc, main, report):
    u.PostMessageW(main, WM_KEYDOWN, 0x77, 0)  # F8
    expect(main, report, lambda value: value.get('settingsWindowAlive') is True,
           'F8 opens V2 Settings')
    return find_settings(proc)


def close_settings(main, window, report):
    u.PostMessageW(window, WM_CLOSE, 0, 0)
    expect(main, report, lambda value: value.get('settingsWindowAlive') is False,
           'WM_CLOSE closes V2 Settings')


def close_owned(proc, main):
    """Graceful close only; a forced termination is recorded and fails QA."""
    forced = False
    if proc.poll() is None:
        u.PostMessageW(main, WM_CLOSE, 0, 0)
        try:
            proc.wait(timeout=8)
        except subprocess.TimeoutExpired:
            forced = True
            proc.terminate()
            proc.wait(timeout=3)
    if forced or proc.returncode != 0:
        raise AssertionError(('owned process did not exit cleanly', proc.pid,
                              proc.returncode, 'forcedKill', forced))
    return {'pid': proc.pid, 'returncode': proc.returncode, 'forcedKill': forced}


def launch(name, config):
    global sequence
    sequence += 1
    report = folder / f'{sequence:02d}-{name}.json'
    if report.exists():
        report.unlink()
    proc = subprocess.Popen([
        str(exe), '--ui-v2', '--test-fixture', '--page', 'music', '--paused',
        '--reduced-motion', '--settings-path', str(config), '--log', str(report)])
    main = wait_window(proc)
    expect(main, report, lambda state: state.get('uiV2') is True,
           'V2 main window diagnostic ready')
    return proc, main, report


def placement_for(rectangle, dpi):
    left, top, right, bottom = rectangle
    return {
        'x': left, 'y': top,
        'widthDip': ((right - left) * 96 + dpi // 2) // dpi,
        'heightDip': ((bottom - top) * 96 + dpi // 2) // dpi,
        'dpi': dpi,
    }


def expected_px(dip, dpi):
    return (dip * dpi + 48) // 96


def wait_saved(config, predicate, description, timeout=6):
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        try:
            last = json.loads(config.read_text(encoding='utf-8'))
            if predicate(last):
                return last
        except (OSError, json.JSONDecodeError):
            pass
        time.sleep(.025)
    raise AssertionError((description, last))


def gesture(window, desired):
    """Drive the same move/size entry/exit messages Windows sends for a gesture."""
    u.SendMessageW(window, WM_ENTERSIZEMOVE, 0, 0)
    left, top, right, bottom = desired
    if not u.SetWindowPos(window, w.HWND(0), left, top, right-left, bottom-top,
                          SWP_NOZORDER | SWP_NOACTIVATE):
        raise c.WinError(c.get_last_error())
    u.SendMessageW(window, WM_EXITSIZEMOVE, 0, 0)
    time.sleep(.25)  # placement persistence has a 200 ms debounce
    return rect(window)


def button(window, control_id):
    child = u.GetDlgItem(window, control_id)
    if not child:
        raise AssertionError(('missing button', control_id))
    if not u.IsWindowEnabled(child):
        return False  # clicking the current page is correctly disabled
    u.SendMessageW(child, BM_CLICK, 0, 0)
    return True


def child_text(hwnd):
    text = c.create_unicode_buffer(256)
    u.GetWindowTextW(hwnd, text, len(text))
    return text.value


def child_rect_for_text(parent, text):
    found = []

    @ENUM
    def callback(child, _):
        if child_text(child) == text:
            found.append(child)
        return True

    u.EnumChildWindows(parent, callback, 0)
    if len(found) != 1:
        raise AssertionError(('static text count', text, found))
    return rect(found[0])


def nav_rects(settings):
    return {control_id: rect(u.GetDlgItem(settings, control_id))
            for control_id in range(300, 306)}


def assert_nav_geometry(settings):
    client = client_screen_rect(settings)
    rectangles = nav_rects(settings)
    visibility = {}
    for control_id, rectangle in rectangles.items():
        child = u.GetDlgItem(settings, control_id)
        visible = bool(u.IsWindowVisible(child))
        enabled = bool(u.IsWindowEnabled(child))
        assert visible, ('navigation button is not visible', control_id)
        visibility[control_id] = {'visible': visible, 'enabled': enabled}
        assert rectangle[2] > rectangle[0] and rectangle[3] > rectangle[1], (
            'navigation has positive area', control_id, rectangle)
        assert within(rectangle, client), ('navigation clipped by client',
                                           control_id, rectangle, client)
    ids = sorted(rectangles)
    for index, first_id in enumerate(ids):
        for second_id in ids[index + 1:]:
            assert not overlap(rectangles[first_id], rectangles[second_id]), (
                'navigation controls overlap', first_id, second_id,
                rectangles[first_id], rectangles[second_id])
    return {'client': rect_dict(client),
            'navigation': {str(key): rect_dict(value)
                           for key, value in rectangles.items()},
            'visibility': {str(key): value for key, value in visibility.items()}}


def focus_thread(settings):
    pid = w.DWORD()
    thread = u.GetWindowThreadProcessId(settings, c.byref(pid))
    info = GuiThreadInfo()
    info.cbSize = c.sizeof(info)
    if not u.GetGUIThreadInfo(thread, c.byref(info)):
        raise c.WinError(c.get_last_error())
    return info.hwndFocus


def tab_to_control(settings, target, limit=48):
    """Traverse the actual Settings message pump's dialog keyboard route."""
    visited = []
    for _ in range(limit):
        current = focus_thread(settings)
        if current == target:
            return visited
        if not current or not u.IsChild(settings, current):
            raise AssertionError(('focus left owned Settings tree', current, target))
        control_id = u.GetDlgCtrlID(current)
        visited.append(control_id)
        if not u.PostMessageW(current, WM_KEYDOWN, 0x09, 0):  # VK_TAB
            raise c.WinError(c.get_last_error())
        deadline = time.monotonic() + .5
        changed = current
        while time.monotonic() < deadline:
            changed = focus_thread(settings)
            if changed != current:
                break
            time.sleep(.01)
        if changed == current:
            raise AssertionError(('Tab did not move Settings focus', current,
                                  control_id, target))
        u.PostMessageW(changed, WM_KEYUP, 0x09, 0)
    raise AssertionError(('Tab traversal limit exceeded', visited, target))


def visit_pages(settings):
    visited = []
    clicked = []
    page_markers = {300: 208, 301: 211, 302: 200, 303: 107,
                    304: 101, 305: 108}
    for control_id in range(300, 306):
        if button(settings, control_id):
            clicked.append(control_id)
            time.sleep(.015)
        marker = u.GetDlgItem(settings, page_markers[control_id])
        assert marker and u.IsWindowVisible(marker), (
            'page-specific control did not become visible after navigation',
            control_id, page_markers[control_id], marker)
        visited.append(control_id)
        assert_nav_geometry(settings)
    assert len(visited) == 6, ('not all Settings pages verified', visited)
    return {'visited': visited, 'clicked': clicked}


def font_height(hwnd):
    font = u.SendMessageW(hwnd, 0x0031, 0, 0)  # WM_GETFONT
    if not font:
        raise AssertionError(('control has no font', hwnd))
    info = LogFontW()
    if not gdi.GetObjectW(w.HANDLE(font), c.sizeof(info), c.byref(info)):
        raise c.WinError(c.get_last_error())
    return info.lfHeight


def no_manual_default_case(seed):
    config = folder / 'default-no-write.json'
    raw = json.dumps({'qaFuture': {'keep': [1, None, 'default']}},
                     separators=(',', ':')).encode()
    config.write_bytes(raw)
    proc, main, report = launch('default-no-write', config)
    try:
        settings = open_settings(proc, main, report)
        current, owner, work = rect(settings), rect(main), monitor_work(settings)
        actual_dpi = u.GetDpiForWindow(settings)
        assert within(current, work), ('default settings outside monitor work area',
                                       current, work)
        window_width = current[2] - current[0]
        window_height = current[3] - current[1]
        gap = 20
        centered_y = work[1] + ((work[3] - work[1]) - window_height) // 2
        vertical_overlap = owner[1] < centered_y + window_height and \
            owner[3] > centered_y
        horizontal_slot = (
            owner[0] - work[0] >= window_width + gap or
            work[2] - owner[2] >= window_width + gap)
        has_nonoverlap_slot = not vertical_overlap or horizontal_slot
        if has_nonoverlap_slot:
            assert not overlap(current, owner), ('default position overlaps owner despite available slot',
                                                 current, owner, work)
        else:
            centered_x = work[0] + ((work[2] - work[0]) - window_width) // 2
            assert abs(current[0] - centered_x) <= 1 and \
                    abs(current[1] - centered_y) <= 1, (
                        'no-slot fallback did not center in work area',
                        current, owner, work, centered_x, centered_y)
        time.sleep(.35)
        close_settings(main, settings, report)
        exit_info = close_owned(proc, main)
        assert config.read_bytes() == raw, ('default placement wrote configuration',
                                            config.read_bytes())
        state = {'case': 'default-auto-avoid-does-not-persist',
                 'settingsRect': rect_dict(current), 'ownerRect': rect_dict(owner),
                 'workArea': rect_dict(work), 'actualDpi': actual_dpi,
                 'nonoverlapSlotAvailable': has_nonoverlap_slot,
                 'result': 'avoided-owner' if has_nonoverlap_slot else 'centered-no-fit',
                 'exit': exit_info}
        results.append(state)
        return placement_for(current, actual_dpi)
    except Exception:
        if proc.poll() is None:
            close_owned(proc, main)
        raise


def autoavoid_with_room_case():
    config = folder / 'default-autoavoid-room.json'
    raw = json.dumps({'qaFuture': {'noWrite': ['autoavoid', None]}},
                     separators=(',', ':')).encode()
    config.write_bytes(raw)
    proc, main, report = launch('default-autoavoid-room', config)
    try:
        original_owner = rect(main)
        work = monitor_work(main)
        owner_width = original_owner[2] - original_owner[0]
        owner_height = original_owner[3] - original_owner[1]
        left = work[0] + 12
        top = min(max(original_owner[1], work[1]), work[3] - owner_height)
        assert u.SetWindowPos(main, w.HWND(0), left, top, owner_width, owner_height,
                              SWP_NOZORDER | SWP_NOACTIVATE), c.WinError(c.get_last_error())
        time.sleep(.05)
        settings = open_settings(proc, main, report)
        current, owner = rect(settings), rect(main)
        assert within(current, work), ('auto-avoid window escaped work area', current, work)
        assert not overlap(current, owner), ('auto-avoid failed with clear side slot',
                                             current, owner, work)
        close_settings(main, settings, report)
        exit_info = close_owned(proc, main)
        assert config.read_bytes() == raw, ('auto-avoid wrote an unrequested placement',
                                            config.read_bytes())
        results.append({'case': 'default-autoavoid-when-side-space-exists',
                        'settingsRect': rect_dict(current),
                        'ownerRect': rect_dict(owner), 'workArea': rect_dict(work),
                        'ownerBeforeMove': rect_dict(original_owner), 'exit': exit_info})
    except Exception:
        if proc.poll() is None:
            close_owned(proc, main)
        raise


def invalid_recovery_cases(saved):
    for name, x, y in [('offscreen', 100000, 100000),
                       ('coordinate-overflow', 2147483600, 2147483600)]:
        config = folder / f'{name}-no-write.json'
        placement = dict(saved)
        placement.update({'x': x, 'y': y})
        fixture = {
            'qaFuture': {'qaPlacementFuture': {'version': 17,
                                               'values': ['retain', None]}},
            'settingsWindowPlacement': placement,
        }
        raw = json.dumps(fixture, separators=(',', ':')).encode()
        config.write_bytes(raw)
        proc, main, report = launch(f'{name}-no-write', config)
        try:
            settings = open_settings(proc, main, report)
            current = rect(settings)
            work = monitor_work(settings)
            assert within(current, work), (f'{name} saved position not recovered',
                                           current, work)
            time.sleep(.3)
            close_settings(main, settings, report)
            exit_info = close_owned(proc, main)
            assert config.read_bytes() == raw, (
                f'{name} recovery overwrote original saved value',
                json.loads(config.read_text(encoding='utf-8')))
            results.append({'case': f'{name}-recovery-does-not-overwrite-saved-value',
                            'savedPlacement': placement,
                            'recoveredRect': rect_dict(current),
                            'workArea': rect_dict(work), 'exit': exit_info,
                            'limitation': 'No monitor topology was changed; no-overlap models a missing saved monitor.'})
        except Exception:
            if proc.poll() is None:
                close_owned(proc, main)
            raise


def manual_restart_and_unknown_case(default_placement):
    config = folder / 'manual-restart-unknown.json'
    unknown = {'future': {'version': 9, 'keep': ['nested', None, {'n': 3}]}}
    initial_placement = dict(default_placement)
    initial_placement['future'] = unknown['future']
    fixture = {'settingsWindowPlacement': initial_placement,
               'enableAnimations': False,
               'qaFuture': {'preserve': True}}
    config.write_text(json.dumps(fixture), encoding='utf-8')
    proc, main, report = launch('manual-placement', config)
    try:
        settings = open_settings(proc, main, report)
        before, work, owner = rect(settings), monitor_work(settings), rect(main)
        dpi = u.GetDpiForWindow(settings)
        width = min(before[2] - before[0] + 52, work[2] - work[0])
        height = min(before[3] - before[1] + 38, work[3] - work[1])
        # Place the Settings window over its owner to prove explicit manual
        # placement takes precedence over default auto-avoid.
        left = min(max(owner[0], work[0]), work[2] - width)
        top = min(max(owner[1], work[1]), work[3] - height)
        actual = gesture(settings, (left, top, left + width, top + height))
        expected = placement_for(actual, dpi)
        saved = wait_saved(config, lambda value:
                           value.get('settingsWindowPlacement', {}).get('x') == expected['x'] and
                           value.get('settingsWindowPlacement', {}).get('y') == expected['y'],
                           'manual placement was persisted')
        placed = saved['settingsWindowPlacement']
        assert all(placed.get(key) == expected[key]
                   for key in ['x', 'y', 'widthDip', 'heightDip', 'dpi']), (
                       'saved placement differs from simulated manual gesture',
                       expected, placed)
        assert placed['future'] == unknown['future'], ('nested unknown placement field lost',
                                                        placed)
        assert overlap(actual, owner), ('manual placement over owner did not overlap',
                                        actual, owner)

        close_settings(main, settings, report)
        reopened = open_settings(proc, main, report)
        reopen_rect = rect(reopened)
        reopen_dpi = u.GetDpiForWindow(reopened)
        assert reopen_rect[0] == expected['x'] and reopen_rect[1] == expected['y'], (
            'same-process reopen position mismatch', expected, reopen_rect)
        assert abs((reopen_rect[2] - reopen_rect[0]) -
                   expected_px(expected['widthDip'], reopen_dpi)) <= 1, (
                       'same-process reopen width mismatch', expected, reopen_rect, reopen_dpi)
        assert abs((reopen_rect[3] - reopen_rect[1]) -
                   expected_px(expected['heightDip'], reopen_dpi)) <= 1, (
                       'same-process reopen height mismatch', expected, reopen_rect, reopen_dpi)
        close_settings(main, reopened, report)
        first_exit = close_owned(proc, main)

        proc, main, report = launch('restart-placement', config)
        reopened = open_settings(proc, main, report)
        restart_rect = rect(reopened)
        current_dpi = u.GetDpiForWindow(reopened)
        assert restart_rect[0] == expected['x'] and restart_rect[1] == expected['y'], (
            'process restart position mismatch', expected, restart_rect)
        assert abs((restart_rect[2] - restart_rect[0]) -
                   expected_px(expected['widthDip'], current_dpi)) <= 1, (
                       'restart width mismatch at current actual DPI', expected,
                       restart_rect, current_dpi)
        assert abs((restart_rect[3] - restart_rect[1]) -
                   expected_px(expected['heightDip'], current_dpi)) <= 1, (
                       'restart height mismatch at current actual DPI', expected,
                       restart_rect, current_dpi)

        # A change unrelated to placement must retain unknown nested placement data.
        button(reopened, 300)  # General page; it is disabled only if already active.
        checkbox = u.GetDlgItem(reopened, 208)
        assert checkbox, 'missing General animation checkbox'
        u.SendMessageW(checkbox, BM_CLICK, 0, 0)
        changed = wait_saved(config, lambda value: value.get('enableAnimations') is True,
                             'unrelated setting change persisted')
        assert changed['settingsWindowPlacement']['future'] == unknown['future'], (
            'unrelated setting save lost nested placement field', changed)
        assert changed['qaFuture'] == fixture['qaFuture'], (
            'unrelated setting save lost top-level unknown field', changed)
        close_settings(main, reopened, report)
        second_exit = close_owned(proc, main)
        results.append({'case': 'manual-move-resize-close-reopen-restart-unknown-preserved',
                        'requested': expected, 'saved': placed,
                        'sameProcessRect': rect_dict(reopen_rect),
                        'restartRect': rect_dict(restart_rect),
                        'restartActualDpi': current_dpi,
                        'unrelatedSetting': changed['enableAnimations'],
                        'unknownPlacementPreserved': True,
                        'firstExit': first_exit, 'restartExit': second_exit})
        return placed
    except Exception:
        if proc.poll() is None:
            close_owned(proc, main)
        raise


def parent_close_dirty_case(saved):
    config = folder / 'parent-close-dirty.json'
    config.write_text(json.dumps({'settingsWindowPlacement': dict(saved),
                                  'qaFuture': {'duringClose': True}}), encoding='utf-8')
    proc, main, report = launch('parent-close-dirty', config)
    try:
        settings = open_settings(proc, main, report)
        before = rect(settings)
        work = monitor_work(settings)
        width, height = before[2] - before[0], before[3] - before[1]
        left = max(work[0], min(before[0] + 31, work[2] - width))
        top = max(work[1], min(before[1] + 27, work[3] - height))
        u.SendMessageW(settings, WM_ENTERSIZEMOVE, 0, 0)
        assert u.SetWindowPos(settings, w.HWND(0), left, top, width + 11, height + 9,
                              SWP_NOZORDER | SWP_NOACTIVATE), c.WinError(c.get_last_error())
        final_rect = rect(settings)
        expected = placement_for(final_rect, u.GetDpiForWindow(settings))
        # Deliberately omit WM_EXITSIZEMOVE; parent shutdown must harvest the
        # in-progress move and flush its pending placement before exiting.
        exit_info = close_owned(proc, main)
        saved_doc = wait_saved(config, lambda value:
                               value.get('settingsWindowPlacement', {}).get('x') == expected['x'] and
                               value.get('settingsWindowPlacement', {}).get('y') == expected['y'],
                               'parent shutdown did not flush active move', timeout=4)
        actual = saved_doc['settingsWindowPlacement']
        assert all(actual.get(key) == expected[key]
                   for key in ['x', 'y', 'widthDip', 'heightDip', 'dpi']), (expected, actual)
        results.append({'case': 'parent-close-flushes-active-manual-move',
                        'expected': expected, 'saved': actual, 'exit': exit_info})
    except Exception:
        if proc.poll() is None:
            close_owned(proc, main)
        raise


def narrow_viewport_case(saved):
    config = folder / 'synthetic-narrow-200dpi.json'
    config.write_text(json.dumps({'settingsWindowPlacement': dict(saved),
                                  'qaViewport': 'synthetic-only'}), encoding='utf-8')
    proc, main, report = launch('synthetic-narrow-200dpi', config)
    try:
        settings = open_settings(proc, main, report)
        work = monitor_work(settings)
        original = rect(settings)
        actual_dpi_before_synthetic = u.GetDpiForWindow(settings)
        initial_font_heights = {control_id: font_height(u.GetDlgItem(settings, control_id))
                                for control_id in range(300, 306)}
        work_width, work_height = work[2] - work[0], work[3] - work[1]
        viewport_width = min(1366, work_width)
        viewport_height = min(690, work_height)
        # Send a synthetic per-window 200% DPI suggestion. This does not change
        # the operating system or monitor DPI; it exercises the window handler.
        suggestion = w.RECT(work[0], work[1], work[0] + viewport_width,
                            work[1] + viewport_height)
        u.SendMessageW(settings, WM_DPICHANGED, (192 | (192 << 16)),
                       c.addressof(suggestion))
        assert u.SetWindowPos(settings, w.HWND(0), work[0], work[1],
                              viewport_width, viewport_height,
                              SWP_NOZORDER | SWP_NOACTIVATE), c.WinError(c.get_last_error())
        time.sleep(.1)
        synthetic_rect = rect(settings)
        assert synthetic_rect[2] - synthetic_rect[0] <= work_width and \
                synthetic_rect[3] - synthetic_rect[1] <= work_height, (
                    'synthetic suggested viewport escaped physical work area',
                    synthetic_rect, work)

        limits = MinMaxInfo()
        u.SendMessageW(settings, WM_GETMINMAXINFO, 0, c.addressof(limits))
        assert 0 < limits.ptMinTrackSize.x <= work_width, (
            'min-track width not bounded by actual work area',
            limits.ptMinTrackSize.x, work_width)
        assert 0 < limits.ptMinTrackSize.y <= work_height, (
            'min-track height not bounded by actual work area',
            limits.ptMinTrackSize.y, work_height)

        # All six navigation children must remain visible, non-overlapping and
        # inside the actual client under the short high-scale viewport.
        nav_checks = assert_nav_geometry(settings)
        synthetic_font_heights = {
            control_id: font_height(u.GetDlgItem(settings, control_id))
            for control_id in range(300, 306)}
        assert all(abs(height) == 28 for height in synthetic_font_heights.values()), (
            'navigation font size did not follow synthetic 200% scale',
            synthetic_font_heights)
        normal_navigation = visit_pages(settings)

        # The product's min-track height correctly clamps a 200% window to the
        # real work area. Exercise the compact-DIP branch at synthetic 400%
        # against that unchanged work area instead of bypassing the constraint.
        compact_suggestion = w.RECT(work[0], work[1], work[2], work[3])
        u.SendMessageW(settings, WM_DPICHANGED, (384 | (384 << 16)),
                       c.addressof(compact_suggestion))
        assert u.SetWindowPos(settings, w.HWND(0), work[0], work[1],
                              work_width, work_height,
                              SWP_NOZORDER | SWP_NOACTIVATE), c.WinError(c.get_last_error())
        time.sleep(.08)
        compact_client = client_screen_rect(settings)
        compact_client_height_dip = (compact_client[3] - compact_client[1]) / 4
        assert compact_client_height_dip < 270, (
            'short viewport did not enter compact-height regime',
            compact_client_height_dip, rect(settings), work)
        compact_navigation = assert_nav_geometry(settings)
        compact_font_heights = {
            control_id: font_height(u.GetDlgItem(settings, control_id))
            for control_id in range(300, 306)}
        assert all(abs(height) == 56 for height in compact_font_heights.values()), (
            'navigation font size did not follow synthetic 400% scale',
            compact_font_heights)
        compact_rectangles = nav_rects(settings)
        compact_columns = sorted({value[0] for value in compact_rectangles.values()})
        compact_rows = sorted({value[1] for value in compact_rectangles.values()})
        assert len(compact_columns) == 2 and len(compact_rows) == 3, (
            'compact navigation is not two columns by three rows',
            compact_columns, compact_rows, compact_rectangles)
        compact_page_navigation = visit_pages(settings)

        # Restore synthetic 200% and the requested narrow viewport before
        # checking right-edge scrolling and keyboard reachability.
        narrow_suggestion = w.RECT(work[0], work[1],
                                   work[0] + viewport_width,
                                   work[1] + viewport_height)
        u.SendMessageW(settings, WM_DPICHANGED, (192 | (192 << 16)),
                       c.addressof(narrow_suggestion))
        assert u.SetWindowPos(settings, w.HWND(0), work[0], work[1],
                              viewport_width, viewport_height,
                              SWP_NOZORDER | SWP_NOACTIVATE), c.WinError(c.get_last_error())
        time.sleep(.08)
        restored_200_rect = rect(settings)
        restored_200_client = client_screen_rect(settings)
        restored_200_font_heights = {
            control_id: font_height(u.GetDlgItem(settings, control_id))
            for control_id in range(300, 306)}
        assert all(abs(height) == 28 for height in restored_200_font_heights.values()), (
            'navigation font size did not restore at synthetic 200%',
            restored_200_font_heights)
        button(settings, 301)  # Appearance page, where the rightmost combo lives.

        edge = u.GetDlgItem(settings, 213)
        assert edge, 'missing rightmost edge selector'
        before_edge = rect(edge)
        before_label = child_rect_for_text(settings, '贴边方向')
        u.SendMessageW(settings, WM_HSCROLL, SB_RIGHT, 0)
        after_edge = rect(edge)
        after_label = child_rect_for_text(settings, '贴边方向')
        client = client_screen_rect(settings)
        assert within(after_edge, client), ('rightmost control not reachable after horizontal scroll',
                                            after_edge, client)
        edge_dx = after_edge[0] - before_edge[0]
        label_dx = after_label[0] - before_label[0]
        assert edge_dx < 0 and label_dx == edge_dx, (
            'horizontal scroll did not move label and control together',
            edge_dx, label_dx, before_edge, after_edge)

        # Traverse via Tab messages delivered through the app's actual UI pump.
        tab_path = tab_to_control(settings, edge)
        assert focus_thread(settings) == edge, ('rightmost control not keyboard-focusable',
                                                focus_thread(settings), edge)
        keyboard_before = u.SendMessageW(edge, CB_GETCURSEL, 0, 0)
        u.PostMessageW(edge, WM_KEYDOWN, VK_END, 0)
        u.PostMessageW(edge, WM_KEYUP, VK_END, 0)
        deadline = time.monotonic() + 1
        selected = u.SendMessageW(edge, CB_GETCURSEL, 0, 0)
        while selected != 3 and time.monotonic() < deadline:
            time.sleep(.02)
            selected = u.SendMessageW(edge, CB_GETCURSEL, 0, 0)
        assert keyboard_before >= 0 and selected == 3, (
            'focused combo did not accept End keyboard navigation',
            keyboard_before, selected)

        # No synthetic DPI or viewport test should persist placement.
        close_settings(main, settings, report)
        exit_info = close_owned(proc, main)
        doc = json.loads(config.read_text(encoding='utf-8'))
        assert doc.get('settingsWindowPlacement') == saved, (
            'synthetic viewport change persisted as a manual placement', doc)
        results.append({'case': 'synthetic-200dpi-narrow-viewport-scroll-keyboard-nav',
                        'physicalWorkArea': rect_dict(work),
                        'requestedViewport': {'widthPx': viewport_width,
                                              'heightPx': viewport_height,
                                              'exact1366px': viewport_width == 1366},
                        'syntheticWindowRect': rect_dict(synthetic_rect),
                        'minTrack': {'width': limits.ptMinTrackSize.x,
                                     'height': limits.ptMinTrackSize.y},
                        'navigation': nav_checks,
                        'normalPageNavigation': normal_navigation,
                        'compactClientHeightDip': compact_client_height_dip,
                        'compactColumns': compact_columns,
                        'compactRows': compact_rows,
                        'compactFontHeightsAtSynthetic400Percent': compact_font_heights,
                        'compactNavigation': compact_navigation,
                        'compactPageNavigation': compact_page_navigation,
                        'restored200WindowRect': rect_dict(restored_200_rect),
                        'restored200Client': rect_dict(restored_200_client),
                        'restored200FontHeights': restored_200_font_heights,
                        'fontHeightsBeforeSyntheticDpi': initial_font_heights,
                        'fontHeightsAtSynthetic200Percent': synthetic_font_heights,
                        'edgeBeforeScroll': rect_dict(before_edge),
                        'edgeAfterScroll': rect_dict(after_edge),
                        'labelControlScrollDx': edge_dx,
                        'keyboardFocusedControlId': 213,
                        'keyboardTabPathControlIds': tab_path,
                        'keyboardIndexBefore': keyboard_before,
                        'keyboardSelectedIndex': selected,
                        'actualMonitorDpi': actual_dpi_before_synthetic,
                        'syntheticDpi': 192,
                        'exit': exit_info,
                        'limitations': ['synthetic viewport/DPI only; no OS DPI or display change']})
    except Exception:
        if proc.poll() is None:
            close_owned(proc, main)
        raise


def main():
    if not exe.is_file():
        raise FileNotFoundError(f'candidate binary missing: {exe}')
    default_placement = no_manual_default_case(None)
    autoavoid_with_room_case()
    invalid_recovery_cases(default_placement)
    saved = manual_restart_and_unknown_case(default_placement)
    parent_close_dirty_case(saved)
    narrow_viewport_case(saved)
    payload = {
        'binary': str(exe),
        'binarySha256': hashlib.sha256(exe.read_bytes()).hexdigest(),
        'cases': results,
        'screenshotsVerified': False,
        'physicalDpiHardwareVerified': False,
        'note': 'Synthetic WM_DPICHANGED is software coverage only; real monitor DPI and work area were read, not changed.',
    }
    (folder / 'results.json').write_text(json.dumps(payload, indent=2), encoding='utf-8')
    print(f"PASS: {len(results)} V2 placement cases; SHA256 {payload['binarySha256']}")


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        payload = {
            'binary': str(exe),
            'binarySha256': hashlib.sha256(exe.read_bytes()).hexdigest() if exe.is_file() else None,
            'casesPassedBeforeFailure': results,
            'failure': repr(error),
            'screenshotsVerified': False,
            'physicalDpiHardwareVerified': False,
        }
        (folder / 'failure.json').write_text(json.dumps(payload, indent=2), encoding='utf-8')
        raise
