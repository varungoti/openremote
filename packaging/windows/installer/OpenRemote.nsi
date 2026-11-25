; OpenRemote per-user, offline Windows host installer.
; Build only through scripts/build-windows-installer.py: its generated definitions
; bind the GUI host to the source/evidence hash and construct the payload.
Unicode true
ManifestDPIAware true
ManifestSupportedOS all
RequestExecutionLevel user
SetCompressor /SOLID lzma
SetDatablockOptimize on
SetDateSave off
CRCCheck force
ShowInstDetails show
ShowUninstDetails show

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "Sections.nsh"
!include "WordFunc.nsh"
!include "WinVer.nsh"
!include "Win\COM.nsh"
!include "x64.nsh"
!include "package-definitions.nsh"

!define OWNER "OpenRemote.NativeSetup.2026.1"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenRemote"

Name "OpenRemote ${VERSION}"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\OpenRemote"
BrandingText "OpenRemote · Windows host"
VIProductVersion "${VERSION}.0"
VIAddVersionKey /LANG=1033 "ProductName" "OpenRemote Windows Host Setup"
VIAddVersionKey /LANG=1033 "ProductVersion" "${VERSION}"
VIAddVersionKey /LANG=1033 "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=1033 "FileDescription" "OpenRemote per-user development installer"
VIAddVersionKey /LANG=1033 "LegalCopyright" "OpenRemote project contributors"

Var InstallRoot
Var ReleaseDir
Var HostLock
Var ErrorText
Var OldOwner
Var OldRelease
Var OldName
Var OldVersion
Var OldIcon
Var OldUninstall
Var OldPublisher
Var OldHostShortcut
Var OldSettingsShortcut
Var OldDesktopShortcut
Var OldDesktopInstalled
Var ActivationStarted
Var StageCreated
Var RollbackFailed
Var DesktopSelected
Var DesktopTouched
Var CheckPath
Var ShortcutPath
Var ShortcutTarget
Var ShortcutArguments
Var ShortcutMatches
Var ComInitialized

!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE "Install OpenRemote on this PC"
!define MUI_WELCOMEPAGE_TEXT "OpenRemote includes its screen-sharing runtime.$\r$\n$\r$\nOpen the app, note Your PC ID and choose Activate Connection. Enter a six-digit PIN, then choose the same action again. Your phone or laptop needs only the PC ID and PIN.$\r$\n$\r$\n${SERVICE_NOTE}$\r$\n$\r$\nThis development build is unsigned. Your existing PC identity and settings are preserved."
!define MUI_COMPONENTSPAGE_TEXT_TOP "Install the host and choose whether to add a desktop shortcut. No service or automatic startup is installed."
!define MUI_FINISHPAGE_TITLE "OpenRemote is installed"
!define MUI_FINISHPAGE_TEXT "Open OpenRemote and choose Activate Connection. Enter your six-digit PIN, then choose the same action again.$\r$\n$\r$\nUse Your PC ID and PIN on your phone or laptop. Open the tray icon to see connections or stop access.$\r$\n$\r$\n${SERVICE_NOTE}"
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_TEXT "Open OpenRemote"
!define MUI_FINISHPAGE_RUN_FUNCTION "LaunchHost"
!define MUI_FINISHPAGE_SHOWREADME "$ReleaseDir\START-HERE.txt"
!define MUI_FINISHPAGE_SHOWREADME_TEXT "Read the setup guide"
!define MUI_FINISHPAGE_SHOWREADME_NOTCHECKED
!define MUI_UNCONFIRMPAGE_TEXT_TOP "Remove this installed OpenRemote host? Your PC identity, settings, received files and previous release directories are kept. Server enrollment is not revoked."

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH
!insertmacro MUI_LANGUAGE "English"

Section "OpenRemote Host (required)" SEC_HOST
  SectionIn RO
SectionEnd

Section /o "Desktop shortcut" SEC_DESKTOP
SectionEnd

