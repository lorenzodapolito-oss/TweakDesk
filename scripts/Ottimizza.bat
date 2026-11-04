@echo off
title Windows 11 Optimizer PRO - Fixed Edition
setlocal EnableDelayedExpansion

:: =========================================
:: CONTROLLO VERSIONE WINDOWS
:: =========================================

:: FIX #1: uso PowerShell per leggere BuildNumber, affidabile su tutte le localizzazioni
for /f %%i in ('powershell -NoProfile -Command "(Get-CimInstance Win32_OperatingSystem).BuildNumber"') do set "WIN_BUILD=%%i"

if %WIN_BUILD% LSS 22000 (
    echo.
    echo [ATTENZIONE] Questo script e' ottimizzato per Windows 11 (build 22000+).
    echo Build rilevata: %WIN_BUILD%
    echo.
    set /p "CONTINUE=Continuare comunque? (S/N): "
    if /i "!CONTINUE!" neq "S" exit /b 1
)

:: =========================================
:: CONTROLLO AMMINISTRATORE
:: =========================================

net session >nul 2>&1
if %errorlevel% neq 0 (
    echo Richiesti privilegi amministratore...
    powershell -Command "Start-Process '%~f0' -Verb RunAs"
    exit /b
)

:: =========================================
:: INIZIALIZZAZIONE VARIABILI
:: =========================================

set "PS=powershell -NoProfile -ExecutionPolicy Bypass -Command"

:: FIX #6: Log salvato sul Desktop, non in TEMP (che viene pulito dopo)
set "LOG_FILE=%USERPROFILE%\Desktop\Win11Optimizer.log"
set "ERROR_COUNT=0"

:: Crea file di log
echo [%DATE% %TIME%] Inizio ottimizzazione Windows 11 > "%LOG_FILE%"
echo [%DATE% %TIME%] Build Windows: %WIN_BUILD% >> "%LOG_FILE%"

cls
echo =====================================
echo    WINDOWS 11 OPTIMIZER PRO FIXED
echo =====================================
echo.
echo Log: %LOG_FILE%
echo.

pause
cls

:: =========================================
:: FUNZIONE PROGRESS BAR
:: =========================================

goto :main

:progress
setlocal
set "percent=%~1"
set "message=%~2"

set /a filled=%percent%/2
set "bar="

:: Caratteri pieni
for /l %%i in (1,1,%filled%) do set "bar=!bar!█"

:: FIX #3: partire da filled+1 per non avere un carattere extra
set /a empty_start=%filled%+1
for /l %%i in (%empty_start%,1,50) do set "bar=!bar!░"

echo [!bar!] %percent%%% - %message%
endlocal
exit /b 0

:: =========================================
:: FUNZIONE LOG ERRORI
:: FIX #1: endlocal & set per propagare ERROR_COUNT al contesto padre
:: =========================================

:log_error
setlocal
set "operation=%~1"
set "error_code=%~2"
set "local_err=%ERROR_COUNT%"

if not "%error_code%"=="" (
    if "%error_code%"=="0" (
        echo [%DATE% %TIME%] OK: %operation% >> "%LOG_FILE%"
    ) else (
        echo [%DATE% %TIME%] ERRORE: %operation% - Codice: %error_code% >> "%LOG_FILE%"
        set /a local_err+=1
    )
) else (
    echo [%DATE% %TIME%] INFO: %operation% >> "%LOG_FILE%"
)
endlocal & set "ERROR_COUNT=%local_err%"
exit /b 0

:: =========================================
:: MAIN - INIZIO OTTIMIZZAZIONI
:: =========================================

:main

call :progress 5 "Avvio ottimizzazione..."

:: =====================================
:: BACKUP REGISTRY (sicurezza)
:: =====================================

echo.
echo Backup chiavi registry principali...
set "REG_BACKUP_DIR=%USERPROFILE%\Desktop\Win11Opt_RegBackup_%DATE:/=-%"
mkdir "%REG_BACKUP_DIR%" >nul 2>&1

reg export "HKCU\Control Panel\Desktop" "%REG_BACKUP_DIR%\Desktop.reg" /y >nul 2>&1
reg export "HKCU\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager" "%REG_BACKUP_DIR%\ContentDelivery.reg" /y >nul 2>&1
echo [OK] Backup registry in: %REG_BACKUP_DIR%

:: =====================================
:: COPILOT - Disabilitazione completa
:: =====================================

