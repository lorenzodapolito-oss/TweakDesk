@echo off
title Installazione Lingua Italiana (da file locali)
setlocal
echo =====================================
echo INSTALLAZIONE LINGUA ITALIANA WINDOWS
echo =====================================
echo.

net session >nul 2>&1
if errorlevel 1 (
    echo Richiedo privilegi amministratore...
    powershell -Command "Start-Process '%~f0' -Verb RunAs"
    exit /b
)

set "SCRIPTDIR=%~dp0"
powershell -ExecutionPolicy Bypass -NoProfile -Command "$t=[IO.File]::ReadAllText('%~f0'); Invoke-Expression $t.Substring($t.IndexOf('#PS'+'#START'))"

echo.
echo =====================================
echo FINITO
echo =====================================
echo.
choice /M "Riavviare il computer ora"
if errorlevel 2 exit /b
shutdown /r /t 5 /c "Riavvio per applicare lingua italiana"
exit /b

#PS#START
# Nessun download e nessun Windows Update: i pacchetti si leggono solo da file locali.
$dir = $env:SCRIPTDIR.TrimEnd('\')
function Say($t, $c = 'Gray') { Write-Host $t -ForegroundColor $c }

# Dove cercare: Script\Lingue, la cartella Lingue accanto al programma, e le ISO montate
# ("Languages and Optional Features" / "Language Pack").
$roots = @()
foreach ($r in @((Join-Path $dir 'Lingue'), (Join-Path (Split-Path $dir -Parent) 'Lingue'))) {
    if (Test-Path -LiteralPath $r) { $roots += $r }
}
foreach ($d in (Get-PSDrive -PSProvider FileSystem)) {
    foreach ($s in 'LanguagesAndOptionalFeatures', 'x64\langpacks', 'LocalExperiencePack') {
        $p = Join-Path $d.Root $s
        if (Test-Path -LiteralPath $p) { $roots += $p }
    }
}
function Find-Files($filter) {
    foreach ($r in $roots) {
        Get-ChildItem -LiteralPath $r -Recurse -Filter $filter -File -ErrorAction SilentlyContinue
    }
}

Say '[1/6] Cerco i pacchetti lingua in locale...' 'Cyan'
foreach ($r in $roots) { Say ('  Cartella: ' + $r) }
$lp   = Find-Files 'Microsoft-Windows-Client-Language-Pack_x64_it-it.cab' | Select-Object -First 1
$lxp  = Find-Files 'LanguageExperiencePack.it-it.Neutral.appx' | Select-Object -First 1
$fodDirs = @(Find-Files '*it-it*.cab' | ForEach-Object { $_.DirectoryName } | Sort-Object -Unique)
if (-not $lp -and -not $lxp -and $fodDirs.Count -eq 0) {
    Say '  Nessun pacchetto italiano trovato. Non scarico nulla.' 'Yellow'
    Say ('  Copia i file .cab/.appx italiani in: ' + (Join-Path $dir 'Lingue')) 'Yellow'
    Say '  (oppure monta l''ISO "Languages and Optional Features" di Windows 11).' 'Yellow'
}

Say ''
Say '[2/6] Pacchetto lingua (interfaccia)...' 'Cyan'
try {
    $has = Get-WindowsPackage -Online -ErrorAction Stop | Where-Object { $_.PackageName -like '*LanguagePack*it-IT*' }
    if ($has) { Say '  [PRESENTE] Language Pack it-IT' 'Green' }
    elseif ($lp) {
        Add-WindowsPackage -Online -PackagePath $lp.FullName -NoRestart -ErrorAction Stop | Out-Null
        Say ('  [OK] ' + $lp.Name) 'Green'
    } else { Say '  [SALTATO] file non trovato' 'Yellow' }
} catch { Say ('  [WARN] ' + $_.Exception.Message) 'Yellow' }

Say ''
Say '[3/6] Local Experience Pack...' 'Cyan'
try {
    $has = Get-AppxProvisionedPackage -Online -ErrorAction Stop | Where-Object { $_.DisplayName -like '*LanguageExperiencePackit-IT*' }
    if ($has) { Say '  [PRESENTE] Local Experience Pack it-IT' 'Green' }
    elseif ($lxp) {
        $lic = Join-Path $lxp.DirectoryName 'License.xml'
        if (Test-Path -LiteralPath $lic) {
            Add-AppxProvisionedPackage -Online -PackagePath $lxp.FullName -LicensePath $lic -ErrorAction Stop | Out-Null
        } else {
            Add-AppxProvisionedPackage -Online -PackagePath $lxp.FullName -SkipLicense -ErrorAction Stop | Out-Null
        }
        Say ('  [OK] ' + $lxp.Name) 'Green'
    } else { Say '  [SALTATO] file non trovato' 'Yellow' }
} catch { Say ('  [WARN] ' + $_.Exception.Message) 'Yellow' }

Say ''
Say '[4/6] Componenti lingua (scrittura, OCR, voce)...' 'Cyan'
$features = @(
    'Language.Basic~~~it-IT~0.0.1.0',
    'Language.Handwriting~~~it-IT~0.0.1.0',
    'Language.OCR~~~it-IT~0.0.1.0',
    'Language.Speech~~~it-IT~0.0.1.0',
    'Language.TextToSpeech~~~it-IT~0.0.1.0'
)
foreach ($f in $features) {
    try {
        $state = (Get-WindowsCapability -Online -Name $f -ErrorAction Stop).State
        if ($state -eq 'Installed') { Say ('  [PRESENTE] ' + $f) 'Green' }
        elseif ($fodDirs.Count -eq 0) { Say ('  [SALTATO] ' + $f + ' (nessun file locale)') 'Yellow' }
        else {
            # -LimitAccess: usa solo -Source, non contatta Windows Update.
            Add-WindowsCapability -Online -Name $f -Source $fodDirs -LimitAccess -ErrorAction Stop | Out-Null
            Say ('  [OK] ' + $f) 'Green'
        }
    } catch { Say ('  [WARN] ' + $f + ': ' + $_.Exception.Message) 'Yellow' }
}

Say ''
Say '[5/6] Lingua, formati, fuso orario...' 'Cyan'
try {
    $list = New-WinUserLanguageList 'it-IT'
    $list.Add('en-US')
    Set-WinUserLanguageList $list -Force -ErrorAction Stop
    Say '  [OK] Lingua utente: it-IT (fallback en-US)' 'Green'
} catch { Say ('  [WARN] ' + $_.Exception.Message) 'Yellow' }
try { Set-WinSystemLocale 'it-IT' -ErrorAction Stop;       Say '  [OK] System Locale: it-IT' 'Green' } catch { Say ('  [WARN] System Locale: ' + $_.Exception.Message) 'Yellow' }
try { Set-WinUILanguageOverride 'it-IT' -ErrorAction Stop; Say '  [OK] UI Language: it-IT' 'Green' } catch { Say ('  [WARN] UI Language: ' + $_.Exception.Message) 'Yellow' }
try { Set-Culture 'it-IT' -ErrorAction Stop;               Say '  [OK] Formati: it-IT' 'Green' } catch { Say ('  [WARN] Culture: ' + $_.Exception.Message) 'Yellow' }
try { Set-WinHomeLocation -GeoId 118 -ErrorAction Stop;    Say '  [OK] Paese: Italia' 'Green' } catch { Say ('  [WARN] HomeLocation: ' + $_.Exception.Message) 'Yellow' }
try {
    if ((Get-TimeZone).Id -eq 'W. Europe Standard Time') { Say '  [PRESENTE] Fuso orario' 'Green' }
    else { Set-TimeZone -Id 'W. Europe Standard Time' -ErrorAction Stop; Say '  [OK] Fuso orario: W. Europe Standard Time' 'Green' }
} catch { Say ('  [WARN] Fuso orario: ' + $_.Exception.Message) 'Yellow' }

Say ''
Say '[6/6] Sincronizzazione orario NTP...' 'Cyan'
try {
    Stop-Service w32time -Force -ErrorAction SilentlyContinue
    & w32tm /unregister 2>$null | Out-Null
    & w32tm /register 2>$null | Out-Null
    Start-Service w32time -ErrorAction Stop
    & w32tm /config /manualpeerlist:'pool.ntp.org,time.windows.com,time.google.com' /syncfromflags:manual /reliable:yes /update 2>$null | Out-Null
    & w32tm /resync /force 2>$null | Out-Null
    Say '  [OK] NTP sincronizzato' 'Green'
} catch { Say ('  [WARN] NTP: ' + $_.Exception.Message) 'Yellow' }

Say ''
Say ('  Locale  : ' + (Get-WinSystemLocale).Name)
Say ('  Cultura : ' + (Get-Culture).Name)
Say ('  Fuso    : ' + (Get-TimeZone).Id)
Say ('  UI      : ' + (Get-WinUILanguageOverride).LanguageTag)
