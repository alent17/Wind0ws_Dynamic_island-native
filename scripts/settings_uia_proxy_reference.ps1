param([string]$TestExe,[string]$ExpectedSha256)
$ErrorActionPreference='Stop'
$nativeRoot=Split-Path $PSScriptRoot
$artifact=Join-Path $nativeRoot 'artifacts/settings-uia-proxy-reference.json'

# This reference probe intentionally does not load UIAutomationClient.dll or
# UIAutomationTypes.dll. CUIAutomation is called through its native COM vtable.
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
    [PreserveSig] int GetCurrentPropertyValueEx(int propertyId, [MarshalAs(UnmanagedType.Bool)] bool ignoreDefault, [MarshalAs(UnmanagedType.Struct)] out object value);
    [PreserveSig] int GetCachedPropertyValue(int propertyId, [MarshalAs(UnmanagedType.Struct)] out object value);
    [PreserveSig] int GetCachedPropertyValueEx(int propertyId, [MarshalAs(UnmanagedType.Bool)] bool ignoreDefault, [MarshalAs(UnmanagedType.Struct)] out object value);
    [PreserveSig] int GetCurrentPatternAs(int patternId, ref Guid iid, out IntPtr pattern);
    [PreserveSig] int GetCachedPatternAs(int patternId, ref Guid iid, out IntPtr pattern);
    [PreserveSig] int GetCurrentPattern(int patternId, out IntPtr pattern);
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