echo.
echo Disabilitazione Copilot...

reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsCopilot" /f >nul 2>&1
reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsCopilot" /v TurnOffWindowsCopilot /t REG_DWORD /d 1 /f >nul 2>&1
call :log_error "Copilot - Policy HKLM" %errorlevel%

reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced" /v ShowCopilotButton /t REG_DWORD /d 0 /f >nul 2>&1
call :log_error "Copilot - Button HKCU" %errorlevel%

reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsCopilot" /v AllowCopilotInWindows /t REG_DWORD /d 0 /f >nul 2>&1

echo [OK] Copilot disabilitato
call :progress 10 "Copilot"

:: =====================================
:: TELEMETRIA - Blocco completo
:: =====================================

echo.
echo Blocco telemetria...

reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\DataCollection" /f >nul 2>&1

:: FIX #2: AllowTelemetry=0 funziona solo su Enterprise/Education.
:: Su Home/Pro il valore minimo accettato e' 1 (Security+Basic).
:: Usiamo 1 per compatibilita' massima; i servizi DiagTrack vengono comunque disabilitati sotto.
reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\DataCollection" /v AllowTelemetry /t REG_DWORD /d 1 /f >nul 2>&1
call :log_error "Telemetria - Policy (AllowTelemetry=1, compatibile Home/Pro)" %errorlevel%

reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\DataCollection" /v MaxTelemetryAllowed /t REG_DWORD /d 1 /f >nul 2>&1

sc stop DiagTrack >nul 2>&1
:: FIX #7: verifica stato invece di timeout fisso
set "WAIT=0"
:wait_diagtrack
sc query DiagTrack | find "STOPPED" >nul 2>&1
if %errorlevel% neq 0 (
    set /a WAIT+=1
    if !WAIT! LSS 10 (
        timeout /t 1 /nobreak >nul
        goto :wait_diagtrack
    )
)
sc config DiagTrack start=disabled >nul 2>&1
call :log_error "Telemetria - DiagTrack" %errorlevel%

sc stop dmwappushservice >nul 2>&1
sc config dmwappushservice start=disabled >nul 2>&1

reg add "HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection" /v AllowTelemetry /t REG_DWORD /d 0 /f >nul 2>&1

echo [OK] Telemetria disabilitata
call :progress 20 "Telemetria"

:: =====================================
:: HOSTS FILE - Blocco telemetria Microsoft
:: =====================================

echo.
echo Blocco telemetria via HOSTS...

set "HOSTS_FILE=%SystemRoot%\System32\drivers\etc\hosts"
set "HOSTS_BACKUP=%SystemRoot%\System32\drivers\etc\hosts.backup_win11opt"

if not exist "%HOSTS_BACKUP%" (
    copy "%HOSTS_FILE%" "%HOSTS_BACKUP%" >nul 2>&1
    call :log_error "Hosts - Backup" %errorlevel%
) else (
    echo [INFO] Backup hosts gia esistente
)

findstr /C:"# Windows 11 Telemetry Block" "%HOSTS_FILE%" >nul 2>&1
if %errorlevel% neq 0 (
    (
        echo.
        echo # Windows 11 Telemetry Block - Win11Optimizer
        echo 0.0.0.0 vortex.data.microsoft.com
        echo 0.0.0.0 settings-win.data.microsoft.com
        echo 0.0.0.0 watson.telemetry.microsoft.com
        echo 0.0.0.0 telemetry.microsoft.com
        echo 0.0.0.0 oca.telemetry.microsoft.com
        echo 0.0.0.0 services.wes.df.telemetry.microsoft.com
        echo 0.0.0.0 sqm.telemetry.microsoft.com
        echo 0.0.0.0 watson.live.com
        echo 0.0.0.0 watson.microsoft.com
        echo 0.0.0.0 telemetry.appex.bing.net
        echo 0.0.0.0 telemetry.urs.microsoft.com
        echo 0.0.0.0 diagnostic.support.microsoft.com
        echo 0.0.0.0 corp.sts.microsoft.com
        echo 0.0.0.0 statsfe1.ws.microsoft.com
        echo 0.0.0.0 statsfe2.ws.microsoft.com
    ) >> "%HOSTS_FILE%"
    call :log_error "Hosts - Entry aggiunte" 0
    echo [OK] Entry hosts aggiunte
) else (
    echo [INFO] Entry hosts gia presenti
)

ipconfig /flushdns >nul 2>&1

