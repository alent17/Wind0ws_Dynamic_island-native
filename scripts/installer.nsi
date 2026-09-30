Unicode true
!include "MUI2.nsh"
Name "Isle Native"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Isle"
InstallDirRegKey HKCU "Software\Isle" "InstallDir"
RequestExecutionLevel user
Icon "${ICON}"
UninstallIcon "${ICON}"
VIProductVersion "${VERSION}.0"
VIAddVersionKey /LANG=1033 "ProductName" "Isle Native"
VIAddVersionKey /LANG=1033 "FileDescription" "Isle Native Installer"
VIAddVersionKey /LANG=1033 "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "MIT License"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"
Section "Isle"
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File "${STAGE}\isle.exe"
  File "${STAGE}\LICENSE"
  File "${STAGE}\LUCIDE-LICENSE.txt"
  SetOutPath "$INSTDIR\fonts"
  File "${STAGE}\fonts\MiSans-Regular.ttf"
  File "${STAGE}\fonts\MiSans-Medium.ttf"
  File "${STAGE}\fonts\MiSans-Bold.ttf"
  SetOutPath "$INSTDIR"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateDirectory "$SMPROGRAMS\Isle"
  CreateShortcut "$SMPROGRAMS\Isle\Isle.lnk" "$INSTDIR\isle.exe"
  WriteRegStr HKCU "Software\Isle" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Isle" "DisplayName" "Isle Native"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Isle" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Isle" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Isle" "DisplayIcon" "$INSTDIR\isle.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Isle" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Isle" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Isle" "NoRepair" 1
SectionEnd
Section "Uninstall"
  SetShellVarContext current
  Delete "$INSTDIR\isle.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\LUCIDE-LICENSE.txt"
  Delete "$INSTDIR\fonts\MiSans-Regular.ttf"
  Delete "$INSTDIR\fonts\MiSans-Medium.ttf"
  Delete "$INSTDIR\fonts\MiSans-Bold.ttf"
  RMDir "$INSTDIR\fonts"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\Isle\Isle.lnk"
  RMDir "$SMPROGRAMS\Isle"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Isle"
  DeleteRegKey HKCU "Software\Isle"
SectionEnd
