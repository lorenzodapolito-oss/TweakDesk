@echo off
chcp 65001 >nul
setlocal

:: ============================================
:: Controlla privilegi amministratore
:: ============================================
net session >nul 2>&1
if %errorlevel% neq 0 (
    echo Devi eseguire questo BAT come amministratore!
    pause
    exit /b
)

:: ============================================
:: CONFIGURA QUI DOVE SI TROVA IL BACKUP
:: ============================================
set "BACKUP_DIR=D:\StartBackup"

echo.
echo ============================================
echo   Importazione configurazione Start Menu
echo ============================================
echo.
echo   Sorgente backup: %BACKUP_DIR%
echo.

if not exist "%BACKUP_DIR%" (
    echo ERRORE: Cartella backup non trovata:
    echo   %BACKUP_DIR%
    echo.
    echo Modifica la variabile BACKUP_DIR con il percorso corretto.
    pause
    exit /b
)

echo   ATTENZIONE: la configurazione attuale dello Start Menu
echo   verra' sovrascritta con quella del PC di riferimento.
echo.
pause

:: Termina explorer e StartMenuExperienceHost per sbloccare i file
echo   Chiudo Explorer e Start Menu...
taskkill /F /IM explorer.exe >nul 2>&1
taskkill /F /IM StartMenuExperienceHost.exe >nul 2>&1
timeout /t 2 >nul

echo   Importo i file di configurazione...

powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$src = '%BACKUP_DIR%';" ^
  "$dst1 = \"$env:LOCALAPPDATA\Packages\Microsoft.Windows.StartMenuExperienceHost_cw5n1h2txyewy\LocalState\";" ^
  "$dst2 = \"$env:APPDATA\Microsoft\Windows\Start Menu\";" ^
  "$dst3 = 'C:\ProgramData\Microsoft\Windows\Start Menu';" ^
  "if (-not (Test-Path $dst1)) { New-Item -ItemType Directory $dst1 -Force | Out-Null };" ^
  "$bins = Get-ChildItem $src -Filter 'start*.bin' -Force -ErrorAction SilentlyContinue;" ^
  "foreach ($f in $bins) { Copy-Item $f.FullName -Destination $dst1 -Force; Write-Host \"  Copiato: $($f.Name)\" -ForegroundColor Green };" ^
  "$userSrc = Join-Path $src 'UserStartMenu';" ^
  "if (Test-Path $userSrc) {" ^
  "  Copy-Item \"$userSrc\*\" -Destination $dst2 -Recurse -Force -ErrorAction SilentlyContinue;" ^
  "  Write-Host '  Ripristinato: Start Menu utente corrente' -ForegroundColor Green" ^
  "};" ^
  "$commonSrc = Join-Path $src 'CommonStartMenu';" ^
  "if (Test-Path $commonSrc) {" ^
  "  Copy-Item \"$commonSrc\*\" -Destination $dst3 -Recurse -Force -ErrorAction SilentlyContinue;" ^
  "  Write-Host '  Ripristinato: Start Menu comune (tutti gli utenti)' -ForegroundColor Green" ^
  "}"

:: Importa registro
set "REG_FILE=%BACKUP_DIR%\StartMenu.reg"
if exist "%REG_FILE%" (
    reg import "%REG_FILE%" >nul 2>&1
    echo   Registro Start Menu importato.
)

:: Riavvia Explorer
echo.
echo   Riavvio Explorer...
start explorer.exe
timeout /t 3 >nul

echo.
echo ============================================
echo   Importazione completata.
echo   Se non vedi le modifiche, riavvia il PC.
echo ============================================
echo.
pause