Section "-Install"
  StrCpy $InstallRoot "$LOCALAPPDATA\Programs\OpenRemote"
  StrCpy $INSTDIR $InstallRoot
  StrCpy $ErrorText "Could not prepare the application folder. No active installation was changed."
  Call CheckInstallDirectories
  IfErrors install_failed
  Call AcquireHostLock
  IfErrors install_failed
  ClearErrors
  CreateDirectory "$InstallRoot\releases"
  IfErrors install_failed
  ; The payload always goes into a new directory. Existing executables are never
  ; overwritten; an activation failure can retain the old release unchanged.
  GetTempFileName $0 "$InstallRoot\releases"
  IfErrors install_failed
  Delete "$0"
  IfErrors install_failed
  ${GetFileName} "$0" $1
  StrCpy $ReleaseDir "$InstallRoot\releases\${VERSION}-${HOST_HASH_SHORT}-$1"
  IfFileExists "$ReleaseDir" install_failed
  CreateDirectory "$ReleaseDir"
  IfErrors install_failed
  StrCpy $StageCreated 1
  SetOutPath "$ReleaseDir"
  StrCpy $ErrorText "OpenRemote could not copy its application files. The previous installation was preserved. Check free space and folder permissions, then run Setup again."
  SetOverwrite off
  File "payload\openremote-agent.exe"
  File "payload\START-HERE.txt"
  File "payload\THIRD-PARTY-NOTICES.txt"
  File "payload\package-manifest.json"
  File "payload\ffmpeg.exe"
  File "payload\FFmpeg-9.0.2-source.tar.xz"
  File "payload\FFmpeg-BUILD.txt"
  IfErrors install_failed
  WriteINIStr "$ReleaseDir\install-owner.ini" "OpenRemote" "Owner" "${OWNER}"
  WriteINIStr "$ReleaseDir\install-owner.ini" "OpenRemote" "Version" "${VERSION}"
  WriteINIStr "$ReleaseDir\install-owner.ini" "OpenRemote" "HostSHA256" "${HOST_SHA256}"
  WriteUninstaller "$ReleaseDir\Uninstall.exe"
  IfErrors install_failed
  Call BackupActivation
  IfErrors install_failed
  StrCpy $ActivationStarted 1
  ClearErrors
  CreateDirectory "$SMPROGRAMS\OpenRemote"
  CreateShortCut "$SMPROGRAMS\OpenRemote\OpenRemote Host.lnk" "$ReleaseDir\openremote-agent.exe" "" "$ReleaseDir\openremote-agent.exe" 0 SW_SHOWNORMAL "" "OpenRemote visible desktop host"
  StrCpy $ErrorText "OpenRemote could not create its shortcuts. Setup will restore the previous shortcuts and registration."
  IfErrors install_failed
  ; Remove only the previous install's verified Settings shortcut. New builds
  ; have no technical settings window. BackupActivation preserves rollback.
  ${If} $OldSettingsShortcut == 1
  ${AndIf} $OldRelease != ""
    StrCpy $ShortcutPath "$SMPROGRAMS\OpenRemote\OpenRemote Settings.lnk"
    StrCpy $ShortcutTarget "$OldRelease\openremote-agent.exe"
    StrCpy $ShortcutArguments "--configure"
    Call ShortcutPointsToHost
    ${If} $ShortcutMatches == 1
      Delete "$ShortcutPath"
    ${EndIf}
  ${EndIf}
  IfErrors install_failed
  ${If} $DesktopSelected == 1
    StrCpy $DesktopTouched 1
    CreateShortCut "$DESKTOP\OpenRemote.lnk" "$ReleaseDir\openremote-agent.exe" "" "$ReleaseDir\openremote-agent.exe" 0 SW_SHOWNORMAL "" "OpenRemote visible desktop host"
  ${ElseIf} $OldDesktopInstalled == 1
    StrCpy $ShortcutPath "$DESKTOP\OpenRemote.lnk"
    StrCpy $ShortcutTarget "$OldRelease\openremote-agent.exe"
    StrCpy $ShortcutArguments ""
    Call ShortcutPointsToHost
    ${If} $ShortcutMatches == 1
      StrCpy $DesktopTouched 1
      Delete "$DESKTOP\OpenRemote.lnk"
    ${EndIf}
  ${EndIf}
  IfErrors install_failed
  WriteINIStr "$ReleaseDir\install-owner.ini" "OpenRemote" "DesktopShortcutInstalled" "$DesktopSelected"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Owner" "${OWNER}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "OpenRemote"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$ReleaseDir\openremote-agent.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$ReleaseDir"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '$\"$ReleaseDir\Uninstall.exe$\"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "OpenRemote"
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "DesktopShortcutInstalled" $DesktopSelected
  StrCpy $ErrorText "OpenRemote could not register the installation for this Windows account. Setup will restore the previous shortcuts and registration."
  IfErrors install_failed
  DetailPrint "Installed ${VERSION} for the current Windows account."
  DetailPrint "PC identity, settings and received files were preserved."
  DetailPrint "The runtime is included; normal use asks only for the PC ID and six-digit PIN."
  Goto install_done

