"""V2 live-save player selector checks with isolated config and fixture media.

The player list performs read-only system session discovery. The script never
sends media-control, volume, mute, or device-selection commands.
"""
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

from interaction import ROOT, OUT, wait_window, close, u, ENUM

if os.environ.get('ISLE_TEST_MONITOR') != r'\\.\DISPLAY2':
    raise SystemExit(r"Set $env:ISLE_TEST_MONITOR='\\.\DISPLAY2' before running UI tests")

u.GetDlgItem.argtypes = [w.HWND, c.c_int]
u.GetDlgItem.restype = w.HWND
u.GetClassNameW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
u.GetWindowTextW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
u.IsWindowEnabled.argtypes = [w.HWND]
u.IsWindowEnabled.restype = w.BOOL

exe = Path(os.environ.get('ISLE_TEST_EXE', ROOT / 'target/release/isle-native.exe')).resolve()
folder = OUT / 'ui-v2-players'
folder.mkdir(exist_ok=True)
config = folder / 'settings.json'
a, b = 'IsleTest.OfflineA', 'IsleTest.OfflineB'
fixture = {
    'selectedPlayerIds': [a],
    'playerOrderIds': [a, b],
    'futurePlayerQa': {'preserve': [1, None, {'version': 2}]},
}
config.write_text(json.dumps(fixture), encoding='utf-8')
cases = []
proc = None
hwnd = None
report = None


def launch(name):
    global report
    report = folder / f'{name}-snapshot.json'
    proc = subprocess.Popen([
        str(exe), '--ui-v2', '--test-fixture', '--page', 'music', '--paused',
        '--reduced-motion', '--benchmark', '--settings-path', str(config),
        '--log', str(report),
    ])
    return proc, wait_window(proc)


def snapshot():
    before = report.stat().st_mtime_ns if report.exists() else 0
    u.PostMessageW(hwnd, 0x803c, 0, 0)
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        if report.exists() and report.stat().st_mtime_ns != before:
            try:
                return json.loads(report.read_text(encoding='utf-8'))
            except json.JSONDecodeError:
                pass
        if proc is not None and proc.poll() is not None:
            raise AssertionError(f'owned Isle process exited with {proc.returncode}')
        time.sleep(.01)
    raise AssertionError('diagnostic snapshot timeout')


def expect(predicate, timeout=12):
    deadline = time.monotonic() + timeout
    state = None
    while time.monotonic() < deadline:
        state = snapshot()
        if predicate(state):
            return state
        time.sleep(.025)
    raise AssertionError(('condition timeout', state))


def own_window(class_name):
    found = []

    @ENUM
    def callback(window, _):
        pid = w.DWORD()
        u.GetWindowThreadProcessId(window, c.byref(pid))
        name = c.create_unicode_buffer(128)
        u.GetClassNameW(window, name, len(name))
        if pid.value == proc.pid and name.value == class_name:
            found.append(window)
        return True

    u.EnumWindows(callback, 0)
    assert len(found) == 1, (class_name, found)
    return found[0]


def click(window, control_id):
    control = u.GetDlgItem(window, control_id)
    assert control and u.IsWindowEnabled(control), ('disabled or missing control', control_id)
    u.SendMessageW(control, 0xf5, 0, 0)  # BM_CLICK


def open_settings():
    u.PostMessageW(hwnd, 0x100, 0x77, 0)  # F8
    expect(lambda state: state['settingsWindowAlive'])
    return own_window('IsleNativeSettingsV2')


def open_players(settings):
    media_nav = u.GetDlgItem(settings, 303)
    assert media_nav, 'missing Media navigation'
    if u.IsWindowEnabled(media_nav):
        click(settings, 303)
    click(settings, 107)  # Player settings
    state = expect(lambda value: value['playerDialogAlive'] and value['playerListReady'])
    window = own_window('IsleNativePlayers')
    assert state['playerListRows'] >= 2, state
    assert not u.GetDlgItem(window, 307), 'V2 live selector must not expose Save'
    close_text = c.create_unicode_buffer(64)
    u.GetWindowTextW(u.GetDlgItem(window, 2), close_text, len(close_text))
    assert close_text.value == '关闭', close_text.value
    return window, state


