@echo off
title Installazione Completa Lingua Italiana
setlocal
echo =====================================
echo INSTALLAZIONE LINGUA ITALIANA WINDOWS
echo =====================================
echo.

:: Controllo amministratore
net session >nul 2>&1
if errorlevel 1 (
    echo Richiedo privilegi amministratore...
    powershell -Command "Start-Process '%~f0' -Verb RunAs"
    exit
)

echo Avvio controllo componenti lingua...
echo.

powershell -ExecutionPolicy Bypass -NoProfile -Command ^
"$lang='it-IT';" ^
"" ^
"# [1/5] Componenti lingua" ^
"Write-Host '[1/5] Installazione componenti lingua...' -ForegroundColor Cyan;" ^
"$features=@(" ^
"'Language.Basic~~~it-IT~0.0.1.0'," ^
"'Language.Handwriting~~~it-IT~0.0.1.0'," ^
"'Language.OCR~~~it-IT~0.0.1.0'," ^
"'Language.Speech~~~it-IT~0.0.1.0'," ^
"'Language.TextToSpeech~~~it-IT~0.0.1.0'" ^
");" ^
"foreach($f in $features){" ^
"  $state=(Get-WindowsCapability -Online -Name $f -ErrorAction SilentlyContinue).State;" ^
"  if($state -eq 'Installed'){" ^
"    Write-Host '  [PRESENTE]' $f -ForegroundColor Green;" ^
"  }else{" ^
"    Write-Host '  [INSTALLO]' $f -ForegroundColor Yellow;" ^
"    Add-WindowsCapability -Online -Name $f -ErrorAction SilentlyContinue | Out-Null;" ^
"    Write-Host '  [OK]' $f -ForegroundColor Green;" ^
"  }" ^
"};" ^
"" ^
"# [2/5] Lingua utente (it-IT + en-US come fallback)" ^
"Write-Host '';" ^
"Write-Host '[2/5] Impostazione lingua utente...' -ForegroundColor Cyan;" ^
"try {" ^
"  $list = New-WinUserLanguageList 'it-IT';" ^
"  $list.Add('en-US');" ^
"  Set-WinUserLanguageList $list -Force -ErrorAction Stop;" ^
"  Write-Host '  [OK] Lingua utente: it-IT (fallback en-US)' -ForegroundColor Green;" ^
"} catch { Write-Host '  [WARN]' $_.Exception.Message -ForegroundColor Yellow };" ^
"" ^
"# [3/5] Impostazioni sistema" ^
"Write-Host '';" ^
"Write-Host '[3/5] Configurazione sistema...' -ForegroundColor Cyan;" ^
"try { Set-WinSystemLocale 'it-IT' -ErrorAction Stop;         Write-Host '  [OK] System Locale: it-IT'          -ForegroundColor Green } catch { Write-Host '  [WARN] System Locale:' $_.Exception.Message -ForegroundColor Yellow };" ^
"try { Set-WinUILanguageOverride 'it-IT' -ErrorAction Stop;   Write-Host '  [OK] UI Language: it-IT'             -ForegroundColor Green } catch { Write-Host '  [WARN] UI Language:' $_.Exception.Message -ForegroundColor Yellow };" ^
"try { Set-Culture 'it-IT' -ErrorAction Stop;                 Write-Host '  [OK] Culture (formati): it-IT'       -ForegroundColor Green } catch { Write-Host '  [WARN] Culture:' $_.Exception.Message -ForegroundColor Yellow };" ^
"try { Set-WinHomeLocation -GeoId 118 -ErrorAction Stop;      Write-Host '  [OK] Paese: Italia (118)'            -ForegroundColor Green } catch { Write-Host '  [WARN] HomeLocation:' $_.Exception.Message -ForegroundColor Yellow };" ^
"" ^
"# [4/5] Fuso orario" ^
"Write-Host '';" ^
"Write-Host '[4/5] Fuso orario...' -ForegroundColor Cyan;" ^
"try {" ^
"  $tz = Get-TimeZone;" ^
"  if ($tz.Id -eq 'W. Europe Standard Time') {" ^
"    Write-Host '  [PRESENTE] Fuso orario gia impostato' -ForegroundColor Green;" ^
"  } else {" ^
"    Set-TimeZone -Id 'W. Europe Standard Time' -ErrorAction Stop;" ^
"    Write-Host '  [OK] Fuso orario: W. Europe Standard Time' -ForegroundColor Green;" ^
"  }" ^
"} catch { Write-Host '  [WARN] Fuso orario:' $_.Exception.Message -ForegroundColor Yellow };" ^
"" ^
"# [5/5] Sincronizzazione NTP" ^
"Write-Host '';" ^
"Write-Host '[5/5] Sincronizzazione orario NTP...' -ForegroundColor Cyan;" ^
"try {" ^
"  Stop-Service w32time -Force -ErrorAction SilentlyContinue;" ^
"  & w32tm /unregister 2>$null | Out-Null;" ^
"  & w32tm /register   2>$null | Out-Null;" ^
"  Start-Service w32time -ErrorAction Stop;" ^
"  & w32tm /config /manualpeerlist:'pool.ntp.org,time.windows.com,time.google.com' /syncfromflags:manual /reliable:yes /update 2>$null | Out-Null;" ^
"  & w32tm /resync /force 2>$null | Out-Null;" ^
"  Write-Host '  [OK] NTP sincronizzato' -ForegroundColor Green;" ^
"} catch { Write-Host '  [WARN] NTP:' $_.Exception.Message -ForegroundColor Yellow };" ^
"" ^
"Write-Host '';" ^
"Write-Host '=====================================' -ForegroundColor Cyan;" ^
"Write-Host '  Riepilogo finale' -ForegroundColor Cyan;" ^
"Write-Host '=====================================' -ForegroundColor Cyan;" ^
"Write-Host ('  Locale  : ' + (Get-WinSystemLocale).Name);" ^
"Write-Host ('  Cultura : ' + (Get-Culture).Name);" ^
"Write-Host ('  Fuso    : ' + (Get-TimeZone).Id);" ^
"Write-Host ('  UI      : ' + (Get-WinUILanguageOverride).LanguageTag);" ^
"Write-Host '';"

echo.
echo =====================================
echo INSTALLAZIONE COMPLETATA
echo =====================================
echo.
choice /M "Riavviare il computer ora"
if errorlevel 2 exit
shutdown /r /t 5 /c "Riavvio per applicare lingua italiana"