install_failed:
  Call RollbackActivation
  Call ReleaseHostLock
  SetErrorLevel 1
  MessageBox MB_OK|MB_ICONSTOP "$ErrorText"
  Abort
install_done:
  Call ReleaseHostLock
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
!insertmacro MUI_DESCRIPTION_TEXT ${SEC_HOST} "Install the visible Windows host and Start menu entries."
!insertmacro MUI_DESCRIPTION_TEXT ${SEC_DESKTOP} "Add or update the OpenRemote shortcut on your desktop."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Function .onInit
  SetShellVarContext current
  SetRegView 64
  StrCpy $HostLock 0
  StrCpy $ActivationStarted 0
  StrCpy $StageCreated 0
  StrCpy $RollbackFailed 0
  StrCpy $DesktopTouched 0
  StrCpy $ReleaseDir ""
  StrCpy $InstallRoot "$LOCALAPPDATA\Programs\OpenRemote"
  StrCpy $INSTDIR $InstallRoot
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_OK|MB_ICONSTOP "OpenRemote requires Windows 10 version 1703 (build 15063) or newer, or Windows 11."
    SetErrorLevel 2
    Quit
  ${EndIf}
  ; Direct DPI API imports and per-monitor-v2 require Windows 10 version 1703.
  ; Reject older builds before the loader can fail on a missing host API entry.
  ${IfNot} ${AtLeastBuild} 15063
    MessageBox MB_OK|MB_ICONSTOP "This Windows build is too old for OpenRemote. Windows 10 version 1703 (build 15063) or newer, or Windows 11, is required. No application files were installed."
    SetErrorLevel 2
    Quit
  ${EndIf}
  ${IfNot} ${IsNativeAMD64}
    MessageBox MB_OK|MB_ICONSTOP "This installer is for Intel/AMD 64-bit Windows PCs. A different build is required for 32-bit Windows or Windows on ARM."
    SetErrorLevel 2
    Quit
  ${EndIf}
  IfSilent silent_install normal_install
silent_install:
  SetErrorLevel 2
  Quit
