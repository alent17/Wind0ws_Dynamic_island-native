param([switch]$ReadOnlyProbe)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$hostMetadata=[ordered]@{
    powershellVersion=$PSVersionTable.PSVersion.ToString()
    powershellEdition=$PSVersionTable.PSEdition
    frameworkDescription=[Runtime.InteropServices.RuntimeInformation]::FrameworkDescription
    uiAutomationClientAssembly=([AppDomain]::CurrentDomain.GetAssemblies()|Where-Object{$_.GetName().Name -eq 'UIAutomationClient'}|Select-Object -First 1).Location
    uiAutomationTypesAssembly=([AppDomain]::CurrentDomain.GetAssemblies()|Where-Object{$_.GetName().Name -eq 'UIAutomationTypes'}|Select-Object -First 1).Location
}
# UIAutomationClient does not always load the Windows client-side proxy assembly
# just because the client and type assemblies are present. Register the official
# Microsoft Win32/MSAA proxies before making the first AutomationElement call.
$clientProxyPath=Join-Path $env:WINDIR 'Microsoft.NET\assembly\GAC_MSIL\UIAutomationClientsideProviders\v4.0_4.0.0.0__31bf3856ad364e35\UIAutomationClientsideProviders.dll'
$clientProxyAssembly=$null
$clientProxyRegistration=[ordered]@{assemblyPath=$clientProxyPath;assemblyLoaded=$false;registered=$false;error=$null}
try {
    if(!(Test-Path -LiteralPath $clientProxyPath)) { throw 'Microsoft GAC proxy assembly is unavailable at the expected .NET Framework path' }
    $clientProxyAssembly=[System.Reflection.Assembly]::LoadFile($clientProxyPath)
    $clientProxyRegistration.assemblyLoaded=$true
    [System.Windows.Automation.ClientSettings]::RegisterClientSideProviderAssembly($clientProxyAssembly.GetName())
    $clientProxyRegistration.registered=$true
} catch {
    $clientProxyRegistration.error=$_.Exception.ToString()
}

if(-not ('SettingsUiaNativeProbe' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
public static class SettingsUiaNativeProbe {
    delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr lParam);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumWindowsProc callback, IntPtr lParam);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd, StringBuilder name, int max);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] static extern bool IsWindow(IntPtr hwnd);
    [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr hwnd, int command);
    [DllImport("user32.dll", SetLastError=true)] static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
    public static IntPtr[] FindTop(int pid, string expectedClass) {
        var found=new List<IntPtr>();
        EnumWindows(delegate(IntPtr hwnd, IntPtr unused) {
            uint owner; GetWindowThreadProcessId(hwnd,out owner);
            if(owner==(uint)pid && Class(hwnd)==expectedClass) found.Add(hwnd);
            return true;
        },IntPtr.Zero);
        return found.ToArray();
    }
    public static IntPtr[] Children(int pid, IntPtr parent) {
        var found=new List<IntPtr>();
        EnumChildWindows(parent,delegate(IntPtr hwnd, IntPtr unused) {
            uint owner; GetWindowThreadProcessId(hwnd,out owner);
            if(owner==(uint)pid) found.Add(hwnd);
            return true;
        },IntPtr.Zero);
        return found.ToArray();
    }
    public static string Class(IntPtr hwnd) {
        var name=new StringBuilder(256); GetClassName(hwnd,name,name.Capacity); return name.ToString();
    }
    public static int OwnerPid(IntPtr hwnd) { uint owner; GetWindowThreadProcessId(hwnd,out owner); return (int)owner; }
    public static void ShowForPaint(IntPtr hwnd,int pid,string expectedClass) {
        Require(hwnd,pid,expectedClass);
        ShowWindow(hwnd,4); // SW_SHOWNOACTIVATE: preserve visible paint without activating the UI.
    }
    public static void Require(IntPtr hwnd,int pid,string expectedClass) {
        if(hwnd==IntPtr.Zero || !IsWindow(hwnd) || OwnerPid(hwnd)!=pid || Class(hwnd)!=expectedClass)
            throw new InvalidOperationException("HWND/PID/class ownership check failed: "+hwnd+" PID="+pid+" class="+expectedClass);
    }
    public static void PostChecked(IntPtr hwnd,int pid,string expectedClass,uint message,IntPtr wParam,IntPtr lParam) {
        Require(hwnd,pid,expectedClass);
        if(!PostMessage(hwnd,message,wParam,lParam)) throw new Win32Exception(Marshal.GetLastWin32Error(),"PostMessage failed for owned HWND "+hwnd);
    }
}
'@
}

