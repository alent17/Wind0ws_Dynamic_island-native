param([switch]$ReadOnlyProbe,[switch]$InteractionProbe)
$ErrorActionPreference='Stop'
if($ReadOnlyProbe -and $InteractionProbe){throw 'Choose either -ReadOnlyProbe or -InteractionProbe, not both'}
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$hostMetadata=[ordered]@{
    powershellVersion=$PSVersionTable.PSVersion.ToString()
    powershellEdition=$PSVersionTable.PSEdition
    frameworkDescription=[Runtime.InteropServices.RuntimeInformation]::FrameworkDescription
    uiAutomationClientAssembly=([AppDomain]::CurrentDomain.GetAssemblies()|Where-Object{$_.GetName().Name -eq 'UIAutomationClient'}|Select-Object -First 1).Location
    uiAutomationTypesAssembly=([AppDomain]::CurrentDomain.GetAssemblies()|Where-Object{$_.GetName().Name -eq 'UIAutomationTypes'}|Select-Object -First 1).Location
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
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] static extern bool AttachThreadInput(uint sourceThread,uint targetThread,bool attach);
    [DllImport("user32.dll")] static extern IntPtr SetFocus(IntPtr hwnd);
    [DllImport("user32.dll")] static extern IntPtr GetFocus();
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] struct Rect { public int left,top,right,bottom; }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] struct MonitorInfo {
        public int size; public Rect monitor; public Rect work; public uint flags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string device;
    }
    [StructLayout(LayoutKind.Sequential)] struct ComboBoxInfo {
        public int size; public Rect item; public Rect button; public uint state;
        public IntPtr combo; public IntPtr itemWindow; public IntPtr list;
    }
    [StructLayout(LayoutKind.Sequential)] struct Point { public int x,y; }
    [StructLayout(LayoutKind.Sequential)] struct GuiThreadInfo {
        public int size; public uint flags; public IntPtr active,focus,capture,menuOwner,moveSize,caret; public Rect caretRect;
    }
    [DllImport("user32.dll", SetLastError=true)] static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] static extern IntPtr MonitorFromPoint(Point point, uint flags);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern bool GetMonitorInfoW(IntPtr monitor, ref MonitorInfo info);
    [DllImport("user32.dll", SetLastError=true)] static extern bool GetComboBoxInfo(IntPtr combo, ref ComboBoxInfo info);
    [DllImport("user32.dll", SetLastError=true)] static extern bool GetGUIThreadInfo(uint threadId, ref GuiThreadInfo info);
    [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr hwnd, int command);
    [DllImport("user32.dll", SetLastError=true)] static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll", EntryPoint="SendMessageW")] static extern IntPtr SendMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
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
    public static IntPtr ComboList(IntPtr combo) {
        var info=new ComboBoxInfo(); info.size=Marshal.SizeOf(typeof(ComboBoxInfo));
        if(!GetComboBoxInfo(combo,ref info)) throw new Win32Exception(Marshal.GetLastWin32Error(),"GetComboBoxInfo failed for owned ComboBox "+combo);
        if(info.list==IntPtr.Zero) throw new InvalidOperationException("GetComboBoxInfo returned no list HWND for owned ComboBox "+combo);
        return info.list;
    }
    public static int ComboCount(IntPtr combo) { return SendMessage(combo,326,IntPtr.Zero,IntPtr.Zero).ToInt32(); }
    public static int OwnerPid(IntPtr hwnd) { uint owner; GetWindowThreadProcessId(hwnd,out owner); return (int)owner; }
    public static IntPtr FocusedOwnedControl(IntPtr topWindow,int pid,string expectedClass) {
        Require(topWindow,pid,expectedClass);
        uint ownerPid; uint targetThread=GetWindowThreadProcessId(topWindow,out ownerPid);
        var info=new GuiThreadInfo();info.size=Marshal.SizeOf(typeof(GuiThreadInfo));
        if(targetThread==0 || !GetGUIThreadInfo(targetThread,ref info)) throw new Win32Exception(Marshal.GetLastWin32Error(),"Could not read focus from the owned Settings thread");
        if(info.focus==IntPtr.Zero || OwnerPid(info.focus)!=pid) throw new InvalidOperationException("Settings focus is not inside the owned fixture process");
        return info.focus;
    }
    public static IntPtr FocusOwnedControl(IntPtr hwnd,int pid,IntPtr topWindow,string expectedClass) {
        if(hwnd==IntPtr.Zero || !IsWindow(hwnd) || OwnerPid(hwnd)!=pid)
            throw new InvalidOperationException("Focus target is not a live HWND owned by the fixture process");
        Require(topWindow,pid,expectedClass);
        uint ownerPid; uint targetThread=GetWindowThreadProcessId(hwnd,out ownerPid); uint currentThread=GetCurrentThreadId();
        if(ownerPid!=(uint)pid || targetThread==0) throw new InvalidOperationException("Focus target thread does not belong to the fixture process");
        bool attached=AttachThreadInput(currentThread,targetThread,true);
        if(!attached) throw new Win32Exception(Marshal.GetLastWin32Error(),"Could not attach the isolated UIA input queues");
        try {
            SetFocus(hwnd);
            if(GetFocus()!=hwnd) throw new InvalidOperationException("The owned Settings control did not accept keyboard focus");
        } finally { AttachThreadInput(currentThread,targetThread,false); }
        return FocusedOwnedControl(topWindow,pid,expectedClass);
    }
    public static void PostOwnedKey(IntPtr hwnd,int pid,int virtualKey,bool keyUp) {
        if(hwnd==IntPtr.Zero || !IsWindow(hwnd) || OwnerPid(hwnd)!=pid)
            throw new InvalidOperationException("Keyboard target is not a live HWND owned by the fixture process");
        uint message=keyUp?0x0101u:0x0100u;
        if(!PostMessage(hwnd,message,(IntPtr)virtualKey,IntPtr.Zero))
            throw new Win32Exception(Marshal.GetLastWin32Error(),"Could not post a key to the owned fixture control");
    }
    public static string MonitorDevice(IntPtr hwnd) {
        Rect rect; if(!GetWindowRect(hwnd,out rect)) throw new Win32Exception(Marshal.GetLastWin32Error(),"GetWindowRect failed");
        Point center=new Point{x=rect.left+(rect.right-rect.left)/2,y=rect.top+(rect.bottom-rect.top)/2};
        IntPtr monitor=MonitorFromPoint(center,2); if(monitor==IntPtr.Zero) throw new Win32Exception("MonitorFromPoint returned no monitor for window center");
        var info=new MonitorInfo(); info.size=Marshal.SizeOf(typeof(MonitorInfo));
        if(!GetMonitorInfoW(monitor,ref info)) throw new Win32Exception(Marshal.GetLastWin32Error(),"GetMonitorInfoW failed");
        return info.device;
    }
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