normal_install:
  InitPluginsDir
  ; Do not adopt or overwrite an unrelated uninstall registration.
  ReadRegStr $OldOwner HKCU "${UNINSTALL_KEY}" "Owner"
  ReadRegStr $0 HKCU "${UNINSTALL_KEY}" "DisplayName"
  ${If} $0 != ""
  ${AndIf} $OldOwner != "${OWNER}"
    MessageBox MB_OK|MB_ICONSTOP "An installation using the OpenRemote registration name already exists. Its ownership could not be verified. No changes were made."
    SetErrorLevel 2
    Quit
  ${EndIf}
  ${If} $OldOwner == "${OWNER}"
    ReadRegDWORD $0 HKCU "${UNINSTALL_KEY}" "DesktopShortcutInstalled"
    ${If} $0 == 1
      ReadRegStr $1 HKCU "${UNINSTALL_KEY}" "InstallLocation"
      StrCpy $ShortcutPath "$DESKTOP\OpenRemote.lnk"
      StrCpy $ShortcutTarget "$1\openremote-agent.exe"
      StrCpy $ShortcutArguments ""
      Call ShortcutPointsToHost
      ${If} $ShortcutMatches == 1
        SectionSetFlags ${SEC_DESKTOP} ${SF_SELECTED}
      ${EndIf}
    ${EndIf}
  ${EndIf}
FunctionEnd

; Refuse directory junctions/symlinks at paths the installer will modify. This
; check is additional defense for same-user paths, not an elevation boundary.
Function CheckDirectory
  System::Call 'kernel32::GetFileAttributesW(w "$CheckPath") i .r0'
  ${If} $0 != -1
    IntOp $1 $0 & 0x400
    ${If} $1 != 0
      SetErrors
      Return
    ${EndIf}
    IntOp $1 $0 & 0x10
    ${If} $1 == 0
      SetErrors
      Return
    ${EndIf}
  ${EndIf}
  ClearErrors
FunctionEnd

Function CheckInstallDirectories
  StrCpy $CheckPath "$LOCALAPPDATA\Programs"
  Call CheckDirectory
  IfErrors check_done
  StrCpy $CheckPath "$InstallRoot"
  Call CheckDirectory
  IfErrors check_done
  StrCpy $CheckPath "$InstallRoot\releases"
  Call CheckDirectory
  IfErrors check_done
  StrCpy $CheckPath "$LOCALAPPDATA\OpenRemote"
  Call CheckDirectory
  IfErrors check_done
  StrCpy $CheckPath "$SMPROGRAMS\OpenRemote"
  Call CheckDirectory
check_done:
FunctionEnd

!macro HOST_LOCK_FUNCTIONS PREFIX
Function ${PREFIX}AcquireHostLock
  ClearErrors
  CreateDirectory "$LOCALAPPDATA\OpenRemote"
  IfErrors lock_failed
  System::Call 'kernel32::GetFileAttributesW(w "$LOCALAPPDATA\OpenRemote\host-instance.lock") i .r0'
  ${If} $0 != -1
    IntOp $1 $0 & 0x410
    ${If} $1 != 0
      Goto lock_failed
    ${EndIf}
  ${EndIf}
  ; GENERIC_READ|GENERIC_WRITE, no sharing, OPEN_ALWAYS, OPEN_REPARSE_POINT.
  ; The host uses this exact lock before opening any settings window.
  System::Call 'kernel32::CreateFileW(w "$LOCALAPPDATA\OpenRemote\host-instance.lock", i 0xC0000000, i 0, p 0, i 4, i 0x00200080, p 0) p .r0'
  ${If} $0 P= -1
  ${OrIf} $0 P= 0
    Goto lock_failed
  ${EndIf}
  StrCpy $HostLock $0
  ClearErrors
  Return
lock_failed:
  StrCpy $ErrorText "OpenRemote is already running, another setup is in progress, or its local lock cannot be opened. Close OpenRemote by choosing Quit from the tray, or deactivate and close the inactive window, close any other setup window, and try again. No running program will be terminated."
  SetErrors
FunctionEnd

Function ${PREFIX}ReleaseHostLock
  ${If} $HostLock P<> 0
    System::Call 'kernel32::CloseHandle(p $HostLock)'
    StrCpy $HostLock 0
  ${EndIf}
FunctionEnd
!macroend
!insertmacro HOST_LOCK_FUNCTIONS ""
!insertmacro HOST_LOCK_FUNCTIONS "un."