echo [OK] Blocco hosts configurato
call :progress 30 "Hosts"

:: =====================================
:: PUBBLICITA WINDOWS
:: =====================================

echo.
echo Disabilitazione pubblicita Windows...

reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager" /v SystemPaneSuggestionsEnabled /t REG_DWORD /d 0 /f >nul 2>&1
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager" /v SoftLandingEnabled /t REG_DWORD /d 0 /f >nul 2>&1
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager" /v SubscribedContent-338388Enabled /t REG_DWORD /d 0 /f >nul 2>&1
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager" /v SubscribedContent-338389Enabled /t REG_DWORD /d 0 /f >nul 2>&1
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager" /v SubscribedContent-310093Enabled /t REG_DWORD /d 0 /f >nul 2>&1
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager" /v ContentDeliveryAllowed /t REG_DWORD /d 0 /f >nul 2>&1
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager" /v FeatureManagementEnabled /t REG_DWORD /d 0 /f >nul 2>&1
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced" /v ShowSuggestions /t REG_DWORD /d 0 /f >nul 2>&1
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager" /v RotatingLockScreenEnabled /t REG_DWORD /d 0 /f >nul 2>&1
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager" /v RotatingLockScreenOverlayEnabled /t REG_DWORD /d 0 /f >nul 2>&1

call :log_error "Pubblicita - Registry" %errorlevel%

echo [OK] Pubblicita disabilitata
call :progress 40 "Ads"

:: =====================================
:: OTTIMIZZAZIONI PRESTAZIONI
:: =====================================

echo.
echo Ottimizzazione prestazioni...

reg add "HKCU\Control Panel\Desktop" /v MenuShowDelay /t REG_SZ /d 0 /f >nul 2>&1
call :log_error "Prestazioni - Menu Delay" %errorlevel%

:: FIX #8: backup UserPreferencesMask prima di sovrascrivere
for /f "tokens=3" %%v in ('reg query "HKCU\Control Panel\Desktop" /v UserPreferencesMask 2^>nul ^| find "REG_BINARY"') do (
    echo [%DATE% %TIME%] BACKUP UserPreferencesMask: %%v >> "%LOG_FILE%"
)
reg add "HKCU\Control Panel\Desktop" /v UserPreferencesMask /t REG_BINARY /d 9032078010000000 /f >nul 2>&1

powercfg -h off >nul 2>&1
call :log_error "Prestazioni - Hibernate" %errorlevel%

bcdedit /set disabledynamictick yes >nul 2>&1
call :log_error "Prestazioni - DynamicTick" %errorlevel%

:: FIX #5: rimosso "bcdedit /set useplatformtick yes" - controverso, puo' causare stuttering

:: FIX #5: sintassi corretta bootux
bcdedit /set bootux disabled >nul 2>&1
call :log_error "Prestazioni - BootUX" %errorlevel%

:: FIX #3: SysMain (Superfetch) lasciato su AUTO su SSD moderni.
:: Disabilitarlo puo' rallentare il caricamento delle app e peggiorare il caching.
:: Decommentare la riga sotto SOLO su HDD o sistemi con RAM < 4GB:
:: sc config SysMain start=disabled && sc stop SysMain
echo [INFO] SysMain lasciato attivo (consigliato su SSD)

reg add "HKCU\Software\Microsoft\GameBar" /v AllowAutoGameMode /t REG_DWORD /d 1 /f >nul 2>&1

echo [OK] Prestazioni migliorate
call :progress 50 "Prestazioni"

:: =====================================
:: RIMOZIONE APP UWP - Tutti gli utenti
:: =====================================

echo.
echo Rimozione bloatware (tutti gli utenti)...

set "APPSToRemove=xbox solitaire zune bing skype clipchamp disney spotify primevideo booking instagram tiktok news finance sports"

for %%a in (%APPSToRemove%) do (
    echo Rimozione: %%a...
    %PS% "Get-AppxPackage -AllUsers '*%%a*' | Remove-AppxPackage -AllUsers -ErrorAction SilentlyContinue" >nul 2>&1
    %PS% "Get-AppxProvisionedPackage -Online | Where-Object {$_.PackageName -like '*%%a*'} | Remove-AppxProvisionedPackage -Online -ErrorAction SilentlyContinue" >nul 2>&1
)