def list_rows(window):
    control = u.GetDlgItem(window, 302)
    count = u.SendMessageW(control, 0x18b, 0, 0)  # LB_GETCOUNT
    values = []
    for index in range(count):
        size = u.SendMessageW(control, 0x18a, index, 0)  # LB_GETTEXTLEN
        buffer = c.create_unicode_buffer(max(1, size + 1))
        u.SendMessageW(control, 0x189, index, c.addressof(buffer))  # LB_GETTEXT
        values.append(buffer.value)
    return values


def select_player(window, player_id):
    rows = list_rows(window)
    indexes = [index for index, row in enumerate(rows) if player_id in row]
    assert len(indexes) == 1, (player_id, rows)
    control = u.GetDlgItem(window, 302)
    u.SendMessageW(control, 0x186, indexes[0], 0)  # LB_SETCURSEL
    u.PostMessageW(window, 0x111, 302 | (1 << 16), control)  # LBN_SELCHANGE
    time.sleep(.05)
    return indexes[0]


def check_state(window, control_id):
    return u.SendMessageW(u.GetDlgItem(window, control_id), 0xf0, 0, 0) == 1


def set_check(window, control_id, desired):
    if check_state(window, control_id) != desired:
        click(window, control_id)


def wait_saved(json_predicate, state_predicate):
    deadline = time.monotonic() + 12
    state = None
    while time.monotonic() < deadline:
        state = snapshot()
        try:
            data = json.loads(config.read_text(encoding='utf-8'))
        except (OSError, json.JSONDecodeError):
            data = {}
        if (json_predicate(data) and state_predicate(state)
                and not state['configurationDirty']
                and not state['configurationSaving']
                and state['persistedRevision'] == state['runtimeRevision']):
            assert data['futurePlayerQa'] == fixture['futurePlayerQa'], data
            return state, data
        time.sleep(.025)
    raise AssertionError(('save timeout', state, config.read_text(encoding='utf-8')))


def summary(state):
    return {key: state.get(key) for key in (
        'runtimeRevision', 'persistedRevision', 'configurationDirty',
        'configurationSaveError', 'playerSelectionAutomatic',
        'playerAllowedCount', 'playerOrderCount', 'playerListRows',
    )}


