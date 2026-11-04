@echo off
title Mega Configurazione Windows
setlocal EnableDelayedExpansion
echo ====================================
echo        MEGA CONFIG WINDOWS
echo ====================================
echo.

:: Controllo amministratore
net session >nul 2>&1
if errorlevel 1 (
    echo Richiedo privilegi amministratore...
    powershell -Command "Start-Process '%~f0' -Verb RunAs"
    exit
)

echo Avvio configurazione sistema...
echo.

powershell -ExecutionPolicy Bypass -NoProfile -Command ^
"" ^
"# ── [1/5] Registro: privacy, telemetria, UI ─────────────────────────────────" ^
"Write-Host '[1/5] Impostazioni registro...' -ForegroundColor Cyan;" ^
"$configs = @(" ^
"  @{Name='Telemetria Windows';          Path='HKLM:\SOFTWARE\Policies\Microsoft\Windows\DataCollection';                   Key='AllowTelemetry';                            Value=0}," ^
"  @{Name='Suggerimenti Windows';        Path='HKCU:\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager';     Key='SystemPaneSuggestionsEnabled';              Value=0}," ^
"  @{Name='Esperienze personalizzate';   Path='HKCU:\Software\Microsoft\Windows\CurrentVersion\Privacy';                    Key='TailoredExperiencesWithDiagnosticDataEnabled'; Value=0}," ^
"  @{Name='Pubblicita Start';            Path='HKCU:\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager';     Key='SubscribedContent-338388Enabled';           Value=0}," ^
"  @{Name='App suggerite';               Path='HKCU:\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager';     Key='SubscribedContent-338389Enabled';           Value=0}," ^
"  @{Name='Barra applicazioni sinistra'; Path='HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced';          Key='TaskbarAl';                                 Value=0}," ^
"  @{Name='Tema sistema scuro';          Path='HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize';         Key='SystemUsesLightTheme';                      Value=0}," ^
"  @{Name='Tema app chiaro';             Path='HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize';         Key='AppsUseLightTheme';                         Value=1}," ^
"  @{Name='UAC disabilitato';            Path='HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System';            Key='EnableLUA';                                 Value=0}" ^
");" ^
"$taskbarChanged = $false;" ^
"$i = 1; $tot = $configs.Count;" ^
"foreach ($c in $configs) {" ^
"  Write-Host (\"  [$i/$tot] $($c.Name)\") -ForegroundColor Cyan;" ^
"  if (!(Test-Path $c.Path)) { New-Item -Path $c.Path -Force | Out-Null };" ^
"  $cur = (Get-ItemProperty -Path $c.Path -Name $c.Key -ErrorAction SilentlyContinue).$($c.Key);" ^
"  if ($cur -eq $c.Value) {" ^
"    Write-Host '         gia configurato' -ForegroundColor Green;" ^
"  } else {" ^
"    Set-ItemProperty -Path $c.Path -Name $c.Key -Value $c.Value -Type DWord;" ^
"    Write-Host '         applicato' -ForegroundColor Yellow;" ^
"    if ($c.Key -eq 'TaskbarAl') { $taskbarChanged = $true };" ^
"  };" ^
"  $i++;" ^
"};" ^
"" ^
"# ── [2/5] Risparmio energetico ──────────────────────────────────────────────" ^
"Write-Host '';" ^
"Write-Host '[2/5] Risparmio energetico...' -ForegroundColor Cyan;" ^
"$power = @(" ^
"  @{Name='[AC] Schermo spento 30 min'; Cmd={powercfg /SETACVALUEINDEX SCHEME_CURRENT SUB_VIDEO VIDEOIDLE 1800}}," ^
"  @{Name='[AC] Sospensione mai';       Cmd={powercfg /SETACVALUEINDEX SCHEME_CURRENT SUB_SLEEP STANDBYIDLE 0}}," ^
"  @{Name='[DC] Schermo spento 15 min'; Cmd={powercfg /SETDCVALUEINDEX SCHEME_CURRENT SUB_VIDEO VIDEOIDLE 900}}," ^
"  @{Name='[DC] Sospensione 30 min';    Cmd={powercfg /SETDCVALUEINDEX SCHEME_CURRENT SUB_SLEEP STANDBYIDLE 1800}}," ^
"  @{Name='Pulsante accensione -> Spegni (AC)'; Cmd={powercfg /SETACVALUEINDEX SCHEME_CURRENT SUB_BUTTONS PBUTTONACTION 3}}," ^
"  @{Name='Pulsante accensione -> Spegni (DC)'; Cmd={powercfg /SETDCVALUEINDEX SCHEME_CURRENT SUB_BUTTONS PBUTTONACTION 3}}," ^
"  @{Name='Coperchio -> Nessuna azione (AC)';   Cmd={powercfg /SETACVALUEINDEX SCHEME_CURRENT SUB_BUTTONS LIDACTION 0}}," ^
"  @{Name='Coperchio -> Nessuna azione (DC)';   Cmd={powercfg /SETDCVALUEINDEX SCHEME_CURRENT SUB_BUTTONS LIDACTION 0}}," ^
"  @{Name='Tasto sospensione -> Niente (AC)';   Cmd={powercfg /SETACVALUEINDEX SCHEME_CURRENT SUB_BUTTONS SBUTTONACTION 0}}," ^
"  @{Name='Tasto sospensione -> Niente (DC)';   Cmd={powercfg /SETDCVALUEINDEX SCHEME_CURRENT SUB_BUTTONS SBUTTONACTION 0}}" ^
");" ^
"foreach ($p in $power) {" ^
"  & $p.Cmd | Out-Null;" ^
"  Write-Host \"  [OK] $($p.Name)\" -ForegroundColor Green;" ^
"};" ^
"powercfg /SETACTIVE SCHEME_CURRENT | Out-Null;" ^
"Write-Host '  [OK] Piano energetico attivato' -ForegroundColor Green;" ^
"" ^
"# ── [3/5] Rete privata ──────────────────────────────────────────────────────" ^
"Write-Host '';" ^
"Write-Host '[3/5] Configurazione rete...' -ForegroundColor Cyan;" ^
"$nets = Get-NetConnectionProfile;" ^
"foreach ($n in $nets) {" ^
"  if ($n.NetworkCategory -ne 'Private') {" ^
"    Set-NetConnectionProfile -InterfaceAlias $n.InterfaceAlias -NetworkCategory Private;" ^
"    Write-Host \"  [OK] $($n.Name) -> Private\" -ForegroundColor Yellow;" ^
"  } else {" ^
"    Write-Host \"  [--] $($n.Name) gia Private\" -ForegroundColor Green;" ^
"  }" ^
"};" ^
"" ^
"# ── [4/5] Explorer (solo se necessario) ─────────────────────────────────────" ^
"Write-Host '';" ^
"Write-Host '[4/5] Riavvio Explorer...' -ForegroundColor Cyan;" ^
"if ($taskbarChanged) {" ^
"  Stop-Process -Name explorer -Force -ErrorAction SilentlyContinue;" ^
"  Start-Sleep -Seconds 1;" ^
"  Start-Process explorer;" ^
"  Write-Host '  [OK] Explorer riavviato (taskbar modificata)' -ForegroundColor Yellow;" ^
"} else {" ^
"  Write-Host '  [--] Non necessario' -ForegroundColor Green;" ^
"};" ^
"" ^
"# ── [5/5] Riepilogo ─────────────────────────────────────────────────────────" ^
"Write-Host '';" ^
"Write-Host '====================================' -ForegroundColor Cyan;" ^
"Write-Host '  Riepilogo' -ForegroundColor Cyan;" ^
"Write-Host '====================================' -ForegroundColor Cyan;" ^
"Write-Host '  UAC         : disabilitato (riavvio richiesto)' -ForegroundColor Yellow;" ^
"Write-Host '  Tema        : sistema scuro / app chiaro' -ForegroundColor Green;" ^
"Write-Host '  Taskbar     : allineata a sinistra' -ForegroundColor Green;" ^
"Write-Host '  Rete        : impostata come Private' -ForegroundColor Green;" ^
"Write-Host '  Energetico  : configurato' -ForegroundColor Green;" ^
"Write-Host '';"

echo.
echo ====================================
echo CONFIGURAZIONE COMPLETATA
echo Riavviare per applicare UAC.
echo ====================================
echo.
pause
