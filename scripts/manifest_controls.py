"""Inspect a candidate's embedded manifest without starting the application.

Creates a temporary activation context only in this inspector process. Loads
ComCtl32 in that context to check that the embedded dependency can resolve.
This does not prove the target's runtime custom-draw callbacks or visual states.
Use only trusted local builds: their activation context selects dependency DLLs,
and this probe loads ComCtl32 and calls its DllGetVersion entry point. Mapping
the target EXE as a resource does not execute that EXE.

References:
https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-actctxw
https://learn.microsoft.com/en-us/windows/win32/controls/cookbook-overview
"""
import argparse
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import os
from pathlib import Path
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[1]
k = c.WinDLL('kernel32', use_last_error=True)
k.LoadLibraryExW.argtypes = [w.LPCWSTR, w.HANDLE, w.DWORD]
k.LoadLibraryExW.restype = w.HMODULE
k.FreeLibrary.argtypes = [w.HMODULE]
k.FreeLibrary.restype = w.BOOL
k.FindResourceW.argtypes = [w.HMODULE, c.c_void_p, c.c_void_p]
k.FindResourceW.restype = w.HANDLE
k.LoadResource.argtypes = [w.HMODULE, w.HANDLE]
k.LoadResource.restype = w.HANDLE
k.LockResource.argtypes = [w.HANDLE]
k.LockResource.restype = c.c_void_p
k.SizeofResource.argtypes = [w.HMODULE, w.HANDLE]
k.SizeofResource.restype = w.DWORD
ENUM_RESOURCE = c.WINFUNCTYPE(w.BOOL, w.HMODULE, c.c_void_p, c.c_void_p, w.LPARAM)
k.EnumResourceNamesW.argtypes = [w.HMODULE, c.c_void_p, ENUM_RESOURCE, w.LPARAM]
k.EnumResourceNamesW.restype = w.BOOL
k.GetModuleHandleW.argtypes = [w.LPCWSTR]
k.GetModuleHandleW.restype = w.HMODULE
k.GetModuleFileNameW.argtypes = [w.HMODULE, w.LPWSTR, w.DWORD]
k.GetModuleFileNameW.restype = w.DWORD


class ActCtx(c.Structure):
    _fields_ = [('cbSize', w.ULONG), ('dwFlags', w.DWORD),
                ('lpSource', w.LPCWSTR), ('wProcessorArchitecture', w.WORD),
                ('wLangId', w.WORD), ('lpAssemblyDirectory', w.LPCWSTR),
                ('lpResourceName', c.c_void_p), ('lpApplicationName', w.LPCWSTR),
                ('hModule', w.HMODULE)]


class DllVersion(c.Structure):
    _fields_ = [('cbSize', w.DWORD), ('dwMajorVersion', w.DWORD),
                ('dwMinorVersion', w.DWORD), ('dwBuildNumber', w.DWORD),
                ('dwPlatformID', w.DWORD)]


k.CreateActCtxW.argtypes = [c.POINTER(ActCtx)]
k.CreateActCtxW.restype = w.HANDLE
k.ActivateActCtx.argtypes = [w.HANDLE, c.POINTER(c.c_size_t)]
k.ActivateActCtx.restype = w.BOOL
k.DeactivateActCtx.argtypes = [w.DWORD, c.c_size_t]
k.DeactivateActCtx.restype = w.BOOL
k.ReleaseActCtx.argtypes = [w.HANDLE]
k.ReleaseActCtx.restype = None


def check(value, message):
    if not value:
        raise AssertionError(message)


def resource(module, kind, name=1):
    found = k.FindResourceW(module, name, kind)
    check(found, f'missing resource type={kind}, id={name}')
    size = k.SizeofResource(module, found)
    loaded = k.LoadResource(module, found)
    pointer = k.LockResource(loaded) if loaded else None
    check(size and pointer, f'cannot read resource type={kind}, id={name}')
    return c.string_at(pointer, size)


