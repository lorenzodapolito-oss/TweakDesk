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
echo   Importazione configurazione Taskbar
echo   (compatibile Windows 11 24H2 / 25H2)
echo ============================================
echo.
echo   Sorgente backup: %BACKUP_DIR%
echo.

if not exist "%BACKUP_DIR%" (
    echo ERRORE: Cartella backup non trovata: %BACKUP_DIR%
    pause
    exit /b
)

echo   ATTENZIONE: la taskbar attuale verra' sovrascritta.
echo.
pause

echo   Chiudo Explorer e StartMenu...
taskkill /F /IM explorer.exe >nul 2>&1
taskkill /F /IM StartMenuExperienceHost.exe >nul 2>&1
taskkill /F /IM ShellExperienceHost.exe >nul 2>&1
timeout /t 3 >nul

echo   Importo i file...

powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$src = '%BACKUP_DIR%';" ^
  "$dst1 = \"$env:LOCALAPPDATA\Packages\Microsoft.Windows.StartMenuExperienceHost_cw5n1h2txyewy\LocalState\";" ^
  "$dst2 = \"$env:LOCALAPPDATA\Microsoft\Windows\Shell\";" ^
  "$dst3 = \"$env:APPDATA\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar\";" ^
  "foreach ($d in @($dst1,$dst2,$dst3)) { if (-not (Test-Path $d)) { New-Item -ItemType Directory $d -Force | Out-Null } };" ^
  "$files = @('taskbar.db','taskbarlayout.db','LayoutModification.xml','LayoutModification.json');" ^
  "foreach ($f in $files) {" ^
  "  $srcFile = Join-Path $src $f;" ^
  "  if (Test-Path $srcFile) {" ^
  "    if ($f -like '*.xml' -or $f -like '*.json') { $d = $dst2 } else { $d = $dst1 };" ^
  "    Copy-Item $srcFile -Destination $d -Force;" ^
  "    Write-Host \"  [OK] $f ripristinato\" -ForegroundColor Green" ^
  "  }" ^
  "};" ^
  "$lnkSrc = Join-Path $src 'TaskBar';" ^
  "if (Test-Path $lnkSrc) {" ^
  "  Get-ChildItem $lnkSrc -Filter '*.lnk' | ForEach-Object { Copy-Item $_.FullName $dst3 -Force };" ^
  "  Write-Host '  [OK] Collegamenti .lnk ripristinati' -ForegroundColor Green" ^
  "};" ^
  "$regFiles = @('Taskband.reg','TaskbarAdvanced.reg');" ^
  "foreach ($r in $regFiles) {" ^
  "  $rp = Join-Path $src $r;" ^
  "  if (Test-Path $rp) { reg import $rp 2>$null; Write-Host \"  [OK] Registro $r importato\" -ForegroundColor Green }" ^
  "}"

echo.
echo   Riavvio Explorer...
start explorer.exe
timeout /t 2 >nul
start explorer.exe

echo.
echo ============================================
echo   Importazione completata.
echo   Se la taskbar non e' cambiata, riavvia
echo   il PC (non solo esegui Disconnetti).
echo ============================================
echo.
pause
