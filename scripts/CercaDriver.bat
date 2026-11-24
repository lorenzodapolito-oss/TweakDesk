@echo off
title Cerca e installa driver mancanti
setlocal

net session >nul 2>&1
if errorlevel 1 (
    echo Richiedo privilegi amministratore...
    powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
    exit /b
)

echo =====================================
echo  CERCA E INSTALLA DRIVER MANCANTI
echo =====================================
echo.
echo Passi: 1) rileva l'hardware, 2) installa i driver della cartella "Driver"
echo (facoltativa, accanto allo script), 3) cerca altri driver su Windows
echo Update, 4) controlla cosa manca ancora.
echo Non cambia nessuna impostazione di Windows Update.

set "TD_FILE=%~f0"
powershell -NoProfile -ExecutionPolicy Bypass -Command "$t=[IO.File]::ReadAllText($env:TD_FILE); $i=$t.LastIndexOf('#PS'+'#'); Invoke-Expression $t.Substring($i+4)"
echo.
pause
exit /b 0

#PS#
$ErrorActionPreference = 'Stop'
$dir = Split-Path -Parent $env:TD_FILE
function Titolo($t) { Write-Host ''; Write-Host $t -ForegroundColor Cyan }
function Problemi { @(Get-CimInstance Win32_PnPEntity | Where-Object { $_.ConfigManagerErrorCode -ne 0 }) }

try {
    Titolo '1/4 Rilevo l''hardware...'
    try { pnputil /scan-devices | Out-Null } catch { }
    $p = Problemi
    if ($p.Count -eq 0) {
        Write-Host 'Nessun dispositivo con problemi di driver.'
    } else {
        Write-Host ('Dispositivi senza driver o con errori: ' + $p.Count)
        foreach ($d in $p) {
            $n = if ($d.Name) { $d.Name } else { $d.DeviceID }
            Write-Host ('  - ' + $n + ' (codice ' + $d.ConfigManagerErrorCode + ')')
        }
    }

    Titolo '2/4 Driver nella cartella "Driver"...'
    $loc = Join-Path $dir 'Driver'
    if (Test-Path -LiteralPath $loc) {
        $inf = @(Get-ChildItem -LiteralPath $loc -Filter *.inf -Recurse -ErrorAction SilentlyContinue)
        if ($inf.Count -gt 0) {
            Write-Host ('Trovati ' + $inf.Count + ' file .inf: li installo.')
            pnputil /add-driver (Join-Path $loc '*.inf') /subdirs /install
        } else {
            Write-Host 'La cartella Driver non contiene file .inf.'
        }
    } else {
        Write-Host 'Nessuna cartella Driver accanto allo script (e'' facoltativa).'
    }

    Titolo '3/4 Cerco driver su Windows Update...'
    $svc = Get-Service -Name wuauserv -ErrorAction SilentlyContinue
    if (-not $svc -or $svc.StartType -eq 'Disabled') {
        Write-Host 'Windows Update e'' disattivato: non cerco driver online.' -ForegroundColor Yellow
        Write-Host 'Usa la cartella Driver o gli installer del produttore.'
    } else {
        try {
            $sess = New-Object -ComObject Microsoft.Update.Session
            $ric = $sess.CreateUpdateSearcher().Search("IsInstalled=0 and Type='Driver'")
            if ($ric.Updates.Count -eq 0) {
                Write-Host 'Nessun driver nuovo trovato.'
            } else {
                Write-Host ('Driver trovati: ' + $ric.Updates.Count)
                foreach ($u in $ric.Updates) { Write-Host ('  - ' + $u.Title) }
                $r = Read-Host 'Li scarico e li installo? (S/N)'
                if ($r -match '^[sS]') {
                    $coll = New-Object -ComObject Microsoft.Update.UpdateColl
                    foreach ($u in $ric.Updates) {
                        if (-not $u.EulaAccepted) { $u.AcceptEula() }
                        [void]$coll.Add($u)
                    }
                    $dl = $sess.CreateUpdateDownloader()
                    $dl.Updates = $coll
                    [void]$dl.Download()
                    $in = $sess.CreateUpdateInstaller()
                    $in.Updates = $coll
                    $res = $in.Install()
                    if ($res.ResultCode -eq 2) {
                        Write-Host 'Driver installati.' -ForegroundColor Green
                    } else {
                        Write-Host ('Installazione non completata (codice ' + $res.ResultCode + ').') -ForegroundColor Yellow
                    }
                    if ($res.RebootRequired) { Write-Host 'Serve riavviare il PC.' }
                } else {
                    Write-Host 'Va bene, non installo nulla.'
                }
            }
        } catch {
            Write-Host ('Ricerca su Windows Update non riuscita: ' + $_.Exception.Message) -ForegroundColor Red
        }
    }

    Titolo '4/4 Controllo finale...'
    $ancora = Problemi
    if ($ancora.Count -eq 0) {
        Write-Host 'Nessun dispositivo con problemi di driver.' -ForegroundColor Green
    } else {
        Write-Host ('Dispositivi ancora senza driver: ' + $ancora.Count)
        Write-Host 'Cerca il driver sul sito del produttore e mettilo nella cartella Driver.'
    }
} catch {
    Write-Host ('ERRORE: ' + $_.Exception.Message) -ForegroundColor Red
}
