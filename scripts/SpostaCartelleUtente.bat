@echo off
chcp 65001 >nul
title Sposta le cartelle utente
setlocal

net session >nul 2>&1
if errorlevel 1 (
    echo Richiedo privilegi amministratore...
    powershell -NoProfile -Command "Start-Process '%~f0' -Verb RunAs"
    exit /b
)

echo.
echo ============================================
echo   Spostamento cartelle utente su un altro disco
echo ============================================
echo.
echo   Utente corrente: %USERNAME%
echo.
echo   Cartelle: Desktop, Documenti, Immagini, Musica, Video, Download
echo.
echo   REGOLE:
echo     - I file non presenti nella destinazione vengono spostati
echo     - Duplicati: vince la destinazione, la copia su C: viene eliminata
echo     - AppData e il resto del profilo NON vengono toccati
echo.
set "DISCO=D"
set /p "DISCO=Lettera del disco di destinazione [D]: "
set "DISCO=%DISCO:~0,1%"
if not exist "%DISCO%:\" (
    echo.
    echo ERRORE: il disco %DISCO%: non esiste o non e' accessibile.
    pause
    exit /b 1
)
set "DESTROOT=%DISCO%:"
echo.
echo   Destinazione: %DESTROOT%\Desktop, %DESTROOT%\Documenti, ...
echo   Controlla di avere abbastanza spazio libero sul disco %DISCO%:.
echo.
pause

powershell -NoProfile -ExecutionPolicy Bypass -Command "$t=[IO.File]::ReadAllText('%~f0'); Invoke-Expression $t.Substring($t.IndexOf('#PS'+'#START'))"

echo.
echo ============================================
echo   Operazione completata.
echo   Riavvia il PC per applicare tutte le modifiche.
echo ============================================
echo.
pause
exit /b

#PS#START
$root = $env:DESTROOT
$items = @(
    @{ N = 'Desktop';   V = 'Desktop';     D = 'Desktop'   },
    @{ N = 'Documenti'; V = 'Personal';    D = 'Documenti' },
    @{ N = 'Immagini';  V = 'My Pictures'; D = 'Immagini'  },
    @{ N = 'Musica';    V = 'My Music';    D = 'Musica'    },
    @{ N = 'Video';     V = 'My Video';    D = 'Video'     },
    @{ N = 'Download';  V = '{374DE290-123F-4565-9164-39C4925E467B}'; D = 'Download' }
)
$usf = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders'
$sf  = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders'
$key = Get-Item -LiteralPath $usf
function Say($t, $c = 'Gray') { Write-Host $t -ForegroundColor $c }

foreach ($it in $items) {
    Say ('=== ' + $it.N + ' ===') 'Cyan'
    # Il percorso attuale si legge dal registro (la cartella puo' essere anche in OneDrive).
    $raw = $key.GetValue($it.V, $null, 'DoNotExpandEnvironmentNames')
    $cur = if ($raw) { [Environment]::ExpandEnvironmentVariables([string]$raw).TrimEnd('\') } else { $null }
    $dest = Join-Path ($root + '\') $it.D
    if (-not $cur -or -not (Test-Path -LiteralPath $cur)) { Say '  Cartella di origine non trovata, saltata.' 'Yellow'; continue }
    if ($cur -ieq $dest) { Say ('  Gia in ' + $dest + ', saltata.') 'Yellow'; continue }
    try {
        New-Item -ItemType Directory -Path $dest -Force -ErrorAction Stop | Out-Null
    } catch { Say ('  [ERRORE] Non riesco a creare ' + $dest + ': ' + $_.Exception.Message) 'Red'; continue }

    # Sposta quello che manca nella destinazione. /XC /XN /XO: un file gia presente non viene toccato.
    & robocopy $cur $dest /E /MOVE /XJ /XC /XN /XO /R:1 /W:1 /NFL /NDL /NJH /NJS /NP | Out-Null
    $rc = $LASTEXITCODE
    if ($rc -ge 8) {
        Say ('  [ERRORE] Spostamento non riuscito (codice ' + $rc + '): il registro non viene cambiato.') 'Red'
        continue
    }
    # Quello che resta nell'origine esiste gia nella destinazione: vince la destinazione.
    $left = @(Get-ChildItem -LiteralPath $cur -Recurse -Force -File -ErrorAction SilentlyContinue)
    if ($left.Count -gt 0) {
        Say ('  Duplicati eliminati da C: ' + $left.Count) 'Yellow'
        Remove-Item -LiteralPath $cur -Recurse -Force -ErrorAction SilentlyContinue
    }
    Remove-Item -LiteralPath $cur -Force -ErrorAction SilentlyContinue

    # La cartella di Windows cambia in entrambi i punti del registro.
    Set-ItemProperty -LiteralPath $usf -Name $it.V -Value $dest -Type ExpandString
    Set-ItemProperty -LiteralPath $sf  -Name $it.V -Value $dest
    Say ('  OK: ' + $it.N + ' -> ' + $dest) 'Green'
}