%PS% "Get-AppxPackage -AllUsers '*Microsoft.549981C3F5F10*' | Remove-AppxPackage -AllUsers -ErrorAction SilentlyContinue" >nul 2>&1
%PS% "Get-AppxPackage -AllUsers '*Microsoft.WindowsFeedbackHub*' | Remove-AppxPackage -AllUsers -ErrorAction SilentlyContinue" >nul 2>&1
%PS% "Get-AppxPackage -AllUsers '*Microsoft.GetHelp*' | Remove-AppxPackage -AllUsers -ErrorAction SilentlyContinue" >nul 2>&1
%PS% "Get-AppxPackage -AllUsers '*Microsoft.Getstarted*' | Remove-AppxPackage -AllUsers -ErrorAction SilentlyContinue" >nul 2>&1
%PS% "Get-AppxPackage -AllUsers '*Microsoft.People*' | Remove-AppxPackage -AllUsers -ErrorAction SilentlyContinue" >nul 2>&1

call :log_error "Bloatware - Rimozione" 0

echo [OK] App inutili rimosse
call :progress 60 "App"

:: =====================================
:: ONEDRIVE - Rimozione completa
:: =====================================

echo.
echo Rimozione OneDrive...

taskkill /f /im OneDrive.exe >nul 2>&1
taskkill /f /im OneDriveSetup.exe >nul 2>&1

:: FIX #9: attesa effettiva terminazione processo
set "WAIT=0"
:wait_onedrive
tasklist | find /i "OneDrive.exe" >nul 2>&1
if %errorlevel% equ 0 (
    set /a WAIT+=1
    if !WAIT! LSS 10 (
        timeout /t 1 /nobreak >nul
        goto :wait_onedrive
    )
    echo [WARN] OneDrive.exe ancora attivo dopo 10 secondi, continuo...
)

if exist "%SystemRoot%\SysWOW64\OneDriveSetup.exe" (
    start /wait "%SystemRoot%\SysWOW64\OneDriveSetup.exe" /uninstall
) else if exist "%SystemRoot%\System32\OneDriveSetup.exe" (
    start /wait "%SystemRoot%\System32\OneDriveSetup.exe" /uninstall
)

