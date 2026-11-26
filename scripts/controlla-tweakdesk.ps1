# Controlla-TweakDesk.ps1
# Verifica cosa e' stato disattivato/rimosso dal debloat di TweakDesk (livello Estremo) e
# stampa l'elenco dei servizi e delle app ancora presenti sul sistema.
#
# USO: aprire PowerShell COME AMMINISTRATORE e lanciare con un comando unico:
#   powershell -ExecutionPolicy Bypass -File .\controlla-tweakdesk.ps1
# oppure, per salvare anche su file:
#   powershell -ExecutionPolicy Bypass -File .\controlla-tweakdesk.ps1 | Tee-Object controllo-tweakdesk.txt

$ErrorActionPreference = 'SilentlyContinue'

function Titolo($t) {
    Write-Host ''
    Write-Host ('-' * 72) -ForegroundColor DarkGray
    Write-Host $t -ForegroundColor Cyan
    Write-Host ('-' * 72) -ForegroundColor DarkGray
}

if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Host 'Attenzione: senza i permessi di amministratore alcuni controlli risultano incompleti.' -ForegroundColor Yellow
}

Titolo 'WINDOWS DEFENDER'
$svc = Get-Service WinDefend -ErrorAction SilentlyContinue
$mp = Get-MpComputerStatus -ErrorAction SilentlyContinue
if ($svc) { "  Servizio WinDefend: $($svc.Status)" } else { '  Servizio WinDefend: non trovato (probabilmente rimosso)' }
if ($mp) { "  Protezione in tempo reale attiva: $($mp.RealTimeProtectionEnabled)" } else { '  Get-MpComputerStatus non risponde: Defender e'' probabilmente disattivato.' }

Titolo 'SMARTSCREEN'
$ss = Get-ItemProperty 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\System' -Name EnableSmartScreen -ErrorAction SilentlyContinue
if ($ss) { "  EnableSmartScreen (criterio): $($ss.EnableSmartScreen)  (0 = spento)" } else { '  Nessun criterio impostato: SmartScreen segue il predefinito di Windows.' }

Titolo 'MICROSOFT EDGE'
$edgePaths = @(
    "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe",
    "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe"
)
$edge = $edgePaths | Where-Object { Test-Path $_ }
if ($edge) { "  Presente: $edge" } else { '  Non trovato: Edge risulta rimosso.' }

Titolo "FUNZIONALITA' DI WINDOWS (Hyper-V, Sandbox, WSL, SMB1, Telnet, Fax...)"
$feat = @(
    'Microsoft-Hyper-V-All', 'Containers-DisposableClientVM', 'VirtualMachinePlatform',
    'Microsoft-Windows-Subsystem-Linux',
    'SMB1Protocol', 'TelnetClient', 'TFTP', 'WorkFolders-Client',
    'Printing-XPSServices-Features', 'FaxServicesClientPackage'
)
foreach ($f in $feat) {
    $s = Get-WindowsOptionalFeature -Online -FeatureName $f -ErrorAction SilentlyContinue
    if ($s) { '  {0,-35} {1}' -f $f, $s.State } else { '  {0,-35} (funzionalita'' non presente in questa edizione)' -f $f }
}

Titolo 'SERVIZIO FAX E WINDOWS UPDATE'
foreach ($name in 'Fax', 'wuauserv', 'bits') {
    $s = Get-Service $name -ErrorAction SilentlyContinue
    if ($s) { '  {0,-10} StartType={1,-10} Stato={2}' -f $name, $s.StartType, $s.Status } else { "  $name : servizio non trovato" }
}

Titolo 'SPAZIO: FILE DI WINDOWS UPDATE E WINSXS'
$sd = Join-Path $env:SystemRoot 'SoftwareDistribution'
if (Test-Path $sd) {
    $size = (Get-ChildItem $sd -Recurse -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum
    if ($null -eq $size) { $size = 0 }
    '  SoftwareDistribution: {0:N0} MB' -f ($size / 1MB)
} else {
    '  SoftwareDistribution: cartella non trovata (probabilmente svuotata).'
}
$wsxs = (Get-ChildItem (Join-Path $env:SystemRoot 'WinSxS') -Recurse -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum
if ($null -eq $wsxs) { $wsxs = 0 }
'  WinSxS: {0:N0} MB' -f ($wsxs / 1MB)

Titolo "APP RIMASTE PER L'UTENTE CORRENTE (AppX)"
Get-AppxPackage | Sort-Object Name | ForEach-Object { "  $($_.Name)" }

Titolo 'APP PREINSTALLATE PER I NUOVI UTENTI (provisioned, contano per chi crea un account dopo)'
Get-AppxProvisionedPackage -Online | Sort-Object DisplayName | ForEach-Object { "  $($_.DisplayName)" }

Titolo 'SERVIZI ANCORA AD AVVIO AUTOMATICO'
Get-Service | Where-Object { $_.StartType -eq 'Automatic' } | Sort-Object DisplayName |
    ForEach-Object { '  {0,-30} {1}' -f $_.Name, $_.Status }

Write-Host ''
Write-Host 'Fatto. Per salvare questo elenco in un file, rilancia cosi'':' -ForegroundColor Green
Write-Host '  powershell -ExecutionPolicy Bypass -File .\controlla-tweakdesk.ps1 | Tee-Object controllo-tweakdesk.txt' -ForegroundColor Green
