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
:: CONFIGURA QUI DOVE SALVARE IL BACKUP
:: ============================================
set "BACKUP_DIR=D:\StartBackup"

echo.
echo ============================================
echo   Esportazione configurazione Start Menu
echo ============================================
echo.
echo   Salvataggio in: %BACKUP_DIR%
echo.

if not exist "%BACKUP_DIR%" (
    mkdir "%BACKUP_DIR%"
    echo   Cartella creata: %BACKUP_DIR%
)

:: Termina explorer per sbloccare i file
echo   Chiudo Explorer per sbloccare i file...
taskkill /F /IM explorer.exe >nul 2>&1
timeout /t 2 >nul

echo   Copio i file di configurazione...

powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$dst = '%BACKUP_DIR%';" ^
  "$src1 = \"$env:LOCALAPPDATA\Packages\Microsoft.Windows.StartMenuExperienceHost_cw5n1h2txyewy\LocalState\";" ^
  "$src2 = \"$env:APPDATA\Microsoft\Windows\Start Menu\";" ^
  "$src3 = 'C:\ProgramData\Microsoft\Windows\Start Menu';" ^
  "if (Test-Path $src1) {" ^
  "  $files = Get-ChildItem $src1 -Filter 'start*.bin' -Force -ErrorAction SilentlyContinue;" ^
  "  foreach ($f in $files) { Copy-Item $f.FullName -Destination $dst -Force; Write-Host \"  Copiato: $($f.Name)\" -ForegroundColor Green }" ^
  "};" ^
  "$userDst = Join-Path $dst 'UserStartMenu';" ^
  "if (-not (Test-Path $userDst)) { New-Item -ItemType Directory $userDst -Force | Out-Null };" ^
  "if (Test-Path $src2) {" ^
  "  Copy-Item \"$src2\*\" -Destination $userDst -Recurse -Force -ErrorAction SilentlyContinue;" ^
  "  Write-Host '  Copiato: Start Menu utente corrente' -ForegroundColor Green" ^
  "};" ^
  "$commonDst = Join-Path $dst 'CommonStartMenu';" ^
  "if (-not (Test-Path $commonDst)) { New-Item -ItemType Directory $commonDst -Force | Out-Null };" ^
  "if (Test-Path $src3) {" ^
  "  Copy-Item \"$src3\*\" -Destination $commonDst -Recurse -Force -ErrorAction SilentlyContinue;" ^
  "  Write-Host '  Copiato: Start Menu comune (tutti gli utenti)' -ForegroundColor Green" ^
  "};" ^
  "reg export 'HKCU\Software\Microsoft\Windows\CurrentVersion\CloudStore\Store\Cache\DefaultAccount' '$dst\StartMenu.reg' /y 2>$null | Out-Null;" ^
  "Write-Host '  Registro Start Menu esportato.' -ForegroundColor Green"

:: Riavvia Explorer
echo.
echo   Riavvio Explorer...
start explorer.exe

echo.
echo ============================================
echo   Esportazione completata.
echo   File salvati in: %BACKUP_DIR%
echo   Copia questa cartella sul PC di destinazione
echo   e lancia ImportaStart.bat
echo ============================================
echo.
pause