Function BackupActivation
  StrCpy $ErrorText "Could not back up the current shortcuts. The previous installation was preserved."
  StrCpy $OldHostShortcut 0
  StrCpy $OldSettingsShortcut 0
  StrCpy $OldDesktopShortcut 0
  StrCpy $OldDesktopInstalled 0
  SectionGetFlags ${SEC_DESKTOP} $0
  IntOp $DesktopSelected $0 & ${SF_SELECTED}
  ClearErrors
  IfFileExists "$SMPROGRAMS\OpenRemote\OpenRemote Host.lnk" 0 host_backup_done
    CopyFiles /SILENT "$SMPROGRAMS\OpenRemote\OpenRemote Host.lnk" "$PLUGINSDIR\host.lnk"
    IfErrors backup_done
    StrCpy $OldHostShortcut 1
host_backup_done:
  IfFileExists "$SMPROGRAMS\OpenRemote\OpenRemote Settings.lnk" 0 settings_backup_done
    CopyFiles /SILENT "$SMPROGRAMS\OpenRemote\OpenRemote Settings.lnk" "$PLUGINSDIR\settings.lnk"
    IfErrors backup_done
    StrCpy $OldSettingsShortcut 1
settings_backup_done:
  ; Re-read registration under the exclusive host/install lock. Another setup
  ; may have completed while this window was on its welcome page.
  ReadRegStr $OldOwner HKCU "${UNINSTALL_KEY}" "Owner"
  ReadRegStr $OldName HKCU "${UNINSTALL_KEY}" "DisplayName"
  ${If} $OldName != ""
  ${AndIf} $OldOwner != "${OWNER}"
    StrCpy $ErrorText "The OpenRemote registration changed while Setup was open. Its ownership could not be verified. No active installation was changed."
    SetErrors
    Return
  ${EndIf}
  ${If} $OldOwner == "${OWNER}"
    ClearErrors
  ReadRegStr $OldRelease HKCU "${UNINSTALL_KEY}" "InstallLocation"
  ReadRegStr $OldName HKCU "${UNINSTALL_KEY}" "DisplayName"
  ReadRegStr $OldVersion HKCU "${UNINSTALL_KEY}" "DisplayVersion"
  ReadRegStr $OldIcon HKCU "${UNINSTALL_KEY}" "DisplayIcon"
  ReadRegStr $OldUninstall HKCU "${UNINSTALL_KEY}" "UninstallString"
  ReadRegStr $OldPublisher HKCU "${UNINSTALL_KEY}" "Publisher"
    IfErrors backup_done
    ReadRegDWORD $OldDesktopInstalled HKCU "${UNINSTALL_KEY}" "DesktopShortcutInstalled"
    ${If} ${Errors}
      StrCpy $OldDesktopInstalled 0
    ${EndIf}
    ${VersionCompare} "$OldVersion" "${VERSION}" $0
    ${If} $0 == 1
      StrCpy $ErrorText "A newer OpenRemote version is installed. This older installer will not replace it."
      SetErrors
      Return
    ${EndIf}
  ${EndIf}
  ClearErrors
  ${If} $DesktopSelected == 1
  ${OrIf} $OldDesktopInstalled == 1
    IfFileExists "$DESKTOP\OpenRemote.lnk" 0 backup_done
      ${If} $DesktopSelected == 1
        StrCpy $ShortcutPath "$DESKTOP\OpenRemote.lnk"
        StrCpy $ShortcutTarget "$OldRelease\openremote-agent.exe"
        StrCpy $ShortcutArguments ""
        Call ShortcutPointsToHost
        ${If} $OldDesktopInstalled != 1
        ${OrIf} $ShortcutMatches != 1
          StrCpy $ErrorText "A desktop shortcut named OpenRemote already exists and does not belong to the previous installation. It was preserved. Run Setup again with Desktop shortcut cleared, or rename that shortcut first."
          SetErrors
          Return
        ${EndIf}
      ${EndIf}
      CopyFiles /SILENT "$DESKTOP\OpenRemote.lnk" "$PLUGINSDIR\desktop.lnk"
      IfErrors backup_done
      StrCpy $OldDesktopShortcut 1
  ${EndIf}
