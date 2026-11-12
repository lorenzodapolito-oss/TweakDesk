@echo off
setlocal enableextensions
title Blocco definitivo di Windows Update
:: ============================================================================
:: Blocca Windows Update in modo che Windows non lo riattivi da solo.
:: Esegui come AMMINISTRATORE. Con /guard (lanciato dall'attivita' pianificata)
:: riapplica il blocco in silenzio. Non esiste uno script di ripristino.
:: ============================================================================

if /i "%~1"=="/guard" goto guard

net session >nul 2>&1
if errorlevel 1 (
    echo Richiedo privilegi amministratore...
    powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
    exit /b
)

echo.
echo ============================================================
echo    BLOCCO DEFINITIVO DI WINDOWS UPDATE
echo ============================================================
echo.
echo Questo script:
echo   - ferma e disattiva Windows Update, Update Orchestrator e WaaSMedic
echo   - disattiva le loro attivita' pianificate
echo   - impedisce l'avvio dei programmi di aggiornamento (anche se Windows
echo     prova a rilanciarli)
echo   - imposta i criteri di sistema e un server di aggiornamento inesistente
echo   - blocca i server Microsoft con hosts e firewall
echo   - crea un controllo all'avvio e ogni ora che rimette il blocco se
echo     Windows lo toglie
echo.
echo Conseguenze: niente aggiornamenti di sicurezza, anche per Microsoft
echo Defender. Nessuno script di questo programma lo annulla.
echo.
choice /c SN /m "Vuoi continuare"
if errorlevel 2 exit /b 0

echo.
echo [1/8] Arresto processi e servizi...
call :stop_all
echo [2/8] Disattivazione servizi...
call :disable_services
echo [3/8] Disattivazione attivita' pianificate...
call :disable_tasks
echo [4/8] Blocco dei programmi di aggiornamento...
call :ifeo
echo [5/8] Criteri di sistema...
call :policies
echo [6/8] Neutralizzazione file di ripristino (WaaSMedic, SIHClient)...
call :files
echo [7/8] Pulizia cache e blocco dei server...
call :caches
call :network
echo [8/8] Controllo automatico all'avvio e ogni ora...
call :guard_task

echo.
echo ============================================================
echo   FATTO: Windows Update e' bloccato.
echo ============================================================
pause
exit /b 0

:guard
call :stop_all
call :disable_services
call :disable_tasks
call :ifeo
call :policies
call :files
exit /b 0

:stop_all
for %%p in (wuauclt.exe WaaSMedicAgent.exe SIHClient.exe UsoClient.exe MoUsoCoreWorker.exe usocoreworker.exe) do taskkill /F /T /IM %%p >nul 2>&1
for %%s in (wuauserv UsoSvc WaaSMedicSvc BITS DoSvc) do net stop %%s /y >nul 2>&1
exit /b 0

:disable_services
for %%s in (wuauserv UsoSvc WaaSMedicSvc BITS DoSvc) do (
    sc config %%s start= disabled >nul 2>&1
    reg add "HKLM\SYSTEM\CurrentControlSet\Services\%%s" /v Start /t REG_DWORD /d 4 /f >nul 2>&1
)
exit /b 0

:disable_tasks
powershell -NoProfile -ExecutionPolicy Bypass -Command "Get-ScheduledTask -TaskPath '\Microsoft\Windows\WindowsUpdate\*','\Microsoft\Windows\UpdateOrchestrator\*','\Microsoft\Windows\WaaSMedic\*' -ErrorAction SilentlyContinue | Disable-ScheduledTask -ErrorAction SilentlyContinue | Out-Null" >nul 2>&1
exit /b 0

:ifeo
:: Ogni avvio di questi programmi viene sostituito da un comando che non fa nulla.
set "IFEO=HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options"
for %%e in (UsoClient.exe MoUsoCoreWorker.exe WaaSMedicAgent.exe SIHClient.exe wuauclt.exe) do reg add "%IFEO%\%%e" /v Debugger /t REG_SZ /d "cmd.exe /c rem" /f >nul 2>&1
exit /b 0