if(-not ('SettingsUiaCrossCheck' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

[ComImport, Guid("30cbe57d-d9d0-452a-ab13-7ac5ac4825ee"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IUIAutomationNative {
    [PreserveSig] int CompareElements(IntPtr first, IntPtr second, out int same);
    [PreserveSig] int CompareRuntimeIds(IntPtr first, IntPtr second, out int same);
    [PreserveSig] int GetRootElement(out IUIAutomationElementNative element);
    [PreserveSig] int ElementFromHandle(IntPtr hwnd, out IUIAutomationElementNative element);
}

[ComImport, Guid("d22108aa-8ac5-49a5-837b-37bbb3d7591e"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IUIAutomationElementNative {
    [PreserveSig] int SetFocus();
    [PreserveSig] int GetRuntimeId(out IntPtr runtimeId);
    [PreserveSig] int FindFirst(int scope, IntPtr condition, out IntPtr element);
    [PreserveSig] int FindAll(int scope, IntPtr condition, out IntPtr elements);
    [PreserveSig] int FindFirstBuildCache(int scope, IntPtr condition, IntPtr request, out IntPtr element);
    [PreserveSig] int FindAllBuildCache(int scope, IntPtr condition, IntPtr request, out IntPtr elements);
    [PreserveSig] int BuildUpdatedCache(IntPtr request, out IntPtr element);
    [PreserveSig] int GetCurrentPropertyValue(int propertyId, [MarshalAs(UnmanagedType.Struct)] out object value);
}

[ComImport, Guid("618736e0-3c3d-11cf-810c-00aa00389b71"), InterfaceType(ComInterfaceType.InterfaceIsIDispatch)]
interface IAccessibleNative {
    [PreserveSig] int GetTypeInfoCount(out uint count);
    [PreserveSig] int GetTypeInfo(uint index, int locale, out IntPtr typeInfo);
    [PreserveSig] int GetIDsOfNames(ref Guid iid, IntPtr names, uint count, int locale, IntPtr ids);
    [PreserveSig] int Invoke(int member, ref Guid iid, int locale, ushort flags, IntPtr parameters, IntPtr result, IntPtr exception, IntPtr argumentError);
    [PreserveSig] int get_accParent(out object parent);
    [PreserveSig] int get_accChildCount(out int count);
    [PreserveSig] int get_accChild([MarshalAs(UnmanagedType.Struct)] object child, out object accessibleChild);
    [PreserveSig] int get_accName([MarshalAs(UnmanagedType.Struct)] object child, [MarshalAs(UnmanagedType.BStr)] out string name);
    [PreserveSig] int get_accValue([MarshalAs(UnmanagedType.Struct)] object child, [MarshalAs(UnmanagedType.BStr)] out string value);
    [PreserveSig] int get_accDescription([MarshalAs(UnmanagedType.Struct)] object child, [MarshalAs(UnmanagedType.BStr)] out string description);
    [PreserveSig] int get_accRole([MarshalAs(UnmanagedType.Struct)] object child, [MarshalAs(UnmanagedType.Struct)] out object role);
    [PreserveSig] int get_accState([MarshalAs(UnmanagedType.Struct)] object child, [MarshalAs(UnmanagedType.Struct)] out object state);
    [PreserveSig] int get_accHelp([MarshalAs(UnmanagedType.Struct)] object child, [MarshalAs(UnmanagedType.BStr)] out string help);
    [PreserveSig] int get_accHelpTopic([MarshalAs(UnmanagedType.BStr)] out string helpFile, [MarshalAs(UnmanagedType.Struct)] object child, out int topic);
    [PreserveSig] int get_accKeyboardShortcut([MarshalAs(UnmanagedType.Struct)] object child, [MarshalAs(UnmanagedType.BStr)] out string shortcut);
    [PreserveSig] int get_accFocus([MarshalAs(UnmanagedType.Struct)] out object focus);
    [PreserveSig] int get_accSelection([MarshalAs(UnmanagedType.Struct)] out object selection);
    [PreserveSig] int get_accDefaultAction([MarshalAs(UnmanagedType.Struct)] object child, [MarshalAs(UnmanagedType.BStr)] out string action);
}

public static class SettingsUiaCrossCheck {
    [DllImport("ole32.dll")] static extern int CoInitializeEx(IntPtr reserved, int coInit);
    [DllImport("oleacc.dll")] static extern int AccessibleObjectFromWindow(IntPtr hwnd, uint objectId, ref Guid iid, [MarshalAs(UnmanagedType.Interface)] out IAccessibleNative accessible);
    static readonly Guid CUIAutomationClsid = new Guid("ff48dba4-60ef-4201-aa87-54103eef594e");
    static readonly Guid IAccessibleIid = new Guid("618736e0-3c3d-11cf-810c-00aa00389b71");
    static readonly Dictionary<int,string> PropertyNames = new Dictionary<int,string> {
        {30002,"ProcessId"},{30003,"ControlType"},{30005,"Name"},{30011,"AutomationId"},
        {30012,"ClassName"},{30020,"NativeWindowHandle"},{30024,"FrameworkId"},{30107,"ProviderDescription"}
    };
    static IUIAutomationNative CreateAutomation() {
        int hr=CoInitializeEx(IntPtr.Zero,0);
        if(hr<0 && hr!=unchecked((int)0x80010106)) Marshal.ThrowExceptionForHR(hr);
        return (IUIAutomationNative)Activator.CreateInstance(Type.GetTypeFromCLSID(CUIAutomationClsid));
    }
    public static Dictionary<string,object> QueryNativeUia(IntPtr hwnd) {
        var result=new Dictionary<string,object>();
        try {
            var automation=CreateAutomation(); IUIAutomationElementNative element;
            int hr=automation.ElementFromHandle(hwnd,out element);
            result["hresult"]=hr;
            if(hr<0 || element==null) return result;
            foreach(var property in PropertyNames) {
                object value=null; hr=element.GetCurrentPropertyValue(property.Key,out value);
                result[property.Value]=hr<0 ? null : value;
                result[property.Value+"Hresult"]=hr;
            }
            return result;
        } catch(Exception error) { result["error"]=error.ToString(); return result; }
    }
    public static Dictionary<string,object> QueryMsaa(IntPtr hwnd) {
        var result=new Dictionary<string,object>(); IAccessibleNative accessible=null;
        try {
            Guid iid=IAccessibleIid; int hr=AccessibleObjectFromWindow(hwnd,0xFFFFFFFC,ref iid,out accessible);
            result["hresult"]=hr;
            if(hr<0 || accessible==null) return result;
            object self=0; int count; hr=accessible.get_accChildCount(out count);
            result["childCountHresult"]=hr; result["childCount"]=hr<0?null:(object)count;
            string name; hr=accessible.get_accName(self,out name);
            result["nameHresult"]=hr; result["name"]=hr<0?null:name;
            object role,state; hr=accessible.get_accRole(self,out role);
            result["roleHresult"]=hr; result["role"]=hr<0||role==null?null:role.ToString();
            hr=accessible.get_accState(self,out state);
            result["stateHresult"]=hr; result["state"]=hr<0||state==null?null:state.ToString();
            string action; hr=accessible.get_accDefaultAction(self,out action);
            result["defaultActionHresult"]=hr; result["defaultAction"]=hr<0?null:action;
        } catch(Exception error) { result["error"]=error.ToString(); }
        return result;
    }
}
'@
}

$nativeRoot=Split-Path $PSScriptRoot
$testExe=if($env:ISLE_TEST_EXE){$env:ISLE_TEST_EXE}else{Join-Path $nativeRoot 'target/release/isle-native.exe'}
$testExe=(Resolve-Path -LiteralPath $testExe).Path
$binarySha=(Get-FileHash -LiteralPath $testExe -Algorithm SHA256).Hash
$artifactRoot=Join-Path $nativeRoot 'artifacts'
$runFolder=Join-Path $artifactRoot ("settings-uia-"+$binarySha.Substring(0,12))
New-Item -ItemType Directory -Force $runFolder|Out-Null
$settingsPath=Join-Path $runFolder 'fixture-settings.json'
$logPath=Join-Path $runFolder 'snapshot.json'
$resultsPath=Join-Path $runFolder 'uia-inventory-native-crosscheck.json'
$failurePath=Join-Path $runFolder 'failure-native-crosscheck.json'
$fixtureText='{"enableAnimations":false,"reduceAnimations":true,"compactLength":100,"floatingFillColor":"#102030","futureSettings":{"keep":[1,null,{"v":2}]}}'
[System.IO.File]::WriteAllText($settingsPath,$fixtureText,[System.Text.UTF8Encoding]::new($false))

if($env:ISLE_TEST_MONITOR -ne '\\.\DISPLAY2'){throw 'ISLE_TEST_MONITOR must be set to \\.\DISPLAY2 before the UI run'}
# This is an inventory-only probe. Do not inherit a create-fault switch into this child.
$savedCreateFault=$env:ISLE_TEST_CONTROL_FAIL_CREATE
Remove-Item Env:\ISLE_TEST_CONTROL_FAIL_CREATE -ErrorAction SilentlyContinue
$owned=$null
$mainHwnd=[IntPtr]::Zero
$settingsHwnd=[IntPtr]::Zero
$mainClass='IsleNativePrototype'
$settingsClass='IsleNativeSettingsV2'
$evidence=$null
try {
    $args=@('--ui-v2','--test-fixture','--page','music','--paused','--reduced-motion','--settings-path',$settingsPath,'--log',$logPath)
    $owned=Start-Process -FilePath $testExe -ArgumentList $args -PassThru -WindowStyle Hidden
    for($i=0;$i -lt 160;$i++) {
        $owned.Refresh()
        $found=[SettingsUiaNativeProbe]::FindTop($owned.Id,$mainClass)
        if($found.Count -gt 1){throw "owned PID has multiple main windows: $($found.Count)"}
        if($found.Count -eq 1){$mainHwnd=$found[0];break}
        if($owned.HasExited){throw "owned app exited before main HWND creation: $($owned.ExitCode)"}
        Start-Sleep -Milliseconds 50
    }
    if($mainHwnd -eq [IntPtr]::Zero){throw 'owned app did not create IsleNativePrototype'}
    [SettingsUiaNativeProbe]::Require($mainHwnd,$owned.Id,$mainClass)
    [SettingsUiaNativeProbe]::ShowForPaint($mainHwnd,$owned.Id,$mainClass)

    function Get-FreshDiagnostic([IntPtr]$Window,[int]$OwnerProcessId,[string]$Path,[string]$Class) {
        $before=if(Test-Path -LiteralPath $Path){(Get-Item -LiteralPath $Path).LastWriteTimeUtc.Ticks}else{0}
        for($try=0;$try -lt 60;$try++) {
            [SettingsUiaNativeProbe]::PostChecked($Window,$OwnerProcessId,$Class,0x803c,[IntPtr]::Zero,[IntPtr]::Zero)
            Start-Sleep -Milliseconds 50
            if((Test-Path -LiteralPath $Path) -and (Get-Item -LiteralPath $Path).LastWriteTimeUtc.Ticks -ne $before) {
                try {$sample=Get-Content -LiteralPath $Path -Raw|ConvertFrom-Json} catch {Start-Sleep -Milliseconds 20;continue}
                if($sample.prototype -eq $true -and $sample.renderer -and $sample.uiV2 -eq $true) {
                    if($sample.configurationValid -ne $true){throw 'UIA fixture configuration is invalid'}
                    $fields=@($sample.PSObject.Properties.Name)
                    if($fields -contains 'audioPolls' -or $fields -contains 'mediaPolls'){
                        throw 'fixture unexpectedly created live audio/media services; UIA will not proceed'
                    }
                    return $sample
                }
            }
        }
        throw 'fresh isolated UI V2 diagnostic did not arrive'
    }

    $initial=Get-FreshDiagnostic $mainHwnd $owned.Id $logPath $mainClass
    if($initial.settingsWindowAlive -eq $true){throw 'new owned host unexpectedly reports Settings alive before F8'}
    [SettingsUiaNativeProbe]::PostChecked($mainHwnd,$owned.Id,$mainClass,0x0100,[IntPtr]0x77,[IntPtr]::Zero)
    for($i=0;$i -lt 100;$i++) {
        $found=[SettingsUiaNativeProbe]::FindTop($owned.Id,$settingsClass)
        if($found.Count -gt 1){throw "owned PID has multiple Settings HWNDs: $($found.Count)"}
        if($found.Count -eq 1){$settingsHwnd=$found[0];break}
        Start-Sleep -Milliseconds 50
    }
    if($settingsHwnd -eq [IntPtr]::Zero){throw 'F8 did not create an owned IsleNativeSettingsV2 window'}
    [SettingsUiaNativeProbe]::Require($settingsHwnd,$owned.Id,$settingsClass)
    $opened=Get-FreshDiagnostic $mainHwnd $owned.Id $logPath $mainClass
    if($opened.settingsWindowAlive -ne $true){throw 'owned V2 Settings window is not alive in the fresh report'}

    $root=[System.Windows.Automation.AutomationElement]::FromHandle($settingsHwnd)
    if($root.Current.ProcessId -ne $owned.Id -or $root.Current.NativeWindowHandle -ne $settingsHwnd.ToInt32()){
        throw "UIA root does not map to the exact Settings HWND/PID (uiaPid=$($root.Current.ProcessId), hwnd=$($root.Current.NativeWindowHandle))"
    }
    $elements=$root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
    $patterns=@(
        @{name='Invoke';pattern=[System.Windows.Automation.InvokePattern]::Pattern},
        @{name='Toggle';pattern=[System.Windows.Automation.TogglePattern]::Pattern},
        @{name='Selection';pattern=[System.Windows.Automation.SelectionPattern]::Pattern},
        @{name='SelectionItem';pattern=[System.Windows.Automation.SelectionItemPattern]::Pattern},
        @{name='RangeValue';pattern=[System.Windows.Automation.RangeValuePattern]::Pattern},
        @{name='Value';pattern=[System.Windows.Automation.ValuePattern]::Pattern},
        @{name='ExpandCollapse';pattern=[System.Windows.Automation.ExpandCollapsePattern]::Pattern}
    )
    $inventory=@()
    foreach($element in $elements) {
        $current=$element.Current
        if($current.ProcessId -ne $owned.Id){throw "UIA descendant escaped owned PID: $($current.ProcessId)"}
        $nativeUia=if($current.NativeWindowHandle -ne 0){[SettingsUiaCrossCheck]::QueryNativeUia([IntPtr]$current.NativeWindowHandle)}else{$null}
        $msaa=if($current.NativeWindowHandle -ne 0){[SettingsUiaCrossCheck]::QueryMsaa([IntPtr]$current.NativeWindowHandle)}else{$null}
        $available=@()
        foreach($item in $patterns) {
            $patternObject=$null
            if($element.TryGetCurrentPattern($item.pattern,[ref]$patternObject)){$available+=,$item.name}
        }
        $rect=$current.BoundingRectangle
        $inventory+=,[ordered]@{
            name=$current.Name;automationId=$current.AutomationId;controlType=$current.ControlType.ProgrammaticName
            className=$current.ClassName;frameworkId=$current.FrameworkId
            providerDescription=if($nativeUia){$nativeUia.ProviderDescription}else{$null}
            nativeCUIAutomation=$nativeUia;msaa=$msaa
            processId=$current.ProcessId;nativeWindowHandle=$current.NativeWindowHandle
            isEnabled=$current.IsEnabled;isOffscreen=$current.IsOffscreen
            bounds=[ordered]@{x=$rect.X;y=$rect.Y;width=$rect.Width;height=$rect.Height}
            patterns=$available
        }
    }
    if($elements.Count -lt 6){throw "Settings UIA subtree is unexpectedly small: $($elements.Count) descendants"}
    $final=Get-FreshDiagnostic $mainHwnd $owned.Id $logPath $mainClass
    $evidence=[ordered]@{
        binarySha256=$binarySha;ownedPid=$owned.Id;mainHwnd=$mainHwnd.ToInt64();settingsHwnd=$settingsHwnd.ToInt64()
        settingsClass=$settingsClass;readOnlyProbe=$true;readOnlyParameterRequested=[bool]$ReadOnlyProbe
        clientProxyRegistration=$clientProxyRegistration
        host=$hostMetadata
        initialConfigurationValid=$initial.configurationValid;finalConfigurationValid=$final.configurationValid
        serviceFieldsAbsent=$true;initialSettingsAlive=$initial.settingsWindowAlive;finalSettingsAlive=$final.settingsWindowAlive
        uiaRootProcessId=$root.Current.ProcessId;uiaRootNativeWindowHandle=$root.Current.NativeWindowHandle
        descendantCount=$elements.Count;inventory=$inventory;screenshotsVerified=$false
        nativeCUIAutomationSameProcessAsManagedClient=$true
        note='Exploratory inventory only: official client-side assembly registration was attempted. The same-process CUIAutomation query is not an independent proxy validation because UIAutomationClient/Types are loaded in this host. MSAA query status is recorded per HWND; no UIA action or value setter was called.'
    }
} catch {
    $evidence=[ordered]@{binarySha256=$binarySha;ownedPid=if($owned){$owned.Id}else{$null};mainHwnd=if($mainHwnd -ne [IntPtr]::Zero){$mainHwnd.ToInt64()}else{$null};settingsHwnd=if($settingsHwnd -ne [IntPtr]::Zero){$settingsHwnd.ToInt64()}else{$null};failure=$_.Exception.Message;forcedKill=$false}
    throw
} finally {
    if($null -ne $owned) {
        $owned.Refresh()
        $closePosted=$false
        if(!$owned.HasExited) {
            if($settingsHwnd -ne [IntPtr]::Zero -and [SettingsUiaNativeProbe]::FindTop($owned.Id,$settingsClass).Count -eq 1){
                [SettingsUiaNativeProbe]::PostChecked($settingsHwnd,$owned.Id,$settingsClass,0x0010,[IntPtr]::Zero,[IntPtr]::Zero)
            }
            if($mainHwnd -ne [IntPtr]::Zero) {
                [SettingsUiaNativeProbe]::PostChecked($mainHwnd,$owned.Id,$mainClass,0x0010,[IntPtr]::Zero,[IntPtr]::Zero)
                $closePosted=$true
            }
            $clock=[System.Diagnostics.Stopwatch]::StartNew()
            if(!$owned.WaitForExit(5000)) {
                $clock.Stop()
                $timeout=[ordered]@{binarySha256=$binarySha;ownedPid=$owned.Id;mainHwnd=$mainHwnd.ToInt64();settingsHwnd=$settingsHwnd.ToInt64();gracefulClosePosted=$closePosted;shutdownWaitMs=5000;forcedKill=$false}
                $timeout|ConvertTo-Json|Set-Content -Encoding UTF8 (Join-Path $runFolder 'timeout-native-crosscheck.json')
                throw 'Owned UIA probe failed to exit within 5000 ms after WM_CLOSE; no forced kill was issued'
            }
            $clock.Stop()
            if($null -ne $evidence){$evidence['shutdownWaitMs']=$clock.ElapsedMilliseconds}
        }
        $owned.Refresh()
        if(!$owned.HasExited){throw 'Owned UIA probe still exists after WM_CLOSE'}
        $exitCode=$owned.ExitCode
        if($null -ne $evidence){$evidence['ownedExitCode']=$exitCode;$evidence['gracefulClosePosted']=$closePosted;$evidence['forcedKill']=$false}
        if($exitCode -ne 0){throw "Owned UIA probe exited with code $exitCode"}
    }
    if($null -ne $evidence) {
        $path=if($evidence.Contains('failure')){$failurePath}else{$resultsPath}
        [System.IO.File]::WriteAllText($path,($evidence|ConvertTo-Json -Depth 10),[System.Text.UTF8Encoding]::new($false))
    }
    if($null -ne $savedCreateFault){$env:ISLE_TEST_CONTROL_FAIL_CREATE=$savedCreateFault}
}

if($null -ne $evidence -and !$evidence.Contains('failure')) { Write-Output "PASS: read-only UIA inventory from owned Settings HWND; $($evidence.descendantCount) descendants" }
