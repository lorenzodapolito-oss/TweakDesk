@echo off
:: ============================================================================
:: SCRIPT DEFINITIVO E AGGRESSIVO: DISTRUZIONE WINDOWS UPDATE (W11/W10)
:: Esegui come AMMINISTRATORE
:: ============================================================================

echo.
echo ============================================================
echo    DISTRUZIONE DEFINITIVA E PREVENTIVA DI WINDOWS UPDATE
echo ============================================================
echo.
echo Questo script e piu potente del precedente: blocca anche i Trigger.
pause

:: 1. TERMINAZIONE PROCESSI E SERVIZI 
echo [1/7] Arresto forzato processi e servizi...
taskkill /F /IM wuauclt.exe /T 2>nul
taskkill /F /IM WaaSMedicAgent.exe /T 2>nul
taskkill /F /IM SIHClient.exe /T 2>nul
taskkill /F /IM UsoClient.exe /T 2>nul

net stop wuauserv /y 2>nul
net stop UsoSvc /y 2>nul
net stop WaaSMedicSvc /y 2>nul
net stop BITS /y 2>nul
net stop dosvc /y 2>nul

:: 2. DISABILITAZIONE TOTALE DEI SERVIZI (VALORE 4) [cite: 3, 4]
echo [2/7] Disabilitazione servizi nel registro...
set "SvcKey=HKLM\SYSTEM\CurrentControlSet\Services"
reg add "%SvcKey%\wuauserv" /v Start /t REG_DWORD /d 4 /f
reg add "%SvcKey%\UsoSvc" /v Start /t REG_DWORD /d 4 /f
reg add "%SvcKey%\WaaSMedicSvc" /v Start /t REG_DWORD /d 4 /f
reg add "%SvcKey%\BITS" /v Start /t REG_DWORD /d 4 /f
reg add "%SvcKey%\DoSvc" /v Start /t REG_DWORD /d 4 /f

:: 3. BLOCCO DEI TRIGGER (IL "SEGRETO" PER NON FARLI RIATTIVARE)
echo [3/7] Rimozione permessi sui trigger dei servizi...
:: Questo impedisce a Windows di cambiare lo stato del servizio da solo
icacls "C:\Windows\System32\Tasks\Microsoft\Windows\WindowsUpdate" /inheritance:r /deny "SYSTEM":(OI)(CI)(F) >nul 2>&1
icacls "C:\Windows\System32\Tasks\Microsoft\Windows\UpdateOrchestrator" /inheritance:r /deny "SYSTEM":(OI)(CI)(F) >nul 2>&1

:: 4. NEUTRALIZZAZIONE FILE DI RIPRISTINO (WaaSMedic e SIH) [cite: 4]
echo [4/7] Neutralizzazione file binari (WaaSMedic e SIHClient)...
takeown /f "%WinDir%\System32\WaaSMedicSvc.dll" /a >nul 2>&1
icacls "%WinDir%\System32\WaaSMedicSvc.dll" /grant Administrators:F /inheritance:r >nul 2>&1
ren "%WinDir%\System32\WaaSMedicSvc.dll" WaaSMedicSvc.dll.old 2>nul

takeown /f "%WinDir%\System32\SIHClient.exe" /a >nul 2>&1
icacls "%WinDir%\System32\SIHClient.exe" /grant Administrators:F /inheritance:r >nul 2>&1
ren "%WinDir%\System32\SIHClient.exe" SIHClient.exe.old 2>nul

:: 5. PULIZIA E BLOCCO CARTELLE DI CACHE 
echo [5/7] Pulizia profonda cache aggiornamenti e DO...
:: Svuota Delivery Optimization
del /s /q /f "%WinDir%\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\*" 2>nul

:: Svuota e blocca SoftwareDistribution 
rd /s /q "%WinDir%\SoftwareDistribution" 2>nul
mkdir "%WinDir%\SoftwareDistribution" 2>nul
icacls "%WinDir%\SoftwareDistribution" /deny "SYSTEM":(F) "LOCAL SERVICE":(F) >nul 2>&1

:: 6. CONFIGURAZIONE GROUP POLICY AGGRESSIVE [cite: 6]
echo [6/7] Applicazione restrizioni di sistema...
reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU" /v NoAutoUpdate /t REG_DWORD /d 1 /f
reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate" /v DisableWindowsUpdateAccess /t REG_DWORD /d 1 /f
reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization" /v DODownloadMode /t REG_DWORD /d 0 /f

:: 7. BLOCCO DNS E FIREWALL [cite: 9, 10]
echo [7/7] Blocco finale via Hosts e Firewall...
set "hosts=%WinDir%\System32\drivers\etc\hosts"
echo 0.0.0.0 slscr.update.microsoft.com >> %hosts%
echo 0.0.0.0 fe2.update.microsoft.com >> %hosts%
echo 0.0.0.0 tlu.dl.delivery.mp.microsoft.com >> %hosts%

:: Regola firewall per il processo svchost che ospita gli update [cite: 9]
netsh advfirewall firewall add rule name="BLOCK_WU_TOTALE" dir=out action=block program="%WinDir%\System32\svchost.exe" service=wuauserv >nul 2>&1

echo.
echo ============================================================
echo   OPERAZIONE COMPLETATA: Windows Update e ora paralizzato.
echo ============================================================
pause