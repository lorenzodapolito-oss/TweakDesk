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
:: Controlla che D: esista
:: ============================================
if not exist "D:\" (
    echo ERRORE: Il disco D: non esiste o non e' accessibile!
    pause
    exit /b
)

echo.
echo ============================================
echo   Spostamento cartelle utente da C a D
echo ============================================
echo.
echo   Utente corrente: %USERNAME%
echo   Destinazione:    D:\^<NomeCartella^>
echo.
echo   Cartelle che verranno spostate:
echo     - Desktop
echo     - Documenti
echo     - Immagini
echo     - Musica
echo     - Video
echo     - Download
echo.
echo   REGOLE:
echo     - File non presenti su D: vengono spostati
echo     - Duplicati: vince D:, il file su C viene eliminato
echo     - AppData e il resto del profilo NON vengono toccati
echo.
echo   Assicurati di avere abbastanza spazio su D:
echo.
pause

echo.
echo Avvio spostamento...
echo.

powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$folders = @(" ^
  "  @{ Name='Desktop';   Shell='Desktop';           Dest='D:\Desktop'   }," ^
  "  @{ Name='Documenti'; Shell='Personal';          Dest='D:\Documenti' }," ^
  "  @{ Name='Immagini';  Shell='My Pictures';       Dest='D:\Immagini'  }," ^
  "  @{ Name='Musica';    Shell='My Music';          Dest='D:\Musica'    }," ^
  "  @{ Name='Video';     Shell='My Video';          Dest='D:\Video'     }," ^
  "  @{ Name='Download';  Shell='{374DE290-123F-4565-9164-39C4925E467B}'; Dest='D:\Download' }" ^
  ");" ^
  "foreach ($f in $folders) {" ^
  "  $dest = $f.Dest;" ^
  "  Write-Host \"=== $($f.Name) ===\" -ForegroundColor Cyan;" ^
  "  $shell = New-Object -ComObject Shell.Application;" ^
  "  $ns = $shell.Namespace($f.Shell);" ^
  "  if ($ns -ne $null) { $current = $ns.Self.Path } else { $current = $null };" ^
  "  if (-not $current -or -not (Test-Path $current)) { Write-Host \"  Cartella sorgente non trovata, saltata.\" -ForegroundColor Yellow; Write-Host; continue };" ^
  "  if ($current -eq $dest) { Write-Host \"  Gia' in $dest - saltata.\" -ForegroundColor Yellow; Write-Host; continue };" ^
  "  if (-not (Test-Path $dest)) { New-Item -ItemType Directory -Path $dest -Force | Out-Null; Write-Host \"  Cartella creata: $dest\" -ForegroundColor Green };" ^
  "  $files = Get-ChildItem -Path $current -Recurse -Force;" ^
  "  $moved = 0; $skipped = 0; $deleted = 0;" ^
  "  foreach ($item in $files) {" ^
  "    $relative = $item.FullName.Substring($current.Length).TrimStart('\');" ^
  "    $target = Join-Path $dest $relative;" ^
  "    if ($item.PSIsContainer) {" ^
  "      if (-not (Test-Path $target)) { New-Item -ItemType Directory -Path $target -Force | Out-Null }" ^
  "    } else {" ^
  "      if (Test-Path $target) {" ^
  "        Remove-Item -Path $item.FullName -Force -ErrorAction SilentlyContinue;" ^
  "        $skipped++; $deleted++;" ^
  "        Write-Host \"  DUPLICATO eliminato da C: $relative\" -ForegroundColor Yellow" ^
  "      } else {" ^
  "        Move-Item -Path $item.FullName -Destination $target -Force -ErrorAction SilentlyContinue;" ^
  "        $moved++" ^
  "      }" ^
  "    }" ^
  "  };" ^
  "  Get-ChildItem -Path $current -Recurse -Force | Where-Object { $_.PSIsContainer } | Sort-Object FullName -Descending | ForEach-Object { if (-not (Get-ChildItem $_.FullName -Force)) { Remove-Item $_.FullName -Force -ErrorAction SilentlyContinue } };" ^
  "  if (-not (Get-ChildItem $current -Force)) { Remove-Item $current -Force -ErrorAction SilentlyContinue; Write-Host \"  Cartella sorgente rimossa: $current\" -ForegroundColor Green };" ^
  "  $regPaths = @(" ^
  "    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders'," ^
  "    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders'" ^
  "  );" ^
  "  foreach ($reg in $regPaths) {" ^
  "    $props = Get-ItemProperty $reg -ErrorAction SilentlyContinue;" ^
  "    if ($props) {" ^
  "      $props.PSObject.Properties | Where-Object { $_.Value -eq $current } | ForEach-Object {" ^
  "        Set-ItemProperty -Path $reg -Name $_.Name -Value $dest;" ^
  "        Write-Host \"  Registro aggiornato: $($_.Name) -> $dest\" -ForegroundColor Green" ^
  "      }" ^
  "    }" ^
  "  };" ^
  "  Write-Host \"  Spostati: $moved  |  Duplicati eliminati da C: $deleted\" -ForegroundColor Green;" ^
  "  Write-Host \"  OK: $($f.Name) -> $dest\" -ForegroundColor Green; Write-Host" ^
  "}"

echo.
echo ============================================
echo   Operazione completata.
echo   Su C: rimane solo AppData e il profilo
echo   di sistema. Riavvia il PC per applicare
echo   tutte le modifiche.
echo ============================================
echo.
pause
