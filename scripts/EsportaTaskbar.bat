@echo off
chcp 65001 >nul
setlocal

net session >nul 2>&1
if %errorlevel% neq 0 (
    echo Devi eseguire questo BAT come amministratore!
    pause
    exit /b
)

set "BACKUP_DIR=D:\TaskbarBackup"

echo.
echo ============================================
echo   Esportazione configurazione Taskbar
echo   (compatibile Windows 11 24H2 / 25H2)
echo ============================================
echo.
echo   Salvataggio in: %BACKUP_DIR%
echo.

if not exist "%BACKUP_DIR%" (
    mkdir "%BACKUP_DIR%"
    echo   Cartella creata: %BACKUP_DIR%
)

echo   Chiudo Explorer e StartMenu per sbloccare i file...
taskkill /F /IM explorer.exe >nul 2>&1
taskkill /F /IM StartMenuExperienceHost.exe >nul 2>&1
timeout /t 2 >nul

echo   Copio i file di configurazione...

powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$dst = '%BACKUP_DIR%';" ^
  "$copied = 0;" ^
  "$paths = @(" ^
  "  @{ Label='taskbar.db (nuovo 24H2+)'; Path=\"$env:LOCALAPPDATA\Packages\Microsoft.Windows.StartMenuExperienceHost_cw5n1h2txyewy\LocalState\taskbar.db\" }," ^
  "  @{ Label='taskbarlayout.db';         Path=\"$env:LOCALAPPDATA\Packages\Microsoft.Windows.StartMenuExperienceHost_cw5n1h2txyewy\LocalState\taskbarlayout.db\" }," ^
  "  @{ Label='LayoutModification';       Path=\"$env:LOCALAPPDATA\Microsoft\Windows\Shell\LayoutModification.xml\" }," ^
  "  @{ Label='LayoutModification.json';  Path=\"$env:LOCALAPPDATA\Microsoft\Windows\Shell\LayoutModification.json\" }" ^
  ");" ^
  "foreach ($p in $paths) {" ^
  "  if (Test-Path $p.Path) {" ^
  "    Copy-Item $p.Path -Destination $dst -Force;" ^
  "    Write-Host \"  [OK] $($p.Label)\" -ForegroundColor Green;" ^
  "    $copied++" ^
  "  } else {" ^
  "    Write-Host \"  [--] $($p.Label) : non presente\" -ForegroundColor DarkGray" ^
  "  }" ^
  "};" ^
  "$lnkSrc = \"$env:APPDATA\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar\";" ^
  "$lnkDst = Join-Path $dst 'TaskBar';" ^
  "if (Test-Path $lnkSrc) {" ^
  "  if (-not (Test-Path $lnkDst)) { New-Item -ItemType Directory $lnkDst -Force | Out-Null };" ^
  "  Get-ChildItem $lnkSrc -Filter '*.lnk' | ForEach-Object { Copy-Item $_.FullName $lnkDst -Force; $copied++ };" ^
  "  Write-Host \"  [OK] Collegamenti .lnk copiati\" -ForegroundColor Green" ^
  "};" ^
  "reg export 'HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Taskband' '$dst\Taskband.reg' /y 2>$null | Out-Null;" ^
  "reg export 'HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced' '$dst\TaskbarAdvanced.reg' /y 2>$null | Out-Null;" ^
  "Write-Host \"  [OK] Registro esportato\" -ForegroundColor Green;" ^
  "Write-Host \"`n  Totale file copiati: $copied\" -ForegroundColor Cyan"

echo.
echo   Riavvio Explorer...
start explorer.exe

echo.
echo ============================================
echo   Esportazione completata.
echo   File salvati in: %BACKUP_DIR%
echo   Copia questa cartella sul PC di destinazione
echo   e lancia ImportaTaskbar.bat
echo ============================================
echo.
pause