backup_done:
FunctionEnd

Function RollbackActivation
  ${If} $ActivationStarted == 1
    ClearErrors
    ${If} $OldHostShortcut == 1
      CopyFiles /SILENT "$PLUGINSDIR\host.lnk" "$SMPROGRAMS\OpenRemote\OpenRemote Host.lnk"
    ${Else}
      Delete "$SMPROGRAMS\OpenRemote\OpenRemote Host.lnk"
    ${EndIf}
    ${If} $OldSettingsShortcut == 1
      CopyFiles /SILENT "$PLUGINSDIR\settings.lnk" "$SMPROGRAMS\OpenRemote\OpenRemote Settings.lnk"
    ${Else}
      Delete "$SMPROGRAMS\OpenRemote\OpenRemote Settings.lnk"
    ${EndIf}
    ${If} $DesktopTouched == 1
      ${If} $OldDesktopShortcut == 1
        CopyFiles /SILENT "$PLUGINSDIR\desktop.lnk" "$DESKTOP\OpenRemote.lnk"
      ${Else}
        Delete "$DESKTOP\OpenRemote.lnk"
      ${EndIf}
    ${EndIf}
    ${If} $OldOwner == "${OWNER}"
      WriteRegStr HKCU "${UNINSTALL_KEY}" "Owner" "$OldOwner"
      WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "$OldName"
      WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "$OldVersion"
      WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$OldIcon"
      WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$OldRelease"
      WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" "$OldUninstall"
      WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "$OldPublisher"
      WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
      WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
      WriteRegDWORD HKCU "${UNINSTALL_KEY}" "DesktopShortcutInstalled" $OldDesktopInstalled
    ${Else}
      DeleteRegKey HKCU "${UNINSTALL_KEY}"
    ${EndIf}
    ${If} ${Errors}
      StrCpy $RollbackFailed 1
      StrCpy $ErrorText "$ErrorText$\r$\n$\r$\nWindows also prevented complete rollback. The previous release files remain in place. Check the Start menu and Installed apps entry before retrying."
    ${EndIf}
  ${EndIf}
  ; A failed staging attempt removes only the exact files this installer owns.
  ; No recursive directory removal or deletion of the state directory occurs.
  ${If} $StageCreated == 1
  ${AndIf} $RollbackFailed != 1
    SetOutPath "$TEMP"
    Delete "$ReleaseDir\openremote-agent.exe"
    Delete "$ReleaseDir\START-HERE.txt"
    Delete "$ReleaseDir\THIRD-PARTY-NOTICES.txt"
    Delete "$ReleaseDir\package-manifest.json"
    Delete "$ReleaseDir\ffmpeg.exe"
    Delete "$ReleaseDir\FFmpeg-9.0.2-source.tar.xz"
    Delete "$ReleaseDir\FFmpeg-BUILD.txt"
    Delete "$ReleaseDir\install-owner.ini"
    Delete "$ReleaseDir\Uninstall.exe"
    RMDir "$ReleaseDir"
  ${EndIf}
FunctionEnd

Function LaunchHost
  Call ReleaseHostLock
  ClearErrors
  ExecShell "open" "$ReleaseDir\openremote-agent.exe"
  ${If} ${Errors}
    MessageBox MB_OK|MB_ICONEXCLAMATION "OpenRemote was installed but Windows did not start it. Open OpenRemote Host from the Start menu. If Windows shows a publisher warning, this is an unsigned development build."
  ${EndIf}
FunctionEnd

Function .onGUIEnd
  Call ReleaseHostLock
FunctionEnd