public static class SettingsUiaProxyReference {
    [StructLayout(LayoutKind.Sequential)] struct InitCommonControls { public int size; public int classes; }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left,top,right,bottom; }
    [StructLayout(LayoutKind.Sequential)] struct ComboBoxInfo {
        public int size; public Rect item; public Rect button; public int buttonState;
        public IntPtr combo, itemWindow, listWindow;
    }
    [StructLayout(LayoutKind.Sequential)] struct Point { public int x,y; public Point(int px,int py){x=px;y=py;} }
    delegate bool EnumWindowProc(IntPtr hwnd,IntPtr lParam);
    [DllImport("comctl32.dll", SetLastError=true)] static extern bool InitCommonControlsEx(ref InitCommonControls value);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr CreateWindowExW(uint exStyle,string className,string title,uint style,int x,int y,int width,int height,IntPtr parent,IntPtr menu,IntPtr instance,IntPtr param);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr SendMessageW(IntPtr hwnd,uint message,IntPtr wParam,IntPtr lParam);
    [DllImport("user32.dll", SetLastError=true)] static extern bool ShowWindow(IntPtr hwnd,int command);
    [DllImport("user32.dll", SetLastError=true)] static extern bool DestroyWindow(IntPtr hwnd);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowProc callback,IntPtr lParam);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent,EnumWindowProc callback,IntPtr lParam);
    [DllImport("user32.dll", SetLastError=true)] static extern bool PostMessageW(IntPtr hwnd,uint message,IntPtr wParam,IntPtr lParam);
    [DllImport("user32.dll", EntryPoint="GetWindowLongPtrW", SetLastError=true)] static extern IntPtr GetWindowLongPtrW(IntPtr hwnd,int index);
    [DllImport("user32.dll", SetLastError=true)] static extern bool GetWindowRect(IntPtr hwnd,out Rect rect);
    [DllImport("user32.dll", SetLastError=true)] static extern bool GetClientRect(IntPtr hwnd,out Rect rect);
    [DllImport("user32.dll", SetLastError=true)] static extern bool ScreenToClient(IntPtr hwnd,ref Point point);
    [DllImport("user32.dll", SetLastError=true)] static extern bool GetComboBoxInfo(IntPtr hwnd,ref ComboBoxInfo info);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern int GetClassNameW(IntPtr hwnd,StringBuilder name,int max);
    [DllImport("user32.dll", SetLastError=true)] static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
    [DllImport("user32.dll", SetLastError=true)] static extern int GetDlgCtrlID(IntPtr hwnd);
    [DllImport("ole32.dll")] static extern int CoInitializeEx(IntPtr reserved,int coInit);
    [DllImport("oleacc.dll")] static extern int AccessibleObjectFromWindow(IntPtr hwnd,uint objectId,ref Guid iid,[MarshalAs(UnmanagedType.Interface)] out IAccessibleNative accessible);
    static readonly Guid CUIAutomationClsid=new Guid("ff48dba4-60ef-4201-aa87-54103eef594e");
    static readonly Guid IAccessibleIid=new Guid("618736e0-3c3d-11cf-810c-00aa00389b71");

    static void Check(IntPtr value,string description){if(value==IntPtr.Zero)throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error(),description);}
    static IntPtr Create(string cls,string text,uint style,int x,int y,int w,int h,IntPtr parent){
        var hwnd=CreateWindowExW(0,cls,text,style,x,y,w,h,parent,IntPtr.Zero,IntPtr.Zero,IntPtr.Zero);
        Check(hwnd,"CreateWindowExW "+cls); return hwnd;
    }
    public static Dictionary<string,long> CreateReferenceControls(){
        var init=new InitCommonControls{size=Marshal.SizeOf(typeof(InitCommonControls)),classes=4};
        if(!InitCommonControlsEx(ref init))throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error(),"InitCommonControlsEx");
        IntPtr host=Create("STATIC","UIA standard proxy reference",0x00CF0000,100,100,620,330,IntPtr.Zero);
        const uint child=0x40000000|0x10000000|0x00010000;
        IntPtr button=Create("Button","Reference Button",child,20,20,180,32,host);
        IntPtr toggle=Create("Button","Reference Toggle",child|0x0003,20,65,180,32,host);
        IntPtr radio=Create("Button","Reference Radio",child|0x0009,20,110,180,32,host);
        IntPtr combo=Create("ComboBox","",child|0x0003,240,25,260,180,host);
        IntPtr item=Marshal.StringToHGlobalUni("Reference one"); SendMessageW(combo,0x0143,IntPtr.Zero,item); Marshal.FreeHGlobal(item);
        item=Marshal.StringToHGlobalUni("Reference two"); SendMessageW(combo,0x0143,IntPtr.Zero,item); Marshal.FreeHGlobal(item);
        SendMessageW(combo,0x014E,IntPtr.Zero,IntPtr.Zero);
        IntPtr slider=Create("msctls_trackbar32","",child|0x0001,240,85,300,40,host);
        SendMessageW(slider,0x0406,IntPtr.Zero,new IntPtr((100<<16)|0)); SendMessageW(slider,0x0405,new IntPtr(1),new IntPtr(45));
        var result=new Dictionary<string,long>{{"host",host.ToInt64()},{"button",button.ToInt64()},{"toggle",toggle.ToInt64()},{"radio",radio.ToInt64()},{"combo",combo.ToInt64()},{"slider",slider.ToInt64()}};
        return result;
    }
    static string Class(IntPtr hwnd){var name=new StringBuilder(256);GetClassNameW(hwnd,name,name.Capacity);return name.ToString();}
    static int Owner(IntPtr hwnd){uint pid;GetWindowThreadProcessId(hwnd,out pid);return (int)pid;}
    public static long[] FindTop(int pid,string expectedClass){
        var found=new List<long>();
        EnumWindows(delegate(IntPtr hwnd,IntPtr unused){if(Owner(hwnd)==pid&&Class(hwnd)==expectedClass)found.Add(hwnd.ToInt64());return true;},IntPtr.Zero);
        return found.ToArray();
    }
    public static long[] Children(int pid,long parent){
        var found=new List<long>();
        EnumChildWindows(new IntPtr(parent),delegate(IntPtr hwnd,IntPtr unused){if(Owner(hwnd)==pid)found.Add(hwnd.ToInt64());return true;},IntPtr.Zero);
        return found.ToArray();
    }
    public static Dictionary<string,object> DescribeWindow(long value){
        IntPtr hwnd=new IntPtr(value);
        return new Dictionary<string,object>{{"hwnd",value},{"className",Class(hwnd)},{"processId",Owner(hwnd)},{"controlId",GetDlgCtrlID(hwnd)},{"style",Style(hwnd)},{"extendedStyle",ExtendedStyle(hwnd)},{"windowRect",WindowRect(hwnd)}};
    }
    public static void PostChecked(long hwnd,int pid,string expectedClass,uint message,IntPtr wParam,IntPtr lParam){
        IntPtr window=new IntPtr(hwnd);if(Owner(window)!=pid||Class(window)!=expectedClass)throw new InvalidOperationException("HWND/PID/class ownership mismatch before PostMessage");
        if(!PostMessageW(window,message,wParam,lParam))throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error(),"PostMessageW failed");
    }
    public static void ShowChecked(long hwnd,int pid,string expectedClass){IntPtr window=new IntPtr(hwnd);if(Owner(window)!=pid||Class(window)!=expectedClass)throw new InvalidOperationException("HWND/PID/class ownership mismatch before ShowWindow");ShowWindow(window,4);}
    public static long[] WindowRect(IntPtr hwnd){Rect rect;GetWindowRect(hwnd,out rect);return new long[]{rect.left,rect.top,rect.right,rect.bottom};}
    public static long Style(IntPtr hwnd){return GetWindowLongPtrW(hwnd,-16).ToInt64();}
    public static long ExtendedStyle(IntPtr hwnd){return GetWindowLongPtrW(hwnd,-20).ToInt64();}
    static object Property(IUIAutomationElementNative element,int id){object value=null;int hr=element.GetCurrentPropertyValue(id,out value);return hr<0?null:value;}
    static Dictionary<string,object> Msaa(IntPtr hwnd){
        var value=new Dictionary<string,object>(); IAccessibleNative accessible=null; Guid iid=IAccessibleIid;
        int hr=AccessibleObjectFromWindow(hwnd,0xFFFFFFFC,ref iid,out accessible);value["hresult"]=hr;
        if(hr<0||accessible==null)return value;
        object self=0,role=null,state=null;string name=null,action=null;int count=0;
        int countHr=accessible.get_accChildCount(out count),nameHr=accessible.get_accName(self,out name);
        int roleHr=accessible.get_accRole(self,out role),stateHr=accessible.get_accState(self,out state);
        int actionHr=accessible.get_accDefaultAction(self,out action);
        value["childCountHresult"]=countHr;value["childCount"]=countHr<0?(object)null:count;
        value["nameHresult"]=nameHr;value["name"]=nameHr<0?null:name;
        value["roleHresult"]=roleHr;value["role"]=roleHr<0||role==null?null:role.ToString();
        value["stateHresult"]=stateHr;value["state"]=stateHr<0||state==null?null:state.ToString();
        value["defaultActionHresult"]=actionHr;value["defaultAction"]=actionHr<0?null:action;
        return value;
    }
    public static Dictionary<string,object> Query(IntPtr hwnd){
        int hr=CoInitializeEx(IntPtr.Zero,0);if(hr<0&&hr!=unchecked((int)0x80010106))Marshal.ThrowExceptionForHR(hr);
        var result=new Dictionary<string,object>{{"hwnd",hwnd.ToInt64()},{"className",Class(hwnd)},{"processId",Owner(hwnd)},{"controlId",GetDlgCtrlID(hwnd)},{"style",Style(hwnd)},{"extendedStyle",ExtendedStyle(hwnd)},{"windowRect",WindowRect(hwnd)}};
        try{
            var automation=(IUIAutomationNative)Activator.CreateInstance(Type.GetTypeFromCLSID(CUIAutomationClsid));
            IUIAutomationElementNative element;hr=automation.ElementFromHandle(hwnd,out element);result["nativeCUIAutomationHresult"]=hr;
            if(hr>=0&&element!=null){
                result["uiaProcessId"]=Property(element,30002);result["controlTypeId"]=Property(element,30003);
                result["name"]=Property(element,30005);result["automationId"]=Property(element,30011);
                result["uiaClassName"]=Property(element,30012);result["nativeWindowHandle"]=Property(element,30020);
                result["frameworkId"]=Property(element,30024);result["providerDescription"]=Property(element,30107);
                var patterns=new Dictionary<string,bool>();
                foreach(var p in new[]{new KeyValuePair<string,int>("Invoke",10000),new KeyValuePair<string,int>("Selection",10001),new KeyValuePair<string,int>("Value",10002),new KeyValuePair<string,int>("RangeValue",10003),new KeyValuePair<string,int>("ExpandCollapse",10005),new KeyValuePair<string,int>("SelectionItem",10010),new KeyValuePair<string,int>("Toggle",10015)}){
                    IntPtr pattern;int phr=element.GetCurrentPattern(p.Value,out pattern);patterns[p.Key]=phr>=0&&pattern!=IntPtr.Zero;if(pattern!=IntPtr.Zero)Marshal.Release(pattern);
                }
                result["patterns"]=patterns;
            }
        }catch(Exception error){result["nativeCUIAutomationError"]=error.ToString();}
        result["msaa"]=Msaa(hwnd);
        if(Class(hwnd)=="ComboBox"){
            Rect window,client;GetWindowRect(hwnd,out window);GetClientRect(hwnd,out client);
            var info=new ComboBoxInfo{size=Marshal.SizeOf(typeof(ComboBoxInfo))};bool ok=GetComboBoxInfo(hwnd,ref info);
            var converted=info.button;Point p1=new Point(converted.left,converted.top),p2=new Point(converted.right,converted.bottom);
            if(ok){ScreenToClient(hwnd,ref p1);ScreenToClient(hwnd,ref p2);}
            result["comboBoxInfo"] = ok ? new Dictionary<string,object>{{"rcItem",new int[]{info.item.left,info.item.top,info.item.right,info.item.bottom}},{"rcButton",new int[]{info.button.left,info.button.top,info.button.right,info.button.bottom}},{"rcButtonScreenToClient",new int[]{p1.x,p1.y,p2.x,p2.y}},{"windowRect",new int[]{window.left,window.top,window.right,window.bottom}},{"clientRect",new int[]{client.left,client.top,client.right,client.bottom}},{"hwndCombo",info.combo.ToInt64()},{"hwndItem",info.itemWindow.ToInt64()},{"hwndList",info.listWindow.ToInt64()}} : null;
        }
        return result;
    }
    public static bool CloseReferenceHost(long hwnd){return DestroyWindow(new IntPtr(hwnd));}
}
'@