def inspect(exe):
    # AS_DATAFILE | AS_IMAGE_RESOURCE: read PE resources without executing it.
    module = k.LoadLibraryExW(str(exe), None, 0x00000002 | 0x00000020)
    if not module:
        raise c.WinError(c.get_last_error())
    try:
        names = []

        @ENUM_RESOURCE
        def collect(_, __, name, ___):
            names.append(name if name <= 0xFFFF else c.wstring_at(name))
            return True

        check(k.EnumResourceNamesW(module, 24, collect, 0),
              'candidate has no embedded RT_MANIFEST')
        check(names == [1], f'expected one process manifest resource #1, got {names}')
        manifest = resource(module, 24).decode('utf-8-sig')
        # These are the existing executable identity resources.
        icon_size = len(resource(module, 14))
        version_size = len(resource(module, 16))
    finally:
        check(k.FreeLibrary(module), 'cannot release mapped candidate resources')

    xml = ET.fromstring(manifest)
    ns = {'v1': 'urn:schemas-microsoft-com:asm.v1',
          'v3': 'urn:schemas-microsoft-com:asm.v3',
          'dpi': 'http://schemas.microsoft.com/SMI/2016/WindowsSettings',
          'legacy_dpi': 'http://schemas.microsoft.com/SMI/2005/WindowsSettings'}
    check(xml.tag == '{urn:schemas-microsoft-com:asm.v1}assembly',
          'manifest root is not an assembly')
    identities = xml.findall('v1:dependency/v1:dependentAssembly/v1:assemblyIdentity', ns)
    common = [item for item in identities
              if item.get('name') == 'Microsoft.Windows.Common-Controls']
    check(len(common) == 1, 'expected exactly one Common Controls dependency')
    check(common[0].get('version') == '6.0.0.0'
          and common[0].get('publicKeyToken') == '6595b64144ccf1df'
          and common[0].get('type') == 'win32', 'incorrect Common Controls v6 identity')
    awareness = xml.find('v3:application/v3:windowsSettings/dpi:dpiAwareness', ns)
    check(awareness is not None
          and 'PerMonitorV2' in [item.strip() for item in (awareness.text or '').split(',')],
          'manifest does not declare PerMonitorV2')
    legacy = xml.find('v3:application/v3:windowsSettings/legacy_dpi:dpiAware', ns)
    check(legacy is not None and (legacy.text or '').strip().lower() == 'true/pm',
          'missing equivalent legacy per-monitor DPI declaration')
    requested = xml.find('v3:trustInfo/v3:security/v3:requestedPrivileges/v3:requestedExecutionLevel', ns)
    check(requested is not None and requested.get('level') == 'asInvoker'
          and requested.get('uiAccess') == 'false', 'manifest unexpectedly requests elevation/UI access')

    # Resolve the dependency in this short-lived inspector, not in the user's
    # running application. Avoid mistaking a previously loaded DLL for a probe.
    check(not k.GetModuleHandleW('comctl32.dll'), 'inspector already loaded ComCtl32')
    activation = ActCtx(cbSize=c.sizeof(ActCtx), dwFlags=0x008,
                        lpSource=str(exe), lpResourceName=1)
    context = k.CreateActCtxW(c.byref(activation))
    if context == c.c_void_p(-1).value:
        raise c.WinError(c.get_last_error())
    cookie = c.c_size_t()
    active = False
    try:
        if not k.ActivateActCtx(context, c.byref(cookie)):
            raise c.WinError(c.get_last_error())
        active = True
        controls = c.WinDLL('comctl32.dll', use_last_error=True)
        controls.DllGetVersion.argtypes = [c.POINTER(DllVersion)]
        controls.DllGetVersion.restype = c.c_long
        version = DllVersion(cbSize=c.sizeof(DllVersion))
        hr = controls.DllGetVersion(c.byref(version))
        check(hr == 0 and version.dwMajorVersion >= 6,
              f'activation context did not resolve ComCtl32 v6: HRESULT={hr}, major={version.dwMajorVersion}')
        library_path = c.create_unicode_buffer(32768)
        check(k.GetModuleFileNameW(controls._handle, library_path, len(library_path)),
              'cannot resolve the inspected ComCtl32 path')
    finally:
        try:
            if active:
                check(k.DeactivateActCtx(0, cookie), 'cannot deactivate inspector context')
        finally:
            k.ReleaseActCtx(context)

    return {'binary': str(exe), 'binarySha256': hashlib.sha256(exe.read_bytes()).hexdigest(),
            'manifestResourceNames': names, 'commonControlsIdentity': common[0].attrib,
            'dpiAwareness': awareness.text, 'legacyDpiAware': legacy.text,
            'executionLevel': requested.attrib,
            'iconResourceBytes': icon_size, 'versionResourceBytes': version_size,
            'activationContextCreated': True, 'inspectionComctl32Path': library_path.value,
            'inspectionComctl32Version': {'major': version.dwMajorVersion,
                                         'minor': version.dwMinorVersion,
                                         'build': version.dwBuildNumber},
            'targetApplicationLaunched': False, 'targetRuntimeVersionVerified': False,
            'screenshotsVerified': False,
            'limitations': ['activation tested only in this inspector process',
                            'target custom-draw callbacks and visual states require separate QA']}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', type=Path, default=Path(os.environ.get(
        'ISLE_TEST_EXE', ROOT / 'target/release/isle-native.exe')))
    args = parser.parse_args()
    candidate = args.exe.resolve(strict=True)
    evidence = inspect(candidate)
    folder = ROOT / 'artifacts/manifest-controls'
    folder.mkdir(parents=True, exist_ok=True)
    (folder / 'results.json').write_text(json.dumps(evidence, indent=2), encoding='utf-8')
    print(f"PASS: embedded v6/PMv2 manifest and inspector activation; SHA256 {evidence['binarySha256']}")