Function un.onInit
  SetShellVarContext current
  SetRegView 64
  StrCpy $HostLock 0
  StrCpy $InstallRoot "$LOCALAPPDATA\Programs\OpenRemote"
  GetFullPathName $ReleaseDir "$INSTDIR"
  ${GetParent} "$ReleaseDir" $0
  ${If} $0 != "$InstallRoot\releases"
    MessageBox MB_OK|MB_ICONSTOP "This uninstaller is not in its owned OpenRemote release directory. No files were removed."
    SetErrorLevel 2
    Quit
  ${EndIf}
  StrCpy $INSTDIR $ReleaseDir
  ; Check both directory levels without following a release-folder junction.
  System::Call 'kernel32::GetFileAttributesW(w "$ReleaseDir") i .r0'
  IntOp $1 $0 & 0x400
  ${If} $0 == -1
  ${OrIf} $1 != 0
    Goto un_invalid
  ${EndIf}
  System::Call 'kernel32::GetFileAttributesW(w "$InstallRoot\releases") i .r0'
  IntOp $1 $0 & 0x400
  ${If} $0 == -1
  ${OrIf} $1 != 0
    Goto un_invalid
  ${EndIf}
  System::Call 'kernel32::GetFileAttributesW(w "$InstallRoot") i .r0'
  IntOp $1 $0 & 0x400
  ${If} $0 == -1
  ${OrIf} $1 != 0
    Goto un_invalid
  ${EndIf}
  System::Call 'kernel32::GetFileAttributesW(w "$LOCALAPPDATA\Programs") i .r0'
  IntOp $1 $0 & 0x400
  ${If} $0 == -1
  ${OrIf} $1 != 0
    Goto un_invalid
  ${EndIf}
  ReadINIStr $0 "$ReleaseDir\install-owner.ini" "OpenRemote" "Owner"
  ReadINIStr $1 "$ReleaseDir\install-owner.ini" "OpenRemote" "HostSHA256"
  ${If} $0 != "${OWNER}"
  ${OrIf} $1 != "${HOST_SHA256}"
    Goto un_invalid
  ${EndIf}
  System::Call 'kernel32::GetFileAttributesW(w "$LOCALAPPDATA\OpenRemote") i .r0'
  ${If} $0 != -1
    IntOp $1 $0 & 0x400
    ${If} $1 != 0
      Goto un_invalid
    ${EndIf}
  ${EndIf}
  Call un.AcquireHostLock
  ${If} ${Errors}
    MessageBox MB_OK|MB_ICONSTOP "$ErrorText"
    SetErrorLevel 2
    Quit
  ${EndIf}
  Return
un_invalid:
  MessageBox MB_OK|MB_ICONSTOP "The OpenRemote installation path or ownership marker could not be verified. No files were removed."
  SetErrorLevel 2
  Quit
FunctionEnd

