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
echo   Prima cosa: tutte le cartelle che trovi sciolte dentro il tuo profilo
echo   (Desktop, Immagini, Musica, Video, Download e quelle che hai creato tu)
echo   vengono messe dentro Documenti.
echo   Poi: Documenti, con tutto quello che contiene, viene spostata sul disco
echo   che scegli.
echo.
echo   REGOLE:
echo     - AppData e OneDrive non vengono mai toccate
echo     - I file non presenti nella destinazione vengono spostati
echo     - Duplicati: vince la destinazione, la copia su C: viene eliminata
echo     - Se uno spostamento fallisce, quella cartella resta dov'e' e il
echo       registro non cambia per lei
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
echo   Destinazione finale: %DESTROOT%\Documenti
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
$usf = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders'
$sf  = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders'
$key = Get-Item -LiteralPath $usf
function Say($t, $c = 'Gray') { Write-Host $t -ForegroundColor $c }

# Nomi di cartella (se la trovi sciolta nel profilo) -> valore di registro da aggiornare,
# cosi' se era una cartella speciale di Windows il registro la segue nella nuova posizione.
$special = @{
    'Desktop'   = 'Desktop'
    'Documenti' = 'Personal'
    'Immagini'  = 'My Pictures'
    'Musica'    = 'My Music'
    'Video'     = 'My Video'
    'Download'  = '{374DE290-123F-4565-9164-39C4925E467B}'
}

$raw = $key.GetValue('Personal', $null, 'DoNotExpandEnvironmentNames')
$docs = if ($raw) { [Environment]::ExpandEnvironmentVariables([string]$raw).TrimEnd('\') } else { $null }
if (-not $docs) {
    Say '[ERRORE] Non trovo Documenti nel registro, mi fermo qui.' 'Red'
    return
}
New-Item -ItemType Directory -Path $docs -Force -ErrorAction SilentlyContinue | Out-Null

Say '=== Sposto dentro Documenti le altre cartelle del profilo ===' 'Cyan'
Get-ChildItem -LiteralPath $env:USERPROFILE -Directory -Force -ErrorAction SilentlyContinue | ForEach-Object {
    $d = $_
    if ($d.FullName -ieq $docs) { return }
    if ($d.Name -ieq 'AppData') { Say '  Salto AppData (dati di sistema, non si tocca).' 'Yellow'; return }
    if ($d.Name -ieq 'OneDrive') { Say '  Salto OneDrive (la gestisce lei, non si tocca).' 'Yellow'; return }
    if ($d.Attributes -band [IO.FileAttributes]::ReparsePoint) { return }

    $dest = Join-Path $docs $d.Name
    Say ('  ' + $d.Name + ' -> Documenti\' + $d.Name) 'Gray'
    try {
        New-Item -ItemType Directory -Path $dest -Force -ErrorAction Stop | Out-Null
    } catch { Say ('  [ERRORE] Non riesco a creare ' + $dest + ': ' + $_.Exception.Message) 'Red'; return }

    & robocopy $d.FullName $dest /E /MOVE /XJ /XC /XN /XO /R:1 /W:1 /NFL /NDL /NJH /NJS /NP | Out-Null
    $rc = $LASTEXITCODE
    if ($rc -ge 8) { Say ('  [ERRORE] Spostamento non riuscito (codice ' + $rc + '), lascio ' + $d.Name + ' dov''e''.') 'Red'; return }

    $left = @(Get-ChildItem -LiteralPath $d.FullName -Recurse -Force -File -ErrorAction SilentlyContinue)
    if ($left.Count -gt 0) { Remove-Item -LiteralPath $d.FullName -Recurse -Force -ErrorAction SilentlyContinue }
    Remove-Item -LiteralPath $d.FullName -Force -ErrorAction SilentlyContinue

    $entry = $special.GetEnumerator() | Where-Object { $_.Key -ieq $d.Name } | Select-Object -First 1
    if ($entry) {
        Set-ItemProperty -LiteralPath $usf -Name $entry.Value -Value $dest -Type ExpandString
        Set-ItemProperty -LiteralPath $sf  -Name $entry.Value -Value $dest
    }
}

Say '=== Sposto Documenti, con tutto dentro, sul disco scelto ===' 'Cyan'
$finale = Join-Path ($root + '\') 'Documenti'
if ($docs -ieq $finale) {
    Say '  Documenti e'' gia'' li'', non sposto nulla.' 'Yellow'
} else {
    New-Item -ItemType Directory -Path $finale -Force -ErrorAction SilentlyContinue | Out-Null
    & robocopy $docs $finale /E /MOVE /XJ /XC /XN /XO /R:1 /W:1 /NFL /NDL /NJH /NJS /NP | Out-Null
    $rc = $LASTEXITCODE
    if ($rc -ge 8) {
        Say ('[ERRORE] Spostamento di Documenti non riuscito (codice ' + $rc + '): il registro non viene cambiato.') 'Red'
    } else {
        $left = @(Get-ChildItem -LiteralPath $docs -Recurse -Force -File -ErrorAction SilentlyContinue)
        if ($left.Count -gt 0) { Remove-Item -LiteralPath $docs -Recurse -Force -ErrorAction SilentlyContinue }
        Remove-Item -LiteralPath $docs -Force -ErrorAction SilentlyContinue

        Set-ItemProperty -LiteralPath $usf -Name 'Personal' -Value $finale -Type ExpandString
        Set-ItemProperty -LiteralPath $sf  -Name 'Personal' -Value $finale

        foreach ($pair in $special.GetEnumerator()) {
            if ($pair.Value -ieq 'Personal') { continue }
            $sub = Join-Path $finale $pair.Key
            if (Test-Path -LiteralPath $sub) {
                Set-ItemProperty -LiteralPath $usf -Name $pair.Value -Value $sub -Type ExpandString
                Set-ItemProperty -LiteralPath $sf  -Name $pair.Value -Value $sub
            }
        }
        Say ('OK: Documenti -> ' + $finale) 'Green'
    }
}