try:
    proc, hwnd = launch('first-run')
    initial = expect(lambda state: state['uiV2'] and state['configurationValid'])
    settings = open_settings()
    players, discovered = open_players(settings)
    assert not discovered['playerSelectionAutomatic'] and discovered['playerAllowedCount'] == 1
    assert a in list_rows(players)[0]

    # Allow all toggles the live runtime immediately and persists null.
    before = snapshot()
    click(players, 301)
    runtime_all = expect(lambda state: state['playerSelectionAutomatic']
                         and state['runtimeRevision'] > before['runtimeRevision'])
    saved_all, data_all = wait_saved(
        lambda data: data.get('selectedPlayerIds', 'missing') is None,
        lambda state: state['playerSelectionAutomatic'],
    )

    # Turning allow-all off restores the fixture's explicit manual selection.
    before = runtime_all
    click(players, 301)
    runtime_manual = expect(lambda state: not state['playerSelectionAutomatic']
                            and state['runtimeRevision'] > before['runtimeRevision'])
    saved_manual, data_manual = wait_saved(
        lambda data: data.get('selectedPlayerIds') == [a],
        lambda state: not state['playerSelectionAutomatic']
                      and state['playerAllowedCount'] == 1,
    )

    # Select and enable the offline row, then persist both order directions.
    select_player(players, b)
    assert not check_state(players, 303)
    click(players, 303)
    saved_enabled, data_enabled = wait_saved(
        lambda data: set(data.get('selectedPlayerIds', [])) == {a, b},
        lambda state: state['playerAllowedCount'] == 2,
    )

    select_player(players, b)
    click(players, 304)  # move up
    saved_up, data_up = wait_saved(
        lambda data: data.get('playerOrderIds', [])[:2] == [b, a],
        lambda state: state['playerOrderCount'] >= 2,
    )
    select_player(players, b)
    click(players, 305)  # move down
    saved_down, data_down = wait_saved(
        lambda data: data.get('playerOrderIds', [])[:2] == [a, b],
        lambda state: state['playerOrderCount'] >= 2,
    )
    cases.extend([
        {'case': 'allow-all-on-immediate-runtime-and-persisted',
         'runtime': summary(runtime_all), 'saved': summary(saved_all),
         'selectedPlayerIdsIsNull': data_all['selectedPlayerIds'] is None},
        {'case': 'allow-all-off-restores-manual-selection',
         'runtime': summary(runtime_manual), 'saved': summary(saved_manual),
         'selectedPlayerIds': data_manual['selectedPlayerIds']},
        {'case': 'select-offline-player-immediate-save',
         'saved': summary(saved_enabled), 'selectedPlayerIds': data_enabled['selectedPlayerIds']},
        {'case': 'priority-up-and-down-immediate-save',
         'moveUp': {'saved': summary(saved_up), 'prefix': data_up['playerOrderIds'][:2]},
         'moveDown': {'saved': summary(saved_down), 'prefix': data_down['playerOrderIds'][:2]}},
    ])

    # Closing and reopening the dialog must reload the saved, explicit selection.
    u.PostMessageW(players, 0x10, 0, 0)
    expect(lambda state: not state['playerDialogAlive'])
    players, reopened = open_players(settings)
    select_player(players, b)
    assert check_state(players, 303), 'saved offline player was not restored in dialog'
    u.PostMessageW(players, 0x10, 0, 0)
    expect(lambda state: not state['playerDialogAlive'])
    cases.append({'case': 'close-reopen-restores-saved-values',
                  'state': summary(reopened), 'selectedPlayerIds': [a, b]})

    # Restart verifies the final saved values independently of the open dialog.
    close(proc, hwnd)
    proc, hwnd = launch('after-restart')
    restarted = expect(lambda state: state['uiV2'] and state['configurationValid']
                       and not state['playerSelectionAutomatic']
                       and state['playerAllowedCount'] == 2
                       and state['playerOrderCount'] >= 2)
    restarted_data = json.loads(config.read_text(encoding='utf-8'))
    assert set(restarted_data['selectedPlayerIds']) == {a, b}, restarted_data
    assert restarted_data['playerOrderIds'][:2] == [a, b], restarted_data
    cases.append({'case': 'restart-restores-selection-and-priority',
                  'state': summary(restarted), 'selectedPlayerIds': restarted_data['selectedPlayerIds'],
                  'priorityPrefix': restarted_data['playerOrderIds'][:2]})

    # A conflicting external edit must survive close, revert, and dialog reopen.
    settings = open_settings()
    players, conflict_start = open_players(settings)
    external = json.dumps({
        'selectedPlayerIds': [b],
        'playerOrderIds': [b, a],
        'futurePlayerQa': {'external': 'preserve'},
    }, separators=(',', ':')).encode('utf-8')
    config.write_bytes(external)
    click(players, 301)
    conflicted = expect(lambda state: state['configurationSaveError']
                        and state['configurationDirty']
                        and state['playerSelectionAutomatic'])
    assert config.read_bytes() == external
    u.PostMessageW(players, 0x10, 0, 0)
    expect(lambda state: not state['playerDialogAlive'])
    click(settings, 305)  # Advanced
    click(settings, 109)  # Revert saved runtime values
    reverted = expect(lambda state: not state['configurationSaveError']
                      and not state['configurationDirty']
                      and not state['playerSelectionAutomatic']
                      and state['playerAllowedCount'] == 2)
    assert config.read_bytes() == external

    players, reopened_after_revert = open_players(settings)
    select_player(players, b)
    assert check_state(players, 303), 'conflicted dialog draft replaced the reverted selection'
    u.PostMessageW(players, 0x10, 0, 0)
    expect(lambda state: not state['playerDialogAlive'])
    time.sleep(.35)
    assert config.read_bytes() == external, 'stale player dialog draft overwrote external config'
    cases.append({'case': 'external-conflict-close-revert-reopen-preserves-external-file',
                  'conflict': summary(conflicted), 'reverted': summary(reverted),
                  'reopened': summary(reopened_after_revert), 'externalFilePreserved': True})

    settings = own_window('IsleNativeSettingsV2')
    u.PostMessageW(settings, 0x10, 0, 0)
    expect(lambda state: not state['settingsWindowAlive'])
    close(proc, hwnd)
    proc = hwnd = None
    (folder / 'results.json').write_text(json.dumps({
        'binarySha256': hashlib.sha256(exe.read_bytes()).hexdigest(),
        'cases': cases,
        'systemPlayerDiscovery': 'read-only; no media controls sent',
        'screenshotsVerified': False,
    }, indent=2), encoding='utf-8')
    print('PASS: V2 player discovery, live allow-list save, offline selection/order, restart, conflict and revert')
finally:
    if proc is not None and hwnd is not None:
        close(proc, hwnd)
