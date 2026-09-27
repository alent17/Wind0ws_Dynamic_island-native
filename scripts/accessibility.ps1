# Run in Windows PowerShell (the .NET Framework Accessibility interop assembly).
$ErrorActionPreference='Stop'
Add-Type -AssemblyName Accessibility
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -ReferencedAssemblies Accessibility -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Threading;
using Accessibility;
public static class NativeAccessCheck {
 [StructLayout(LayoutKind.Sequential)] struct Point {public int x,y;}
 [DllImport("user32.dll")] static extern bool ScreenToClient(IntPtr h,ref Point p);
 [DllImport("oleacc.dll")] static extern int AccessibleObjectFromWindow(IntPtr h, uint id, ref Guid iid, [MarshalAs(UnmanagedType.Interface)] out IAccessible a);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
 static IAccessible Get(IntPtr h) { var iid=new Guid("618736e0-3c3d-11cf-810c-00aa00389b71"); IAccessible a; Marshal.ThrowExceptionForHR(AccessibleObjectFromWindow(h,0xfffffffc,ref iid,out a)); return a; }
 static void Require(bool value,string message) {if(!value)throw new Exception(message);}
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
  PostMessage(h,0x201,(IntPtr)1,packed);PostMessage(h,0x202,IntPtr.Zero,packed);Thread.Sleep(400);
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
  PostMessage(h,0x100,(IntPtr)0x1b,IntPtr.Zero);Thread.Sleep(150);
  Require(Get(h).accChildCount==0,"collapsed accessible children released");
  return "MSAA names, roles, invoke, focus, bounds, volume value, stale handles and collapse passed: "+String.Join(", ",names);
 }
}
'@
$nativeRoot=Split-Path $PSScriptRoot
$owned=Start-Process (Join-Path $nativeRoot 'target/release/isle-native.exe') -ArgumentList '--page','music','--paused','--reduced-motion','--test-dpi','144' -PassThru -WindowStyle Hidden
try {
    for($i=0;$i -lt 100;$i++) {
        $owned.Refresh()
        if($owned.MainWindowHandle -ne [IntPtr]::Zero){break}
        Start-Sleep -Milliseconds 50
    }
    Start-Sleep -Milliseconds 500
    $uiaRoot=[System.Windows.Automation.AutomationElement]::FromHandle($owned.MainWindowHandle)
    $uiaChildren=$uiaRoot.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
    $uiaNames=@($uiaChildren | ForEach-Object { $_.Current.Name })
    $volumeName=([char]0x97f3).ToString()+([char]0x91cf).ToString()
    if($uiaNames -notcontains $volumeName){throw "UIA legacy bridge did not expose volume: $($uiaNames -join ', ')"}
    Write-Output "UIA legacy bridge exposed $($uiaChildren.Count) descendants"
    $volumeElement=@($uiaChildren | Where-Object {$_.Current.Name -eq $volumeName})[0]
    ([System.Windows.Automation.InvokePattern]$volumeElement.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)).Invoke()
    Start-Sleep -Milliseconds 300
    [NativeAccessCheck]::BackFromVolume($owned.MainWindowHandle)
    Write-Output 'UIA InvokePattern navigation passed'
    [NativeAccessCheck]::Run($owned.MainWindowHandle)
    $evidence=[ordered]@{
        binarySha256=(Get-FileHash (Join-Path $nativeRoot 'target/release/isle-native.exe') -Algorithm SHA256).Hash
        syntheticDpi=144; uiaDescendants=$uiaChildren.Count; uiaInvokePassed=$true
        msaaNamesRolesFocusBoundsValuePassed=$true; staleProviderRejected=$true
        scaledPointerClickPassed=$true; collapsedChildrenReleased=$true
    }
    New-Item -ItemType Directory -Force (Join-Path $nativeRoot 'artifacts')|Out-Null
    $evidence | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $nativeRoot 'artifacts/accessibility-results.json')
} finally {
    if(!$owned.HasExited) {
        [NativeAccessCheck]::PostMessage($owned.MainWindowHandle,0x10,[IntPtr]::Zero,[IntPtr]::Zero)|Out-Null
        if(!$owned.WaitForExit(5000)){$owned.Kill()}
    }
}