:policies
set "WU=HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate"
reg add "%WU%" /v DisableWindowsUpdateAccess /t REG_DWORD /d 1 /f >nul 2>&1
reg add "%WU%" /v DoNotConnectToWindowsUpdateInternetLocations /t REG_DWORD /d 1 /f >nul 2>&1
reg add "%WU%" /v SetDisableUXWUAccess /t REG_DWORD /d 1 /f >nul 2>&1
reg add "%WU%" /v ExcludeWUDriversInQualityUpdate /t REG_DWORD /d 1 /f >nul 2>&1
reg add "%WU%" /v WUServer /t REG_SZ /d "http://127.0.0.1" /f >nul 2>&1
reg add "%WU%" /v WUStatusServer /t REG_SZ /d "http://127.0.0.1" /f >nul 2>&1
reg add "%WU%\AU" /v NoAutoUpdate /t REG_DWORD /d 1 /f >nul 2>&1
reg add "%WU%\AU" /v AUOptions /t REG_DWORD /d 1 /f >nul 2>&1
reg add "%WU%\AU" /v UseWUServer /t REG_DWORD /d 1 /f >nul 2>&1
reg add "%WU%\AU" /v NoAutoRebootWithLoggedOnUsers /t REG_DWORD /d 1 /f >nul 2>&1
reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization" /v DODownloadMode /t REG_DWORD /d 0 /f >nul 2>&1
exit /b 0

:files
:: Si usa l'SID del gruppo Amministratori: il nome cambia con la lingua di Windows.
for %%f in (WaaSMedicSvc.dll WaaSMedicAgent.exe SIHClient.exe) do (
    if exist "%WinDir%\System32\%%f" (
        takeown /f "%WinDir%\System32\%%f" /a >nul 2>&1
        icacls "%WinDir%\System32\%%f" /grant "*S-1-5-32-544:F" >nul 2>&1
        ren "%WinDir%\System32\%%f" "%%f.old" >nul 2>&1
    )
)
exit /b 0

:caches
del /s /q /f "%WinDir%\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\*" >nul 2>&1
rd /s /q "%WinDir%\SoftwareDistribution" >nul 2>&1
mkdir "%WinDir%\SoftwareDistribution" >nul 2>&1
icacls "%WinDir%\SoftwareDistribution" /deny "*S-1-5-18:(OI)(CI)(F)" "*S-1-5-19:(OI)(CI)(F)" >nul 2>&1
exit /b 0

:network
set "HOSTS=%WinDir%\System32\drivers\etc\hosts"
>>"%HOSTS%" echo.
for %%d in (windowsupdate.microsoft.com update.microsoft.com download.windowsupdate.com au.download.windowsupdate.com fe2.update.microsoft.com fe3.delivery.mp.microsoft.com sls.update.microsoft.com slscr.update.microsoft.com tlu.dl.delivery.mp.microsoft.com dl.delivery.mp.microsoft.com) do (
    findstr /i /c:"%%d" "%HOSTS%" >nul 2>&1 || >>"%HOSTS%" echo 0.0.0.0 %%d
)
for %%s in (wuauserv UsoSvc BITS) do (
    netsh advfirewall firewall delete rule name="BLOCCO_WU_%%s" >nul 2>&1
    netsh advfirewall firewall add rule name="BLOCCO_WU_%%s" dir=out action=block program="%WinDir%\System32\svchost.exe" service=%%s >nul 2>&1
)
exit /b 0

:guard_task
set "GUARDDIR=%ProgramData%\TweakDesk"
mkdir "%GUARDDIR%" >nul 2>&1
copy /y "%~f0" "%GUARDDIR%\blocco-update.cmd" >nul 2>&1
schtasks /create /tn "TweakDesk\BloccoUpdate-Avvio" /sc onstart /ru SYSTEM /rl highest /tr "\"%GUARDDIR%\blocco-update.cmd\" /guard" /f >nul 2>&1
schtasks /create /tn "TweakDesk\BloccoUpdate-Orario" /sc hourly /mo 1 /ru SYSTEM /rl highest /tr "\"%GUARDDIR%\blocco-update.cmd\" /guard" /f >nul 2>&1
exit /b 0