[ComImport, Guid("618736e0-3c3d-11cf-810c-00aa00389b71"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
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
Add-Type -AssemblyName System.Windows.Forms
$availableDisplays=@([System.Windows.Forms.Screen]::AllScreens|ForEach-Object{$_.DeviceName})
if($env:ISLE_TEST_MONITOR -notin $availableDisplays){
    throw "Required test display $($env:ISLE_TEST_MONITOR) is not connected/enabled; detected: $($availableDisplays -join ', ')"
}
# This is an inventory-only probe. Do not inherit a create-fault switch into this child.
$savedCreateFault=$env:ISLE_TEST_CONTROL_FAIL_CREATE
Remove-Item Env:\ISLE_TEST_CONTROL_FAIL_CREATE -ErrorAction SilentlyContinue
$owned=$null
$mainHwnd=[IntPtr]::Zero
$settingsHwnd=[IntPtr]::Zero
$mainClass='IsleNativePrototype'
$settingsClass='IsleNativeSettingsV2'
$evidence=$null
$probeFailure=$null
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
    $mainMonitor=[SettingsUiaNativeProbe]::MonitorDevice($mainHwnd)
    if($mainMonitor -ne $env:ISLE_TEST_MONITOR){throw "Main test window is on $mainMonitor, expected $($env:ISLE_TEST_MONITOR)"}

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
    $settingsMonitor=[SettingsUiaNativeProbe]::MonitorDevice($settingsHwnd)
    if($settingsMonitor -ne $env:ISLE_TEST_MONITOR){throw "Settings test window is on $settingsMonitor, expected $($env:ISLE_TEST_MONITOR)"}
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
    $interactionEvidence=@()
    if($InteractionProbe) {
        function Find-SettingsElement([string]$AutomationId) {
            $currentRoot=[System.Windows.Automation.AutomationElement]::FromHandle($settingsHwnd)
            $descendants=$currentRoot.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
            foreach($candidate in $descendants) {
                if($candidate.Current.ProcessId -ne $owned.Id){throw 'UIA action target escaped owned fixture process'}
                if($candidate.Current.AutomationId -eq $AutomationId){return $candidate}
            }
            return $null
        }
        function Wait-SettingsElement([string]$AutomationId) {
            for($attempt=0;$attempt -lt 80;$attempt++) {
                $candidate=Find-SettingsElement $AutomationId
                if($null -ne $candidate){return $candidate}
                Start-Sleep -Milliseconds 50
            }
            throw "Settings UIA element $AutomationId did not appear after navigation"
        }
        function Get-SettingsPattern($Element,$Pattern) {
            $instance=$null
            if(-not $Element.TryGetCurrentPattern($Pattern,[ref]$instance) -or $null -eq $instance){
                throw "Required UIA pattern is not available for $($Element.Current.AutomationId)"
            }
            return $instance
        }

        $appearance=Wait-SettingsElement '301'
        (Get-SettingsPattern $appearance ([System.Windows.Automation.InvokePattern]::Pattern)).Invoke()
        [void](Wait-SettingsElement '211')
        $interactionEvidence+=,[ordered]@{action='Invoke';automationId='301';result='navigated to Appearance'}

        $sliderElement=Wait-SettingsElement '220'
        $slider=Get-SettingsPattern $sliderElement ([System.Windows.Automation.RangeValuePattern]::Pattern)
        $initialValue=$slider.Current.Value
        $minimum=$slider.Current.Minimum;$maximum=$slider.Current.Maximum
        $changedValue=if($initialValue -lt $maximum){$initialValue+1}else{$initialValue-1}
        if($changedValue -lt $minimum -or $changedValue -gt $maximum){throw 'Fixture slider has no reversible in-range value'}
        $slider.SetValue($changedValue)
        Start-Sleep -Milliseconds 100
        $updatedSlider=Get-SettingsPattern (Wait-SettingsElement '220') ([System.Windows.Automation.RangeValuePattern]::Pattern)
        if($updatedSlider.Current.Value -ne $changedValue){throw 'UIA RangeValue.SetValue did not update the native slider'}
        $updatedSlider.SetValue($initialValue)
        Start-Sleep -Milliseconds 100
        $restoredSlider=Get-SettingsPattern (Wait-SettingsElement '220') ([System.Windows.Automation.RangeValuePattern]::Pattern)
        if($restoredSlider.Current.Value -ne $initialValue){throw 'UIA RangeValue.SetValue did not restore the fixture slider'}
        $interactionEvidence+=,[ordered]@{action='RangeValue';automationId='220';initial=$initialValue;changed=$changedValue;restored=$restoredSlider.Current.Value}

        $rootElement=[System.Windows.Automation.AutomationElement]::FromHandle($settingsHwnd)
        $selection=Get-SettingsPattern $rootElement ([System.Windows.Automation.SelectionPattern]::Pattern)
        if($selection.Current.CanSelectMultiple -or -not $selection.Current.IsSelectionRequired){throw 'Settings radio selection must be single-select and required'}
        $selectedItems=@($selection.Current.GetSelection())
        if($selectedItems.Count -ne 1){throw "Expected exactly one selected spectrum radio, got $($selectedItems.Count)"}
        $initialRadioId=$selectedItems[0].Current.AutomationId
        if($initialRadioId -notin @('252','253')){throw "Unexpected selected spectrum radio AutomationId $initialRadioId"}
        [string[]]$radioIds=@('252','253')|Where-Object{$_ -ne $initialRadioId}
        $alternateRadio=Wait-SettingsElement $radioIds[0]
        $alternateSelectionItem=Get-SettingsPattern $alternateRadio ([System.Windows.Automation.SelectionItemPattern]::Pattern)
        if($alternateSelectionItem.Current.IsSelected){throw 'Alternate radio unexpectedly reports selected before selection'}
        if($alternateSelectionItem.Current.SelectionContainer.Current.NativeWindowHandle -ne $settingsHwnd.ToInt32()){
            throw 'Radio SelectionContainer does not resolve to the Settings root HWND'
        }
        $alternateSelectionItem.Select()
        Start-Sleep -Milliseconds 150
        $alternateSelected=@((Get-SettingsPattern ([System.Windows.Automation.AutomationElement]::FromHandle($settingsHwnd)) ([System.Windows.Automation.SelectionPattern]::Pattern)).Current.GetSelection())
        if($alternateSelected.Count -ne 1 -or $alternateSelected[0].Current.AutomationId -ne $radioIds[0]){throw 'Selection.Select did not update Settings selection'}
        if(-not (Get-SettingsPattern (Wait-SettingsElement $radioIds[0]) ([System.Windows.Automation.SelectionItemPattern]::Pattern)).Current.IsSelected){throw 'SelectionItem.IsSelected did not become true after selection'}
        $removeRejected=$false
        try{(Get-SettingsPattern (Wait-SettingsElement $radioIds[0]) ([System.Windows.Automation.SelectionItemPattern]::Pattern)).RemoveFromSelection()}catch{$removeRejected=$true}
        if(-not $removeRejected){throw 'Required single-selection radio accepted RemoveFromSelection'}
        (Get-SettingsPattern (Wait-SettingsElement $initialRadioId) ([System.Windows.Automation.SelectionItemPattern]::Pattern)).Select()
        Start-Sleep -Milliseconds 150
        $restoredSelection=@((Get-SettingsPattern ([System.Windows.Automation.AutomationElement]::FromHandle($settingsHwnd)) ([System.Windows.Automation.SelectionPattern]::Pattern)).Current.GetSelection())
        if($restoredSelection.Count -ne 1 -or $restoredSelection[0].Current.AutomationId -ne $initialRadioId){throw 'Selection.Select did not restore the initial Settings radio'}
        $interactionEvidence+=,[ordered]@{action='Selection/SelectionItem';initial=$initialRadioId;changed=$radioIds[0];restored=$restoredSelection[0].Current.AutomationId;removeRejected=$removeRejected;containerHwnd=$settingsHwnd.ToInt64()}

        $general=Wait-SettingsElement '300'
        (Get-SettingsPattern $general ([System.Windows.Automation.InvokePattern]::Pattern)).Invoke()
        $toggleElement=Wait-SettingsElement '208'
        $toggle=Get-SettingsPattern $toggleElement ([System.Windows.Automation.TogglePattern]::Pattern)
        $initialToggle=$toggle.Current.ToggleState
        $toggle.Toggle()
        Start-Sleep -Milliseconds 100
        $toggled=(Get-SettingsPattern (Wait-SettingsElement '208') ([System.Windows.Automation.TogglePattern]::Pattern)).Current.ToggleState
        if($toggled -eq $initialToggle){throw 'UIA Toggle did not change the checkbox state'}
        $toggle=Get-SettingsPattern (Wait-SettingsElement '208') ([System.Windows.Automation.TogglePattern]::Pattern)
        $toggle.Toggle()
        Start-Sleep -Milliseconds 100
        $restored=(Get-SettingsPattern (Wait-SettingsElement '208') ([System.Windows.Automation.TogglePattern]::Pattern)).Current.ToggleState
        if($restored -ne $initialToggle){throw 'UIA Toggle did not restore the original fixture state'}
        $interactionEvidence+=,[ordered]@{action='Toggle';automationId='208';initial=[string]$initialToggle;toggled=[string]$toggled;restored=[string]$restored}

        $comboElement=Wait-SettingsElement '210'
        $combo=Get-SettingsPattern $comboElement ([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
        if($combo.Current.ExpandCollapseState -ne [System.Windows.Automation.ExpandCollapseState]::Collapsed){throw 'Fixture timezone ComboBox was not collapsed before the UIA action'}
        $combo.Expand()
        Start-Sleep -Milliseconds 100
        $expanded=Get-SettingsPattern (Wait-SettingsElement '210') ([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
        if($expanded.Current.ExpandCollapseState -ne [System.Windows.Automation.ExpandCollapseState]::Expanded){throw 'UIA Expand did not open the timezone ComboBox'}
        $comboElement=Wait-SettingsElement '210'
        $comboSelection=Get-SettingsPattern $comboElement ([System.Windows.Automation.SelectionPattern]::Pattern)
        if($comboSelection.Current.CanSelectMultiple -or -not $comboSelection.Current.IsSelectionRequired){throw 'ComboBox must expose required single-selection semantics'}
        $initialComboSelection=@($comboSelection.Current.GetSelection())
        if($initialComboSelection.Count -ne 1){throw "Expected one selected timezone item, got $($initialComboSelection.Count)"}
        $initialComboItem=$initialComboSelection[0]
        $initialComboItemId=$initialComboItem.Current.AutomationId
        if($initialComboItemId -notmatch '^210:\d+$'){throw "Unexpected ComboBox item AutomationId $initialComboItemId"}
        $comboHwnd=[IntPtr]$comboElement.Current.NativeWindowHandle
        $comboListHwnd=[SettingsUiaNativeProbe]::ComboList($comboHwnd)
        $comboDescendants=$comboElement.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
        $comboPopupRoot=[System.Windows.Automation.AutomationElement]::FromHandle($comboListHwnd)
        $comboPopupChildren=[System.Collections.Generic.List[System.Windows.Automation.AutomationElement]]::new()
        $popupWalker=[System.Windows.Automation.TreeWalker]::RawViewWalker
        $nativeComboCount=[SettingsUiaNativeProbe]::ComboCount($comboHwnd)
        if($nativeComboCount -lt 2 -or $nativeComboCount -gt 2048){throw "Timezone ComboBox native count is out of range: $nativeComboCount"}
        $popupChild=$null
        for($popupIndex=0;$popupIndex -lt $nativeComboCount;$popupIndex++){
            $popupChild=if($popupIndex -eq 0){$popupWalker.GetFirstChild($comboPopupRoot)}else{$popupWalker.GetNextSibling($popupChild)}
            if($null -eq $popupChild){throw "ComboLBox UIA fragment ended at index $popupIndex before native count $nativeComboCount"}
            $comboPopupChildren.Add($popupChild)
        }
        $comboPopupItems=@(foreach($candidate in $comboPopupChildren){
            if($candidate.Current.ControlType -eq [System.Windows.Automation.ControlType]::ListItem){
                $itemPattern=$null
                $hasSelectionItem=$candidate.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern,[ref]$itemPattern)
                [ordered]@{name=$candidate.Current.Name;automationId=$candidate.Current.AutomationId;nativeWindowHandle=$candidate.Current.NativeWindowHandle;hasSelectionItem=$hasSelectionItem}
            }
        })
        if($comboPopupItems.Count -lt 2){throw "ComboLBox UIA fragment exposed $($comboPopupItems.Count) timezone items; native CB_GETCOUNT=$([SettingsUiaNativeProbe]::ComboCount($comboHwnd)); list class=$([SettingsUiaNativeProbe]::Class($comboListHwnd)); root type=$($comboPopupRoot.Current.ControlType.ProgrammaticName); first item=$($comboPopupItems|ConvertTo-Json -Compress)"}
        $alternateComboItem=$comboPopupChildren|Where-Object{$_.Current.ControlType -eq [System.Windows.Automation.ControlType]::ListItem -and $_.Current.AutomationId -ne $initialComboItemId}|Select-Object -First 1
        if($null -eq $alternateComboItem){throw 'ComboLBox UIA fragment did not expose an alternate timezone item'}
        $alternateComboItemPattern=Get-SettingsPattern $alternateComboItem ([System.Windows.Automation.SelectionItemPattern]::Pattern)
        if($alternateComboItemPattern.Current.SelectionContainer.Current.NativeWindowHandle -ne $comboHwnd.ToInt32()){
            throw 'ComboBox item SelectionContainer does not resolve to the owning ComboBox HWND'
        }
        $alternateComboItemPattern.Select()
        Start-Sleep -Milliseconds 120
        $changedComboSelection=@((Get-SettingsPattern (Wait-SettingsElement '210') ([System.Windows.Automation.SelectionPattern]::Pattern)).Current.GetSelection())
        if($changedComboSelection.Count -ne 1 -or $changedComboSelection[0].Current.AutomationId -ne $alternateComboItem.Current.AutomationId){throw 'SelectionItem.Select did not change the ComboBox selection'}
        $comboRemoveRejected=$false
        try{$alternateComboItemPattern.RemoveFromSelection()}catch{$comboRemoveRejected=$true}
        if(-not $comboRemoveRejected){throw 'Required ComboBox accepted RemoveFromSelection'}
        (Get-SettingsPattern $initialComboItem ([System.Windows.Automation.SelectionItemPattern]::Pattern)).Select()
        Start-Sleep -Milliseconds 120
        $restoredComboSelection=@((Get-SettingsPattern (Wait-SettingsElement '210') ([System.Windows.Automation.SelectionPattern]::Pattern)).Current.GetSelection())
        if($restoredComboSelection.Count -ne 1 -or $restoredComboSelection[0].Current.AutomationId -ne $initialComboItemId){throw 'SelectionItem.Select did not restore the original ComboBox selection'}
        $expanded.Collapse()
        Start-Sleep -Milliseconds 100
        $collapsed=Get-SettingsPattern (Wait-SettingsElement '210') ([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
        if($collapsed.Current.ExpandCollapseState -ne [System.Windows.Automation.ExpandCollapseState]::Collapsed){throw 'UIA Collapse did not close the timezone ComboBox'}
        $interactionEvidence+=,[ordered]@{action='Selection/SelectionItem ComboBox';automationId='210';initial=$initialComboItemId;changed=$alternateComboItem.Current.AutomationId;restored=$restoredComboSelection[0].Current.AutomationId;itemCount=$comboPopupItems.Count;removeRejected=$comboRemoveRejected;listHwnd=$comboListHwnd.ToInt64()}
        $interactionEvidence+=,[ordered]@{action='ExpandCollapse';automationId='210';expanded='Expanded';restored='Collapsed';popupListItems=$comboPopupItems;comboTreeDescendantCount=$comboDescendants.Count;popupHwnd=$comboListHwnd.ToInt64()}

        $weather=Wait-SettingsElement '304'
        (Get-SettingsPattern $weather ([System.Windows.Automation.InvokePattern]::Pattern)).Invoke()
        $queryElement=Wait-SettingsElement '101'
        $valuePattern=Get-SettingsPattern $queryElement ([System.Windows.Automation.ValuePattern]::Pattern)
        $initialQuery=$valuePattern.Current.Value
        $valuePattern.SetValue('UIA fixture')
        Start-Sleep -Milliseconds 100
        $updatedQuery=(Get-SettingsPattern (Wait-SettingsElement '101') ([System.Windows.Automation.ValuePattern]::Pattern)).Current.Value
        if($updatedQuery -ne 'UIA fixture'){throw 'UIA Value.SetValue did not update the native Edit control'}
        (Get-SettingsPattern (Wait-SettingsElement '101') ([System.Windows.Automation.ValuePattern]::Pattern)).SetValue($initialQuery)
        Start-Sleep -Milliseconds 100
        $restoredQuery=(Get-SettingsPattern (Wait-SettingsElement '101') ([System.Windows.Automation.ValuePattern]::Pattern)).Current.Value
        if($restoredQuery -ne $initialQuery){throw 'UIA Value.SetValue did not restore the fixture Edit control'}
        $interactionEvidence+=,[ordered]@{action='Value';automationId='101';changed='UIA fixture';restored=$restoredQuery}

        function Send-OwnedKey([string]$Keys,[IntPtr]$Target) {
            if($null -eq $Target -or $Target.ToInt64() -eq 0){throw "Refusing $Keys because the focused native HWND is missing"}
            if([SettingsUiaNativeProbe]::OwnerPid($Target) -ne $owned.Id){throw "Refusing $Keys because target HWND is not owned by the fixture"}
            switch($Keys){
                '{TAB}' { [SettingsUiaNativeProbe]::PostOwnedKey($Target,$owned.Id,0x09,$false);[SettingsUiaNativeProbe]::PostOwnedKey($Target,$owned.Id,0x09,$true) }
                '{SPACE}' { [SettingsUiaNativeProbe]::PostOwnedKey($Target,$owned.Id,0x20,$false);[SettingsUiaNativeProbe]::PostOwnedKey($Target,$owned.Id,0x20,$true) }
                '{F4}' { [SettingsUiaNativeProbe]::PostOwnedKey($Target,$owned.Id,0x73,$false);[SettingsUiaNativeProbe]::PostOwnedKey($Target,$owned.Id,0x73,$true) }
                '{ESC}' { [SettingsUiaNativeProbe]::PostOwnedKey($Target,$owned.Id,0x1B,$false);[SettingsUiaNativeProbe]::PostOwnedKey($Target,$owned.Id,0x1B,$true) }
                '{ENTER}' { [SettingsUiaNativeProbe]::PostOwnedKey($Target,$owned.Id,0x0D,$false);[SettingsUiaNativeProbe]::PostOwnedKey($Target,$owned.Id,0x0D,$true) }
                '{RIGHT}' { [SettingsUiaNativeProbe]::PostOwnedKey($Target,$owned.Id,0x27,$false);[SettingsUiaNativeProbe]::PostOwnedKey($Target,$owned.Id,0x27,$true) }
                '{LEFT}' { [SettingsUiaNativeProbe]::PostOwnedKey($Target,$owned.Id,0x25,$false);[SettingsUiaNativeProbe]::PostOwnedKey($Target,$owned.Id,0x25,$true) }
                default { throw "Unsupported isolated keyboard test key: $Keys" }
            }
            Start-Sleep -Milliseconds 100
            $afterHwnd=[SettingsUiaNativeProbe]::FocusedOwnedControl($settingsHwnd,$owned.Id,$settingsClass)
            $afterKey=[System.Windows.Automation.AutomationElement]::FromHandle($afterHwnd)
            if($afterKey.Current.ProcessId -ne $owned.Id){throw 'Keyboard focus escaped the owned Settings fixture process'}
            return $afterKey
        }

        $generalNav=Wait-SettingsElement '300'
        (Get-SettingsPattern $generalNav ([System.Windows.Automation.InvokePattern]::Pattern)).Invoke()
        $keyboardToggle=Wait-SettingsElement '208'
        $keyboardToggleHwnd=[IntPtr]$keyboardToggle.Current.NativeWindowHandle
        $focusBeforeHwnd=[SettingsUiaNativeProbe]::FocusOwnedControl($keyboardToggleHwnd,$owned.Id,$settingsHwnd,$settingsClass)
        Start-Sleep -Milliseconds 80
        $focusBefore=[System.Windows.Automation.AutomationElement]::FromHandle($focusBeforeHwnd)
        if($focusBefore.Current.ProcessId -ne $owned.Id -or $focusBefore.Current.AutomationId -ne '208'){throw 'Could not focus the owned General checkbox'}
        $tabFocus=Send-OwnedKey -Keys '{TAB}' -Target $focusBeforeHwnd
        if($tabFocus.Current.AutomationId -eq '208'){throw 'Tab did not advance Settings keyboard focus'}
        $interactionEvidence+=,[ordered]@{action='Keyboard Tab';from='208';to=$tabFocus.Current.AutomationId}

        $keyboardToggleHwnd=[SettingsUiaNativeProbe]::FocusOwnedControl($keyboardToggleHwnd,$owned.Id,$settingsHwnd,$settingsClass)
        Start-Sleep -Milliseconds 80
        $toggleBefore=(Get-SettingsPattern $keyboardToggle ([System.Windows.Automation.TogglePattern]::Pattern)).Current.ToggleState
        [void](Send-OwnedKey -Keys '{SPACE}' -Target $keyboardToggleHwnd)
        $toggleAfter=(Get-SettingsPattern (Wait-SettingsElement '208') ([System.Windows.Automation.TogglePattern]::Pattern)).Current.ToggleState
        if($toggleAfter -eq $toggleBefore){throw 'Space did not toggle the focused Settings checkbox'}
        [void](Send-OwnedKey -Keys '{SPACE}' -Target $keyboardToggleHwnd)
        $toggleRestored=(Get-SettingsPattern (Wait-SettingsElement '208') ([System.Windows.Automation.TogglePattern]::Pattern)).Current.ToggleState
        if($toggleRestored -ne $toggleBefore){throw 'Space did not restore the original Settings checkbox state'}
        $interactionEvidence+=,[ordered]@{action='Keyboard Space';automationId='208';initial=[string]$toggleBefore;changed=[string]$toggleAfter;restored=[string]$toggleRestored}

        $keyboardCombo=Wait-SettingsElement '210'
        $keyboardComboHwnd=[SettingsUiaNativeProbe]::FocusOwnedControl([IntPtr]$keyboardCombo.Current.NativeWindowHandle,$owned.Id,$settingsHwnd,$settingsClass)
        Start-Sleep -Milliseconds 80
        [void](Send-OwnedKey -Keys '{F4}' -Target $keyboardComboHwnd)
        $comboKeyboardExpanded=(Get-SettingsPattern (Wait-SettingsElement '210') ([System.Windows.Automation.ExpandCollapsePattern]::Pattern)).Current.ExpandCollapseState
        if($comboKeyboardExpanded -ne [System.Windows.Automation.ExpandCollapseState]::Expanded){throw 'F4 did not expand the focused Settings ComboBox'}
        [void](Send-OwnedKey -Keys '{ESC}' -Target $keyboardComboHwnd)
        $comboKeyboardRestored=(Get-SettingsPattern (Wait-SettingsElement '210') ([System.Windows.Automation.ExpandCollapsePattern]::Pattern)).Current.ExpandCollapseState
        if($comboKeyboardRestored -ne [System.Windows.Automation.ExpandCollapseState]::Collapsed){throw 'Escape did not close the Settings ComboBox'}
        $interactionEvidence+=,[ordered]@{action='Keyboard F4/Escape';automationId='210';expanded=[string]$comboKeyboardExpanded;restored=[string]$comboKeyboardRestored}

        $appearanceNav=Wait-SettingsElement '301'
        Start-Sleep -Milliseconds 80
        (Get-SettingsPattern $appearanceNav ([System.Windows.Automation.InvokePattern]::Pattern)).Invoke()
        [void](Wait-SettingsElement '211')
        $interactionEvidence+=,[ordered]@{action='Invoke';automationId='301';result='navigated to Appearance before keyboard radio tests'}

        $keyboardSelection=Get-SettingsPattern ([System.Windows.Automation.AutomationElement]::FromHandle($settingsHwnd)) ([System.Windows.Automation.SelectionPattern]::Pattern)
        $keyboardInitial=@($keyboardSelection.Current.GetSelection())
        if($keyboardInitial.Count -ne 1){throw 'Keyboard radio fixture did not start with exactly one selected option'}
        $keyboardInitialId=$keyboardInitial[0].Current.AutomationId
        $keyboardRadio=Wait-SettingsElement $keyboardInitialId
        $keyboardRadioHwnd=[SettingsUiaNativeProbe]::FocusOwnedControl([IntPtr]$keyboardRadio.Current.NativeWindowHandle,$owned.Id,$settingsHwnd,$settingsClass)
        Start-Sleep -Milliseconds 80
        [void](Send-OwnedKey -Keys '{RIGHT}' -Target $keyboardRadioHwnd)
        $keyboardChanged=@((Get-SettingsPattern ([System.Windows.Automation.AutomationElement]::FromHandle($settingsHwnd)) ([System.Windows.Automation.SelectionPattern]::Pattern)).Current.GetSelection())
        if($keyboardChanged.Count -ne 1 -or $keyboardChanged[0].Current.AutomationId -eq $keyboardInitialId){throw 'Right Arrow did not move Settings radio selection'}
        $changedRadioHwnd=[SettingsUiaNativeProbe]::FocusedOwnedControl($settingsHwnd,$owned.Id,$settingsClass)
        [void](Send-OwnedKey -Keys '{LEFT}' -Target $changedRadioHwnd)
        $keyboardRestored=@((Get-SettingsPattern ([System.Windows.Automation.AutomationElement]::FromHandle($settingsHwnd)) ([System.Windows.Automation.SelectionPattern]::Pattern)).Current.GetSelection())
        if($keyboardRestored.Count -ne 1 -or $keyboardRestored[0].Current.AutomationId -ne $keyboardInitialId){throw 'Left Arrow did not restore the initial Settings radio selection'}
        $interactionEvidence+=,[ordered]@{action='Keyboard Left/Right';initial=$keyboardInitialId;changed=$keyboardChanged[0].Current.AutomationId;restored=$keyboardRestored[0].Current.AutomationId}
    }
    $final=Get-FreshDiagnostic $mainHwnd $owned.Id $logPath $mainClass
    $evidence=[ordered]@{
        binarySha256=$binarySha;ownedPid=$owned.Id;mainHwnd=$mainHwnd.ToInt64();settingsHwnd=$settingsHwnd.ToInt64()
        settingsClass=$settingsClass;readOnlyProbe=(-not $InteractionProbe);readOnlyParameterRequested=[bool]$ReadOnlyProbe
        host=$hostMetadata
        initialConfigurationValid=$initial.configurationValid;finalConfigurationValid=$final.configurationValid
        serviceFieldsAbsent=$true;initialSettingsAlive=$initial.settingsWindowAlive;finalSettingsAlive=$final.settingsWindowAlive
        uiaRootProcessId=$root.Current.ProcessId;uiaRootNativeWindowHandle=$root.Current.NativeWindowHandle
        descendantCount=$elements.Count;inventory=$inventory;screenshotsVerified=$false
        nativeCUIAutomationSameProcessAsManagedClient=$true
        mainMonitor=$mainMonitor;settingsMonitor=$settingsMonitor;interactionEvidence=$interactionEvidence
        note=if($InteractionProbe){'UIA Invoke, Selection, SelectionItem, Toggle, Value, RangeValue, Expand, and Collapse were exercised only in an isolated owned fixture; mutable values were restored.'}else{'Read-only inventory only. Native CUIAutomation and MSAA are cross-checked from an owned fixture process. No UIA action or value setter was called.'}
    }
} catch {
    $probeFailure=$_.Exception.Message
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
                $timeout=[ordered]@{binarySha256=$binarySha;ownedPid=$owned.Id;mainHwnd=$mainHwnd.ToInt64();settingsHwnd=if($settingsHwnd -ne [IntPtr]::Zero){$settingsHwnd.ToInt64()}else{$null};gracefulClosePosted=$closePosted;shutdownWaitMs=5000;probeFailure=$probeFailure;forcedKill=$false}
                $timeout|ConvertTo-Json|Set-Content -Encoding UTF8 (Join-Path $runFolder 'timeout-native-crosscheck.json')
                if($null -ne $evidence) {
                    $evidence['gracefulClosePosted']=$closePosted;$evidence['forcedKill']=$false;$evidence['shutdownTimeout']=$true
                    $evidence|ConvertTo-Json -Depth 10|Set-Content -Encoding UTF8 $failurePath
                }
                throw "Owned UIA probe failed to exit within 5000 ms after WM_CLOSE; no forced kill was issued. Probe failure: $probeFailure"
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

if($null -ne $evidence -and !$evidence.Contains('failure')) {
    if($InteractionProbe){Write-Output "PASS: Settings UIA inventory and $($evidence.interactionEvidence.Count) isolated interaction checks"}
    else{Write-Output "PASS: read-only UIA inventory from owned Settings HWND; $($evidence.descendantCount) descendants"}
}