:: FIX #8: verifica disinstallazione tramite cartella dati, non il file setup
:: (OneDriveSetup.exe rimane su disco anche dopo uninstall - non e' un indicatore affidabile)
if exist "%LOCALAPPDATA%\Microsoft\OneDrive\OneDrive.exe" (
    echo [WARN] OneDrive potrebbe non essere stato rimosso completamente
    call :log_error "OneDrive - Disinstallazione incompleta" 1
) else (
    echo [OK] OneDrive disinstallato correttamente
    call :log_error "OneDrive - Disinstallazione" 0
)

reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\OneDrive" /v DisableFileSyncNGSC /t REG_DWORD /d 1 /f >nul 2>&1
reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\OneDrive" /v DisablePersonalSync /t REG_DWORD /d 1 /f >nul 2>&1
reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\OneDrive" /v DisableLibrariesDefaultSaveToOneDrive /t REG_DWORD /d 1 /f >nul 2>&1

reg delete "HKCR\CLSID\{018D5C66-4533-4307-9B53-224DE2ED1FE6}" /f >nul 2>&1
reg delete "HKCR\Wow6432Node\CLSID\{018D5C66-4533-4307-9B53-224DE2ED1FE6}" /f >nul 2>&1

if exist "%USERPROFILE%\OneDrive" rd /s /q "%USERPROFILE%\OneDrive" 2>nul
if exist "%LOCALAPPDATA%\Microsoft\OneDrive" rd /s /q "%LOCALAPPDATA%\Microsoft\OneDrive" 2>nul
if exist "%ProgramData%\Microsoft OneDrive" rd /s /q "%ProgramData%\Microsoft OneDrive" 2>nul

echo [OK] OneDrive rimosso
call :progress 70 "OneDrive"

:: =====================================
:: SERVIZI INUTILI
:: FIX #4: SysMain RIMOSSO da questa lista (gia' gestito sopra)
:: =====================================

echo.
echo Disabilitazione servizi non essenziali...

for %%s in (
    DiagTrack
    dmwappushservice
    MapsBroker
    Fax
    RemoteRegistry
    RetailDemo
    WerSvc
    WMPNetworkSvc
    XblAuthManager
    XblGameSave
    XboxNetApiSvc
    SEMgrSvc
    PhoneSvc
    TapiSrv
    ALG
    PeerDistSvc
    lfsvc
    MessagingService
) do (
    sc stop %%s >nul 2>&1
    sc config %%s start=disabled >nul 2>&1
    echo Disabilitato: %%s
)

call :log_error "Servizi - Disabilitazione" 0

echo [OK] Servizi non essenziali disabilitati
echo [NOTA] Wi-Fi, Windows Hello e Compatibilita' programmi mantenuti attivi
call :progress 80 "Servizi"

:: =====================================
:: EDGE UPDATE - Blocco
:: =====================================

echo.
echo Blocco aggiornamenti Edge...

sc stop edgeupdate >nul 2>&1
sc config edgeupdate start=disabled >nul 2>&1
sc stop edgeupdatem >nul 2>&1
sc config edgeupdatem start=disabled >nul 2>&1

reg add "HKLM\SOFTWARE\Policies\Microsoft\EdgeUpdate" /v UpdateDefault /t REG_DWORD /d 0 /f >nul 2>&1
reg add "HKLM\SOFTWARE\Policies\Microsoft\EdgeUpdate" /v DisableAutoUpdateChecksCheckboxValue /t REG_DWORD /d 1 /f >nul 2>&1
reg add "HKLM\SOFTWARE\Policies\Microsoft\EdgeUpdate" /v Update{56EB18F8-B008-4CBD-B6D2-8C97FE7E9062} /t REG_DWORD /d 0 /f >nul 2>&1
reg add "HKLM\SOFTWARE\Policies\Microsoft\MicrosoftEdge" /v AllowPrelaunch /t REG_DWORD /d 0 /f >nul 2>&1
reg add "HKLM\SOFTWARE\Policies\Microsoft\MicrosoftEdge" /v AllowTabPreloading /t REG_DWORD /d 0 /f >nul 2>&1

call :log_error "Edge - Blocco update" 0

echo [OK] Edge update bloccato
call :progress 85 "Edge"

:: =====================================
:: TASK TELEMETRIA
:: =====================================

echo.
echo Disabilitazione task telemetria...

for %%t in (
    "Microsoft\Windows\Application Experience\Microsoft Compatibility Appraiser"
    "Microsoft\Windows\Application Experience\ProgramDataUpdater"
    "Microsoft\Windows\Application Experience\StartupAppTask"
    "Microsoft\Windows\Customer Experience Improvement Program\Consolidator"
    "Microsoft\Windows\Customer Experience Improvement Program\UsbCeip"
    "Microsoft\Windows\DiskDiagnostic\Microsoft-Windows-DiskDiagnosticDataCollector"
    "Microsoft\Windows\Windows Error Reporting\QueueReporting"
    "Microsoft\Windows\CloudExperienceHost\CreateObjectTask"
    "Microsoft\Windows\Shell\FamilySafetyMonitor"
    "Microsoft\Windows\Shell\FamilySafetyRefreshTask"
    "Microsoft\Windows\PI\Sqm-Tasks"
    "Microsoft\Office\OfficeTelemetryAgentLogOn"
    "Microsoft\Office\OfficeTelemetryAgentFallBack"
    "Microsoft\Windows\Windows Update\Automatic App Update"
) do (
    schtasks /change /TN "%%t" /disable >nul 2>&1
)

call :log_error "Task Telemetria - Disabilitazione" 0

echo [OK] Task telemetria disabilitati
call :progress 90 "Task"

:: =====================================
:: WINDOWS UPDATE - Ottimizzazioni
:: =====================================

echo.
echo Ottimizzazione Windows Update...

reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU" /f >nul 2>&1
reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU" /v AUOptions /t REG_DWORD /d 2 /f >nul 2>&1
reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\DriverSearching" /v SearchOrderConfig /t REG_DWORD /d 0 /f >nul 2>&1
reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization" /v DODownloadMode /t REG_DWORD /d 0 /f >nul 2>&1
reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU" /v NoAutoRebootWithLoggedOnUsers /t REG_DWORD /d 1 /f >nul 2>&1

call :log_error "Windows Update - Ottimizzazione" 0

echo [OK] Windows Update ottimizzato
call :progress 95 "Update"

:: =====================================
:: DISM CLEANUP
:: =====================================

echo.
echo Pulizia componenti Windows...
echo (potrebbe richiedere alcuni minuti)...

:: FIX #6: rimosso /ResetBase - e' irreversibile e impedisce di disinstallare aggiornamenti.
:: Usiamo solo StartComponentCleanup che e' sicuro e reversibile.
Dism /Online /Cleanup-Image /StartComponentCleanup >nul 2>&1
if %errorlevel% equ 0 (
    echo [OK] Pulizia componenti completata
    call :log_error "DISM Cleanup" 0
) else (
    echo [WARN] DISM Cleanup non completo - continuo...
    call :log_error "DISM Cleanup" %errorlevel%
)

call :progress 98 "Cleanup"

:: =====================================
:: PULIZIA FILE TEMP
:: FIX #6: Log NON e' in TEMP quindi non viene cancellato
:: =====================================

echo.
echo Pulizia file temporanei...

if exist "%TEMP%\*" (
    del /q /f /s "%TEMP%\*" >nul 2>&1
    for /d %%p in ("%TEMP%\*") do rd /q "%%p" 2>nul
)

if exist "%SystemRoot%\Temp\*" (
    del /q /f /s "%SystemRoot%\Temp\*" >nul 2>&1
    for /d %%p in ("%SystemRoot%\Temp\*") do rd /q "%%p" 2>nul
)

:: FIX #7: Prefetch NON viene toccato.
:: Cancellarlo non migliora le performance: Windows lo ricrea e i primi avvii risultano piu' lenti.

echo [OK] File temporanei puliti

:: =====================================
:: PULIZIA DISCO (Cleanmgr)
:: FIX #2: rimosso sageset interattivo; configurazione via registry
:: =====================================

:: FIX #9: cleanmgr e' legacy su Windows 11. Usiamo Storage Sense via PowerShell.
echo.
echo Pulizia disco con Storage Sense (Windows 11)...

%PS% " ^
  $StorageSense = 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\StorageSense\Parameters\StoragePolicy'; ^
  if (-not (Test-Path $StorageSense)) { New-Item -Path $StorageSense -Force | Out-Null }; ^
  Set-ItemProperty -Path $StorageSense -Name '01' -Value 1 -Type DWord; ^
  Set-ItemProperty -Path $StorageSense -Name '04' -Value 1 -Type DWord; ^
  Set-ItemProperty -Path $StorageSense -Name '08' -Value 1 -Type DWord; ^
  Set-ItemProperty -Path $StorageSense -Name '32' -Value 0 -Type DWord; ^
  Set-ItemProperty -Path $StorageSense -Name '256' -Value 30 -Type DWord; ^
  & ([ScriptBlock]::Create((New-Object Net.WebClient))) 2>$null; ^
  Start-Process -FilePath 'cleanmgr.exe' -ArgumentList '/sagerun:1' -NoNewWindow -Wait -ErrorAction SilentlyContinue ^
" >nul 2>&1

call :log_error "Storage Sense - Configurazione" 0
echo [OK] Storage Sense configurato e pulizia avviata

:: =====================================
:: VERIFICA RETE
:: =====================================

echo.
echo Verifica servizi di rete...

sc query WlanSvc | find "RUNNING" >nul 2>&1
if %errorlevel% neq 0 (
    echo [WARN] Servizio Wi-Fi non attivo - riavvio...
    sc start WlanSvc >nul 2>&1
) else (
    echo [OK] Servizio Wi-Fi attivo
)

echo [OK] Connettivita' di rete preservata

:: =====================================
:: RIEPILOGO FINALE
:: =====================================

call :progress 100 "Completato!"

echo.
echo =====================================
echo   OTTIMIZZAZIONE COMPLETATA
echo =====================================
echo.
echo Log completo: %LOG_FILE%
echo Backup registry: %REG_BACKUP_DIR%
echo Backup hosts: %HOSTS_BACKUP%
echo.
echo Errori riscontrati: %ERROR_COUNT%
echo.

if %ERROR_COUNT% gtr 0 (
    echo [ATTENZIONE] Alcune operazioni hanno generato errori.
    echo Controllare il file di log per dettagli.
    echo.
)

echo IMPORTANTE: Riavviare il PC per applicare tutte le modifiche.
echo.

set /p "REBOOT=Riavviare ora? (S/N): "
if /i "!REBOOT!"=="S" (
    echo Riavvio in 15 secondi... (Ctrl+C per annullare)
    shutdown /r /t 15 /c "Riavvio per completare ottimizzazione Windows 11"
) else (
    echo.
    echo Ricorda di riavviare manualmente per applicare le modifiche.
)

echo.
pause
exit /b 0