Section "Uninstall"
  SetOutPath "$TEMP"
  ClearErrors
  Delete "$INSTDIR\openremote-agent.exe"
  Delete "$INSTDIR\START-HERE.txt"
  Delete "$INSTDIR\THIRD-PARTY-NOTICES.txt"
  Delete "$INSTDIR\package-manifest.json"
  Delete "$INSTDIR\ffmpeg.exe"
  Delete "$INSTDIR\FFmpeg-9.0.2-source.tar.xz"
  Delete "$INSTDIR\FFmpeg-BUILD.txt"
  ${If} ${Errors}
    Call un.ReleaseHostLock
    MessageBox MB_OK|MB_ICONSTOP "Windows could not remove one or more OpenRemote application files. Close any window using those files and run uninstall again. Your settings and received files were kept."
    SetErrorLevel 1
    Abort
  ${EndIf}
  ; An old release's uninstaller must never remove a newer release's active
  ; shortcuts or Installed apps registration.
  ReadRegStr $0 HKCU "${UNINSTALL_KEY}" "Owner"
  ReadRegStr $1 HKCU "${UNINSTALL_KEY}" "InstallLocation"
  ${If} $0 == "${OWNER}"
  ${AndIf} $1 == "$INSTDIR"
    StrCpy $ShortcutTarget "$INSTDIR\openremote-agent.exe"
    StrCpy $ShortcutArguments ""
    StrCpy $ShortcutPath "$SMPROGRAMS\OpenRemote\OpenRemote Host.lnk"
    Call un.ShortcutPointsToHost
    ${If} $ShortcutMatches == 1
      Delete "$ShortcutPath"
    ${EndIf}
    StrCpy $ShortcutArguments "--configure"
    StrCpy $ShortcutPath "$SMPROGRAMS\OpenRemote\OpenRemote Settings.lnk"
    Call un.ShortcutPointsToHost
    ${If} $ShortcutMatches == 1
      Delete "$ShortcutPath"
    ${EndIf}
    ReadINIStr $0 "$INSTDIR\install-owner.ini" "OpenRemote" "DesktopShortcutInstalled"
    ${If} $0 == 1
      StrCpy $ShortcutArguments ""
      StrCpy $ShortcutPath "$DESKTOP\OpenRemote.lnk"
      Call un.ShortcutPointsToHost
      ${If} $ShortcutMatches == 1
        Delete "$ShortcutPath"
      ${EndIf}
    ${EndIf}
    RMDir "$SMPROGRAMS\OpenRemote"
    DeleteRegKey HKCU "${UNINSTALL_KEY}"
  ${EndIf}
  Delete "$INSTDIR\install-owner.ini"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  RMDir "$InstallRoot\releases"
  RMDir "$InstallRoot"
  DetailPrint "Removed this OpenRemote release. Local identity, settings, received files and previous release directories were kept."
  Call un.ReleaseHostLock
SectionEnd

Function un.onGUIEnd
  Call un.ReleaseHostLock
FunctionEnd

; Read only: never resolve, relink or execute a shortcut. Delete a shortcut on
; upgrade/uninstall only when its saved target AND arguments still belong to us.
; A user-edited shortcut or a failed COM lookup is preserved.
!macro SHORTCUT_FUNCTION PREFIX
Function ${PREFIX}ShortcutPointsToHost
  StrCpy $ShortcutMatches 0
  StrCpy $ComInitialized 0
  System::Call 'ole32::CoInitializeEx(p 0, i 2) i .r5'
  ${If} $5 >= 0
    StrCpy $ComInitialized 1
  ${EndIf}
  StrCpy $0 0
  StrCpy $1 0
  !insertmacro ComHlpr_CreateInProcInstance ${CLSID_ShellLink} ${IID_IShellLink} r0 ".r2"
  ${If} $0 P<> 0
    ${IUnknown::QueryInterface} $0 '("${IID_IPersistFile}",.r1).r2'
    ${If} $1 P<> 0
      ${IPersistFile::Load} $1 '("$ShortcutPath",0).r2'
      ${If} $2 == 0
        ; SLGP_RAWPATH=4 avoids resolution and network/target searching.
        ${IShellLink::GetPath} $0 '(.r3,${NSIS_MAX_STRLEN},0,4).r2'
        ${If} $2 == 0
        ${AndIf} $3 == "$ShortcutTarget"
          ${IShellLink::GetArguments} $0 '(.r4,${NSIS_MAX_STRLEN}).r2'
          ${If} $2 == 0
          ${AndIf} $4 == "$ShortcutArguments"
            StrCpy $ShortcutMatches 1
          ${EndIf}
        ${EndIf}
      ${EndIf}
      ${IUnknown::Release} $1 ""
    ${EndIf}
    ${IUnknown::Release} $0 ""
  ${EndIf}
  ${If} $ComInitialized == 1
    System::Call 'ole32::CoUninitialize()'
  ${EndIf}
FunctionEnd
!macroend
!insertmacro SHORTCUT_FUNCTION ""
!insertmacro SHORTCUT_FUNCTION "un."
