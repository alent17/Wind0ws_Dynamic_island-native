# Run in Windows PowerShell (the .NET Framework Accessibility interop assembly).
$ErrorActionPreference='Stop'
Add-Type -AssemblyName Accessibility
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -ReferencedAssemblies Accessibility -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using Accessibility;
public static class NativeAccessCheck {
 [StructLayout(LayoutKind.Sequential)] struct Point {public int x,y;}
 delegate bool EnumWindowsProc(IntPtr h,IntPtr l);
 [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowsProc callback,IntPtr l);
 [DllImport("user32.dll")] static extern bool IsWindow(IntPtr h);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,StringBuilder name,int max);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
 [DllImport("user32.dll")] static extern bool ScreenToClient(IntPtr h,ref Point p);
 [DllImport("oleacc.dll")] static extern int AccessibleObjectFromWindow(IntPtr h, uint id, ref Guid iid, [MarshalAs(UnmanagedType.Interface)] out IAccessible a);
 [DllImport("user32.dll",SetLastError=true)] static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
 static IAccessible Get(IntPtr h) { var iid=new Guid("618736e0-3c3d-11cf-810c-00aa00389b71"); IAccessible a; Marshal.ThrowExceptionForHR(AccessibleObjectFromWindow(h,0xfffffffc,ref iid,out a)); return a; }
 static void Require(bool value,string message) {if(!value)throw new Exception(message);}
 public static IntPtr FindPrototype(int pid) {
  var found=new List<IntPtr>();
  EnumWindows(delegate(IntPtr h,IntPtr l) {
   uint owner;GetWindowThreadProcessId(h,out owner);
   if(owner==(uint)pid) {var name=new StringBuilder(256);GetClassName(h,name,name.Capacity);if(name.ToString()=="IsleNativePrototype")found.Add(h);}
   return true;
  },IntPtr.Zero);
  Require(found.Count<=1,"owned PID has multiple IsleNativePrototype windows: "+found.Count);
  return found.Count==1?found[0]:IntPtr.Zero;
 }
 public static void RequirePrototype(IntPtr h,int pid) {
  Require(h!=IntPtr.Zero&&IsWindow(h),"owned prototype HWND is no longer valid");
  uint owner;GetWindowThreadProcessId(h,out owner);
  var name=new StringBuilder(256);GetClassName(h,name,name.Capacity);
  Require(owner==(uint)pid&&name.ToString()=="IsleNativePrototype","HWND is not the owned IsleNativePrototype");
 }
 public static void PostChecked(IntPtr h,uint m,IntPtr w,IntPtr l) {
  if(!PostMessage(h,m,w,l))throw new Win32Exception(Marshal.GetLastWin32Error(),"PostMessage failed for owned HWND "+h);
 }
 public static void BackFromVolume(IntPtr h) {
  var root=Get(h);Require(root.accChildCount==7,"UIA invoke opened volume page");
  ((IAccessible)root.get_accChild(6)).accDoDefaultAction(0);Thread.Sleep(200);
 }
 public static string Run(IntPtr h) {
  var root=Get(h); Require(root.accChildCount==8,"music control count");
  var names=new List<string>();
  for(int i=1;i<=root.accChildCount;i++)names.Add(root.get_accName(i));
  var volumeTool=(IAccessible)root.get_accChild(2);
  Require(volumeTool.get_accName(0)=="\u97f3\u91cf","volume tool name");
  int vx,vy,vw,vh;volumeTool.accLocation(out vx,out vy,out vw,out vh,0);
  var point=new Point{x=vx+vw/2,y=vy+vh/2};ScreenToClient(h,ref point);
  var packed=(IntPtr)((point.x&65535)|((point.y&65535)<<16));
  PostChecked(h,0x201,(IntPtr)1,packed);PostChecked(h,0x202,IntPtr.Zero,packed);Thread.Sleep(400);
  root=Get(h); Require(root.accChildCount==7,"volume page control count");
  var slider=(IAccessible)root.get_accChild(7);
  Require(Convert.ToInt32(slider.get_accRole(0))==0x33,"slider role");
  slider.set_accValue(0,"73"); Thread.Sleep(150);
  Require(slider.get_accValue(0)=="73","slider value write/read");
  slider.accSelect(1,0); Thread.Sleep(150);
  Require((Convert.ToInt32(slider.get_accState(0))&4)!=0,"slider focused state");
  int x,y,w,height;slider.accLocation(out x,out y,out w,out height,0);
  Require(w>0&&height>0,"physical control bounds");
  ((IAccessible)root.get_accChild(6)).accDoDefaultAction(0);Thread.Sleep(300);
  bool staleRejected=false;try{slider.set_accValue(0,"24");}catch(COMException){staleRejected=true;}
  Require(staleRejected,"stale child provider rejected");
  root=Get(h);Require(root.accChildCount==8,"back navigation via accessibility");
  PostChecked(h,0x100,(IntPtr)0x1b,IntPtr.Zero);Thread.Sleep(150);
  Require(Get(h).accChildCount==0,"collapsed accessible children released");
  return "MSAA names, roles, invoke, focus, bounds, volume value, stale handles and collapse passed: "+String.Join(", ",names);
 }
}
'@
$nativeRoot=Split-Path $PSScriptRoot
$testExe=if($env:ISLE_TEST_EXE){$env:ISLE_TEST_EXE}else{Join-Path $nativeRoot 'target/release/isle-native.exe'}
$artifactRoot=Join-Path $nativeRoot 'artifacts'
New-Item -ItemType Directory -Force $artifactRoot|Out-Null
$testSettings=Join-Path $artifactRoot 'accessibility-settings-test.json'
$fixtureText='{"showCustomFunctionPanel":true,"showTimerTool":true,"showVolumeTool":true,"showFloatingTool":true,"showSettingsTool":true,"showHideTool":true,"showClockTool":true,"showWeatherTool":true,"enableAnimations":false,"reduceAnimations":true}'
[System.IO.File]::WriteAllText($testSettings,$fixtureText,[System.Text.UTF8Encoding]::new($false))
$accessLog=Join-Path $artifactRoot 'accessibility-snapshot.json'
function Get-IsolatedDiagnostics([IntPtr]$Window,[string]$Path,[int]$ExpectedPid) {
    $before=if(Test-Path -LiteralPath $Path){(Get-Item -LiteralPath $Path).LastWriteTimeUtc.Ticks}else{0}
    for($attempt=0;$attempt -lt 40;$attempt++) {
        [NativeAccessCheck]::RequirePrototype($Window,$ExpectedPid)
        [NativeAccessCheck]::PostChecked($Window,0x803c,[IntPtr]::Zero,[IntPtr]::Zero)
        Start-Sleep -Milliseconds 50
        if((Test-Path -LiteralPath $Path) -and (Get-Item -LiteralPath $Path).LastWriteTimeUtc.Ticks -ne $before) {
            try {
                $sample=Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
                if($sample.prototype -eq $true -and $sample.renderer) {
                    if($sample.configurationValid -ne $true){throw 'Accessibility fixture configuration did not load as valid UTF-8 JSON'}
                    # App::report only emits these fields when its service exists.
                    # Absence confirms no real service, even before its first poll.
                    $fields=@($sample.PSObject.Properties.Name)
                    if($fields -contains 'audioPolls' -or $fields -contains 'mediaPolls'){throw 'Accessibility fixture created live services'}
                    return $sample
                }
            } catch [System.ArgumentException] {}
        }
    }
    throw 'Accessibility fixture diagnostics did not update'
}
$owned=Start-Process $testExe -ArgumentList '--demo','--page','music','--paused','--reduced-motion','--test-dpi','144','--settings-path',$testSettings,'--log',$accessLog -PassThru -WindowStyle Hidden
$mainHwnd=[IntPtr]::Zero
$evidence=$null
try {
    for($i=0;$i -lt 100;$i++) {
        $owned.Refresh()
        $mainHwnd=[NativeAccessCheck]::FindPrototype($owned.Id)
        if($mainHwnd -ne [IntPtr]::Zero){break}
        Start-Sleep -Milliseconds 50
    }
    if($mainHwnd -eq [IntPtr]::Zero){throw 'Owned PID did not create a unique IsleNativePrototype HWND'}
    [NativeAccessCheck]::RequirePrototype($mainHwnd,$owned.Id)
    Start-Sleep -Milliseconds 500
    # Verify the isolated run before invoking any value-changing pattern.
    $initialDiag=Get-IsolatedDiagnostics $mainHwnd $accessLog $owned.Id
    if($initialDiag.configurationValid -ne $true){throw 'Initial fixture diagnostics did not confirm a valid configuration'}
    [NativeAccessCheck]::RequirePrototype($mainHwnd,$owned.Id)
    $uiaRoot=[System.Windows.Automation.AutomationElement]::FromHandle($mainHwnd)
    $uiaChildren=$uiaRoot.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
    $uiaNames=@($uiaChildren | ForEach-Object { $_.Current.Name })
    $volumeName=([char]0x97f3).ToString()+([char]0x91cf).ToString()
    if($uiaNames -notcontains $volumeName){throw "UIA legacy bridge did not expose volume: $($uiaNames -join ', ')"}
    Write-Output "UIA legacy bridge exposed $($uiaChildren.Count) descendants"
    $volumeElement=@($uiaChildren | Where-Object {$_.Current.Name -eq $volumeName})[0]
    ([System.Windows.Automation.InvokePattern]$volumeElement.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)).Invoke()
    Start-Sleep -Milliseconds 300
    $volumePage=[System.Windows.Automation.AutomationElement]::FromHandle($mainHwnd)
    $volumeDescendants=$volumePage.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
    $sliderElement=@($volumeDescendants | Where-Object {$_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Slider}) | Select-Object -First 1
    if(!$sliderElement){throw "UIA volume detail did not expose a slider: $((@($volumeDescendants | ForEach-Object {$_.Current.Name}) -join ', '))"}
    $rangePattern=$null
    if(!$sliderElement.TryGetCurrentPattern([System.Windows.Automation.RangeValuePattern]::Pattern,[ref]$rangePattern)){throw 'UIA volume slider lacks RangeValuePattern'}
    $volumeRangeMin=$rangePattern.Current.Minimum
    $volumeRangeMax=$rangePattern.Current.Maximum
    if($volumeRangeMin -ne 0 -or $volumeRangeMax -ne 100){throw "UIA volume range must be 0–100, got $volumeRangeMin–$volumeRangeMax"}
    if($rangePattern.Current.IsReadOnly -or $rangePattern.Current.SmallChange -ne 1 -or $rangePattern.Current.LargeChange -ne 10){throw 'UIA volume read-only or step metadata is incorrect'}
    $rangePattern.SetValue(67)
    Start-Sleep -Milliseconds 200
    if($rangePattern.Current.Value -ne 67){throw "UIA RangeValue SetValue did not update volume: $($rangePattern.Current.Value)"}
    $invalidRangeRejected=$false
    try { $rangePattern.SetValue(101) } catch [System.Runtime.InteropServices.COMException] { $invalidRangeRejected=$true } catch [System.InvalidOperationException] { $invalidRangeRejected=$true }
    if(!$invalidRangeRejected){throw 'UIA accepted volume outside its 0–100 range'}
    [NativeAccessCheck]::RequirePrototype($mainHwnd,$owned.Id)
    [NativeAccessCheck]::BackFromVolume($mainHwnd)
    Write-Output 'UIA InvokePattern navigation passed'
    [NativeAccessCheck]::RequirePrototype($mainHwnd,$owned.Id)
    [NativeAccessCheck]::Run($mainHwnd)
    $diag=Get-IsolatedDiagnostics $mainHwnd $accessLog $owned.Id
    $evidence=[ordered]@{
        ownedPid=$owned.Id
        mainHwnd=$mainHwnd.ToInt64()
        binarySha256=(Get-FileHash $testExe -Algorithm SHA256).Hash
        isolatedDemo=$true; initialServiceFieldsAbsent=$true; finalServiceFieldsAbsent=$true
        initialConfigurationValid=$initialDiag.configurationValid
        finalConfigurationValid=$diag.configurationValid
        initialMediaPolls=$initialDiag.mediaPolls; initialAudioPolls=$initialDiag.audioPolls
        liveMediaPolls=$diag.mediaPolls; liveAudioPolls=$diag.audioPolls
        syntheticDpi=144; uiaDescendants=$uiaChildren.Count; uiaInvokePassed=$true
        volumeRangeMin=$volumeRangeMin; volumeRangeMax=$volumeRangeMax
        volumeRangeSetPassed=$true; volumeOutOfRangeRejected=$invalidRangeRejected
        msaaNamesRolesFocusBoundsValuePassed=$true; staleProviderRejected=$true
        scaledPointerClickPassed=$true; collapsedChildrenReleased=$true
    }
} finally {
    $closePosted=$false
    $shutdownWaitMs=0
    $owned.Refresh()
    if(!$owned.HasExited) {
        $closePosted=$true
        if($mainHwnd -eq [IntPtr]::Zero){$mainHwnd=[NativeAccessCheck]::FindPrototype($owned.Id)}
        [NativeAccessCheck]::RequirePrototype($mainHwnd,$owned.Id)
        $shutdown=[System.Diagnostics.Stopwatch]::StartNew()
        [NativeAccessCheck]::PostChecked($mainHwnd,0x10,[IntPtr]::Zero,[IntPtr]::Zero)
        if(!$owned.WaitForExit(5000)) {
            $shutdown.Stop()
            $timeoutEvidence=[ordered]@{ownedPid=$owned.Id;mainHwnd=$mainHwnd.ToInt64();mainClass='IsleNativePrototype';gracefulClosePosted=$true;postTargetValidated=$true;shutdownWaitMs=5000;forcedKill=$false;binarySha256=(Get-FileHash $testExe -Algorithm SHA256).Hash}
            $timeoutEvidence | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $nativeRoot 'artifacts/accessibility-timeout.json')
            throw 'Owned accessibility process did not exit after WM_CLOSE within 5000 ms; no forced kill was issued'
        }
        $shutdown.Stop()
        $shutdownWaitMs=$shutdown.ElapsedMilliseconds
    }
    $owned.Refresh()
    if(!$closePosted) { throw 'Owned accessibility process exited before the WM_CLOSE cleanup phase' }
    if(!$owned.HasExited) { throw 'Owned accessibility process remained alive after WM_CLOSE' }
    $exitCode=$owned.ExitCode
    if($exitCode -ne 0) { throw "Owned accessibility process exited with code $exitCode" }
    if($null -ne $evidence) {
        $evidence['ownedExitCode']=$exitCode
        $evidence['mainHwndValidated']=($mainHwnd -ne [IntPtr]::Zero)
        $evidence['gracefulClosePosted']=$closePosted
        $evidence['shutdownWaitMs']=$shutdownWaitMs
        $evidence['forcedKill']=$false
        New-Item -ItemType Directory -Force (Join-Path $nativeRoot 'artifacts')|Out-Null
        $evidence | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $nativeRoot 'artifacts/accessibility-results.json')
    }
}
