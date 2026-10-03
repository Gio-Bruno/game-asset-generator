; Compiled locally by scripts/package.py. No network requests or administrative privileges.
Unicode true
RequestExecutionLevel user
SetCompressor /SOLID lzma
AllowSkipFiles off
!include "MUI2.nsh"
!include "x64.nsh"
!include "WinVer.nsh"

Name "Asset Forge"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\Asset Forge"
InstallDirRegKey HKCU "Software\AssetForge" "InstallLocation"
VIProductVersion "${APP_VERSION}.0"
VIAddVersionKey /LANG=1033 "ProductName" "Asset Forge"
VIAddVersionKey /LANG=1033 "ProductVersion" "${APP_VERSION}"
VIAddVersionKey /LANG=1033 "FileVersion" "${APP_VERSION}"
VIAddVersionKey /LANG=1033 "FileDescription" "Asset Forge per-user installer"
VIAddVersionKey /LANG=1033 "LegalCopyright" "Asset Forge contributors"

!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\asset-forge-studio.exe"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${PACKAGE_DIR}\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
    ${IfNot} ${RunningX64}
        MessageBox MB_OK|MB_ICONSTOP "Asset Forge requires Windows 10 or 11, 64-bit."
        Abort
    ${EndIf}
    ${IfNot} ${AtLeastWin10}
        MessageBox MB_OK|MB_ICONSTOP "Asset Forge requires Windows 10 or 11, 64-bit."
        Abort
    ${EndIf}
FunctionEnd

!macro CheckUnlockedExecutable NAME
    ${If} ${FileExists} "$INSTDIR\${NAME}"
        ; OPEN_EXISTING probes access without creating, truncating, or writing a file.
        System::Call 'kernel32::CreateFileW(w "$INSTDIR\${NAME}", i 0x40000000, i 0, p 0, i 3, i 0x80, p 0) p .r0'
        ${If} $0 == -1
            MessageBox MB_OK|MB_ICONSTOP "Quit Asset Forge and its command-line processes, then run this installer or uninstaller again. The existing program cannot be replaced or removed while it is open." /SD IDOK
            Pop $0
            Abort
        ${EndIf}
        System::Call 'kernel32::CloseHandle(p r0)'
    ${EndIf}
!macroend

!macro RequireClosedPrograms FUNCTION_PREFIX
Function ${FUNCTION_PREFIX}RequireClosedPrograms
    Push $0
    !insertmacro CheckUnlockedExecutable "asset-forge-studio.exe"
    !insertmacro CheckUnlockedExecutable "asset-forge.exe"
    Pop $0
FunctionEnd
!macroend

!insertmacro RequireClosedPrograms ""
!insertmacro RequireClosedPrograms "un."

Section "Asset Forge" SEC_MAIN
    SetShellVarContext current
    ; Check both EXEs before modifying any payload or registration.
    Call RequireClosedPrograms
    SetOutPath "$INSTDIR"
    File /r "${PACKAGE_DIR}\*"
    WriteUninstaller "$INSTDIR\Uninstall.exe"
    CreateDirectory "$SMPROGRAMS\Asset Forge"
    CreateShortcut "$SMPROGRAMS\Asset Forge\Asset Forge.lnk" "$INSTDIR\asset-forge-studio.exe"
    CreateShortcut "$SMPROGRAMS\Asset Forge\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
    CreateShortcut "$DESKTOP\Asset Forge.lnk" "$INSTDIR\asset-forge-studio.exe"
    WriteRegStr HKCU "Software\AssetForge" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AssetForge" "DisplayName" "Asset Forge"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AssetForge" "DisplayVersion" "${APP_VERSION}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AssetForge" "Publisher" "Asset Forge contributors"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AssetForge" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AssetForge" "DisplayIcon" "$INSTDIR\asset-forge-studio.exe"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AssetForge" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AssetForge" "NoModify" 1
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AssetForge" "NoRepair" 1
SectionEnd

Section "Uninstall"
    SetShellVarContext current
    Call un.RequireClosedPrograms
    ; Generated explicit file list avoids deleting projects, Codex login, or added user files.
    !include "${UNINSTALL_FILES}"
    Delete "$INSTDIR\Uninstall.exe"
    RMDir "$INSTDIR"
    Delete "$SMPROGRAMS\Asset Forge\Asset Forge.lnk"
    Delete "$SMPROGRAMS\Asset Forge\Uninstall.lnk"
    RMDir "$SMPROGRAMS\Asset Forge"
    Delete "$DESKTOP\Asset Forge.lnk"
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AssetForge"
    DeleteRegKey HKCU "Software\AssetForge"
SectionEnd