$uiaAssemblies=[AppDomain]::CurrentDomain.GetAssemblies()|Where-Object{$_.GetName().Name -in @('UIAutomationClient','UIAutomationTypes')}
if(@($uiaAssemblies).Count -ne 0){throw 'Reference probe must not load UIAutomationClient or UIAutomationTypes'}
if($TestExe) {
    $exe=(Resolve-Path -LiteralPath $TestExe).Path
    $sha=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
    if($ExpectedSha256 -and $sha -ne $ExpectedSha256){throw "Candidate SHA mismatch: expected $ExpectedSha256 got $sha"}
    if($env:ISLE_TEST_MONITOR -ne '\\.\DISPLAY2'){throw 'ISLE_TEST_MONITOR must be set to \\.\DISPLAY2 before the UI run'}
    $runFolder=Join-Path $nativeRoot ("artifacts/settings-uia-"+$sha.Substring(0,12))
    New-Item -ItemType Directory -Force $runFolder|Out-Null
    $stamp=Get-Date -Format 'yyyyMMdd-HHmmss'
    $settingsPath=Join-Path $runFolder ("native-core-fixture-settings-"+$stamp+".json")
    $logPath=Join-Path $runFolder ("native-core-snapshot-"+$stamp+".json")
    $resultsPath=Join-Path $runFolder ("uia-inventory-native-core-"+$stamp+".json")
    $failurePath=Join-Path $runFolder ("failure-native-core-"+$stamp+".json")
    $fixture='{"enableAnimations":false,"reduceAnimations":true,"compactLength":100,"floatingFillColor":"#102030","futureSettings":{"keep":[1,null,{"v":2}]}}'
    [System.IO.File]::WriteAllText($settingsPath,$fixture,[System.Text.UTF8Encoding]::new($false))
    $savedFault=$env:ISLE_TEST_CONTROL_FAIL_CREATE
    Remove-Item Env:\ISLE_TEST_CONTROL_FAIL_CREATE -ErrorAction SilentlyContinue
    $owned=$null;$main=0L;$settings=0L;$evidence=$null;$inventory=@();$childWindows=@()
    try {
        $args=@('--ui-v2','--test-fixture','--page','music','--paused','--reduced-motion','--settings-path',$settingsPath,'--log',$logPath)
        $owned=Start-Process -FilePath $exe -ArgumentList $args -PassThru -WindowStyle Hidden
        for($i=0;$i -lt 160;$i++){
            $found=[SettingsUiaProxyReference]::FindTop($owned.Id,'IsleNativePrototype')
            if($found.Count -gt 1){throw 'owned app has multiple IsleNativePrototype windows'}
            if($found.Count -eq 1){$main=$found[0];break}
            $owned.Refresh();if($owned.HasExited){throw "candidate exited before main window: $($owned.ExitCode)"}
            Start-Sleep -Milliseconds 50
        }
        if($main -eq 0){throw 'owned candidate did not create IsleNativePrototype'}
        [SettingsUiaProxyReference]::ShowChecked($main,$owned.Id,'IsleNativePrototype')
        function Get-CleanDiagnostic([long]$MainHwnd,[int]$OwnerProcessId,[string]$Path){
            $before=if(Test-Path -LiteralPath $Path){(Get-Item -LiteralPath $Path).LastWriteTimeUtc.Ticks}else{0}
            for($n=0;$n -lt 60;$n++){
                [SettingsUiaProxyReference]::PostChecked($MainHwnd,$OwnerProcessId,'IsleNativePrototype',0x803c,[IntPtr]::Zero,[IntPtr]::Zero)
                Start-Sleep -Milliseconds 50
                if((Test-Path -LiteralPath $Path) -and (Get-Item -LiteralPath $Path).LastWriteTimeUtc.Ticks -ne $before){
                    try{$sample=Get-Content -LiteralPath $Path -Raw|ConvertFrom-Json}catch{Start-Sleep -Milliseconds 20;continue}
                    if($sample.prototype -eq $true -and $sample.uiV2 -eq $true -and $sample.renderer){
                        if($sample.configurationValid -ne $true){throw 'isolated UI V2 fixture is invalid'}
                        $names=@($sample.PSObject.Properties.Name)
                        if($names -contains 'audioPolls' -or $names -contains 'mediaPolls'){throw 'fixture created live audio/media services'}
                        return $sample
                    }
                }
            }
            throw 'fresh candidate diagnostic did not arrive'
        }
        $initial=Get-CleanDiagnostic $main $owned.Id $logPath
        if($initial.settingsWindowAlive -eq $true){throw 'Settings unexpectedly alive before F8'}
        [SettingsUiaProxyReference]::PostChecked($main,$owned.Id,'IsleNativePrototype',0x0100,[IntPtr]0x77,[IntPtr]::Zero)
        for($i=0;$i -lt 100;$i++){
            $found=[SettingsUiaProxyReference]::FindTop($owned.Id,'IsleNativeSettingsV2')
            if($found.Count -gt 1){throw 'owned app has multiple IsleNativeSettingsV2 windows'}
            if($found.Count -eq 1){$settings=$found[0];break}
            Start-Sleep -Milliseconds 50
        }
        if($settings -eq 0){throw 'F8 did not create IsleNativeSettingsV2'}
        [SettingsUiaProxyReference]::ShowChecked($settings,$owned.Id,'IsleNativeSettingsV2')
        $opened=Get-CleanDiagnostic $main $owned.Id $logPath
        if($opened.settingsWindowAlive -ne $true){throw 'fresh diagnostic did not report Settings alive'}
        $hwnds=[SettingsUiaProxyReference]::Children($owned.Id,$settings)
        foreach($value in $hwnds){
            $window=[SettingsUiaProxyReference]::DescribeWindow($value)
            if($window.processId -ne $owned.Id){throw "EnumChildWindows returned HWND outside candidate PID: hwnd=$value class=$($window.className) nativeOwner=$($window.processId) expected=$($owned.Id)"}
            $childWindows+=,$window
        }
        $progressPath=Join-Path $runFolder ("uia-progress-native-core-"+$stamp+".json")
        foreach($value in $hwnds){
            $queryStarted=[Diagnostics.Stopwatch]::StartNew()
            $item=[SettingsUiaProxyReference]::Query([IntPtr]$value)
            $queryStarted.Stop()
            $item['queryElapsedMs']=$queryStarted.ElapsedMilliseconds
            $item['uiaProcessIdMatchesHwndOwner']=if($null -eq $item.uiaProcessId){$null}else{[int]$item.uiaProcessId -eq [int]$item.processId}
            if($item.processId -ne $owned.Id){throw "Query HWND ownership changed during inventory: hwnd=$value class=$($item.className) nativeOwner=$($item.processId) expected=$($owned.Id)"}
            $inventory+=,$item
            $partial=[ordered]@{
                candidate=$exe;binarySha256=$sha;ownedPid=$owned.Id;mainHwnd=$main;settingsHwnd=$settings
                host=[ordered]@{powershellVersion=$PSVersionTable.PSVersion.ToString();powershellEdition=$PSVersionTable.PSEdition;frameworkDescription=[Runtime.InteropServices.RuntimeInformation]::FrameworkDescription;uiAutomationClientLoaded=$false;uiAutomationTypesLoaded=$false;clientProxyRegistration='not used'}
                fixture=[ordered]@{configurationValid=$initial.configurationValid;settingsAlive=$opened.settingsWindowAlive}
                hwndCount=$hwnds.Count;queriedHwndCount=$inventory.Count;childWindows=$childWindows;inventory=$inventory
                currentHwnd=$value;resultsAreNativeCUIAutomationCore=$true;msaaQueried=$true;noCustomProviderRegistered=$true
                note='Partial independent E6 baseline; per-HWND UIA ProcessId differences are recorded and not treated as native HWND ownership violations.'
            }
            [System.IO.File]::WriteAllText($progressPath,($partial|ConvertTo-Json -Depth 14),[System.Text.UTF8Encoding]::new($false))
        }
        $final=Get-CleanDiagnostic $main $owned.Id $logPath
        $evidence=[ordered]@{
            candidate=$exe;binarySha256=$sha;ownedPid=$owned.Id;mainHwnd=$main;settingsHwnd=$settings;settingsClass='IsleNativeSettingsV2'
            host=[ordered]@{powershellVersion=$PSVersionTable.PSVersion.ToString();powershellEdition=$PSVersionTable.PSEdition;frameworkDescription=[Runtime.InteropServices.RuntimeInformation]::FrameworkDescription;uiAutomationClientLoaded=$false;uiAutomationTypesLoaded=$false;clientProxyRegistration='not used'}
            fixture=[ordered]@{configurationValid=$initial.configurationValid;finalConfigurationValid=$final.configurationValid;audioMediaFieldsAbsent=$true;settingsAlive=$opened.settingsWindowAlive}
            descendantHwndCount=$hwnds.Count;inventory=$inventory;resultsAreNativeCUIAutomationCore=true;msaaQueried=true
            noCustomProviderRegistered=$true;screenshotsVerified=$false;note='Read-only isolated E6 baseline. Native CUIAutomation and MSAA queried from a PS 5.1 process without UIAutomationClient/Types or custom provider registration. This result is diagnostic and does not alone assign a product defect.'
        }
    }catch{
        $evidence=[ordered]@{binarySha256=$sha;ownedPid=if($owned){$owned.Id}else{$null};mainHwnd=$main;settingsHwnd=$settings;failure=$_.Exception.ToString();forcedKill=$false;uiAutomationClientLoaded=$false;childWindows=$childWindows;inventory=$inventory}
        throw
    }finally{
        if($null -ne $owned){
            $owned.Refresh();$closePosted=$false
            if(!$owned.HasExited){
                if($settings -ne 0 -and [SettingsUiaProxyReference]::FindTop($owned.Id,'IsleNativeSettingsV2').Count -eq 1){[SettingsUiaProxyReference]::PostChecked($settings,$owned.Id,'IsleNativeSettingsV2',0x0010,[IntPtr]::Zero,[IntPtr]::Zero)}
                if($main -ne 0){[SettingsUiaProxyReference]::PostChecked($main,$owned.Id,'IsleNativePrototype',0x0010,[IntPtr]::Zero,[IntPtr]::Zero);$closePosted=$true}
                if(!$owned.WaitForExit(5000)){
                    $timeout=[ordered]@{binarySha256=$sha;ownedPid=$owned.Id;mainHwnd=$main;settingsHwnd=$settings;gracefulClosePosted=$closePosted;shutdownWaitMs=5000;forcedKill=$false}
                    [System.IO.File]::WriteAllText((Join-Path $runFolder ("timeout-native-core-"+$stamp+".json")),($timeout|ConvertTo-Json -Depth 8),[System.Text.UTF8Encoding]::new($false))
                    throw 'Owned candidate did not exit within 5000 ms after WM_CLOSE; no forced kill was issued'
                }
            }
            $owned.Refresh()
            if(!$owned.HasExited){throw 'Owned candidate still exists after graceful close'}
            if($null -ne $evidence){$evidence['ownedExitCode']=$owned.ExitCode;$evidence['gracefulClosePosted']=$closePosted;$evidence['forcedKill']=$false}
            if($owned.ExitCode -ne 0){throw "Owned candidate exited with code $($owned.ExitCode)"}
        }
        if($null -ne $evidence){$file=if($evidence.Contains('failure')){$failurePath}else{$resultsPath};[System.IO.File]::WriteAllText($file,($evidence|ConvertTo-Json -Depth 14),[System.Text.UTF8Encoding]::new($false))}
        if($null -ne $savedFault){$env:ISLE_TEST_CONTROL_FAIL_CREATE=$savedFault}
    }
    $typeCounts=$inventory|Group-Object controlTypeId|ForEach-Object{"$($_.Name)=$($_.Count)"}
    Write-Output "Native-core candidate baseline captured; HWNDs=$($hwnds.Count); control types=$($typeCounts -join ', '); output=$resultsPath"
}else{
    $hwnds=[SettingsUiaProxyReference]::CreateReferenceControls()
    try {
        Start-Sleep -Milliseconds 200
        $results=@()
        foreach($name in @('button','toggle','radio','combo','slider')) {
            $item=[SettingsUiaProxyReference]::Query([IntPtr]$hwnds[$name])
            $item['referenceName']=$name
            $item['windowStyleContext']='Win32 stock control class; no custom UIA provider registered'
            $results+=,$item
        }
        $patternsExpected=@{button='Invoke';toggle='Toggle';radio='SelectionItem';combo='ExpandCollapse';slider='RangeValue'}
        foreach($item in $results){$pattern=$patternsExpected[$item.referenceName];$item['expectedPattern']=$pattern;$item['expectedPatternPresent']=$item.patterns -and $item.patterns[$pattern]}
        $comboReference=@($results|Where-Object{$_.referenceName -eq 'combo'})
        $comboChrome=if($comboReference.Count -gt 0){$comboReference[0]['comboBoxInfo']}else{$null}
        $metadata=[ordered]@{
            powershellVersion=$PSVersionTable.PSVersion.ToString()
            powershellEdition=$PSVersionTable.PSEdition
            frameworkDescription=[Runtime.InteropServices.RuntimeInformation]::FrameworkDescription
            uiAutomationClientLoaded=$false
            windowProcessId=$PID
            hwnds=$hwnds
            results=$results
            comboChrome=$comboChrome
            noCustomProviderRegistered=$true
        }
        $metadata|ConvertTo-Json -Depth 12|Set-Content -LiteralPath $artifact -Encoding UTF8
        $bad=@($results|Where-Object{-not $_.expectedPatternPresent})
        if($bad.Count -gt 0){Write-Output "Reference patterns unavailable for: $(($bad.referenceName) -join ', ')"}else{Write-Output "PASS: stock Win32 control UIA reference patterns available; $artifact"}
    } finally {
        [void][SettingsUiaProxyReference]::CloseReferenceHost($hwnds.host)
    }
}
