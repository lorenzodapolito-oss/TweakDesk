@echo off
rem Scarica (o aggiorna) e compila TweakDesk. Metti questo file in AmbienteRust.
title Costruisci TweakDesk
set "ROOT=%~dp0"
if "%ROOT:~-1%"=="\" set "ROOT=%ROOT:~0,-1%"
rem I pacchetti vanno estratti su un disco NTFS: exFAT/FAT32 (chiavette, dischi USB) non accettano le date
rem 1970 dei pacchetti e cargo fallisce con "failed to set mtime". Toolchain e programmi restano nella cartella.
set "CARGO_HOME=%LOCALAPPDATA%\AmbienteRust-cargo"
if not exist "%CARGO_HOME%" mkdir "%CARGO_HOME%"
set "RUSTUP_HOME=%ROOT%\rust\rustup"
rem Usa direttamente cargo e rustc della toolchain: non serve rustup (che su un PC nuovo prova a riscaricarla).
set "TC="
for /d %%t in ("%ROOT%\rust\rustup\toolchains\*") do if exist "%%t\bin\cargo.exe" set "TC=%%t\bin;"
set "PATH=%TC%%ROOT%\winlibs\mingw64\bin;%ROOT%\rust\cargo\bin;%ROOT%\git\cmd;%ROOT%\git\bin;%PATH%"
set "PROG=%ROOT%\Progetti"
if not exist "%PROG%" mkdir "%PROG%"
cd /d "%PROG%"
taskkill /f /im tweakdesk.exe >nul 2>&1

echo === Scarico o aggiorno TweakDesk ===
if exist "%PROG%\TweakDesk\.git" (
  git -c "safe.directory=*" -C "%PROG%\TweakDesk" fetch origin
  if errorlevel 1 goto fail
  git -c "safe.directory=*" -C "%PROG%\TweakDesk" reset --hard origin/main
) else (
  git clone https://github.com/lorenzodapolito-oss/TweakDesk "%PROG%\TweakDesk"
)
if errorlevel 1 goto fail

echo === Compilo ===
cd /d "%PROG%\TweakDesk"
cargo build --release
if errorlevel 1 goto fail

rem Se nel frattempo hai riaperto TweakDesk (per provarlo), il suo file resta occupato
rem e la copia darebbe "Accesso negato": lo richiudiamo anche qui, con un attimo di attesa.
taskkill /f /im TweakDesk.exe >nul 2>&1
if not errorlevel 1 timeout /t 2 /nobreak >nul

set "OUT=%PROG%\TweakDesk\dist\TweakDesk-portable"
set "TENTATIVI=0"
:copia
if exist "%OUT%" rmdir /s /q "%OUT%" 2>nul
mkdir "%OUT%" 2>nul
copy /y "target\release\tweakdesk.exe" "%OUT%\TweakDesk.exe" >nul 2>nul
if not errorlevel 1 goto copiato
set /a TENTATIVI+=1
if %TENTATIVI% geq 5 goto copia_fallita
echo File ancora occupato, riprovo tra un attimo...
taskkill /f /im TweakDesk.exe >nul 2>&1
timeout /t 2 /nobreak >nul
goto copia

:copia_fallita
echo.
echo Non riesco a sostituire "%OUT%\TweakDesk.exe": chiudi TweakDesk
echo (anche dalla barra delle applicazioni) e il suo antivirus, se lo sta
echo scansionando, poi premi un tasto per riprovare.
pause >nul
set "TENTATIVI=0"
goto copia

:copiato

echo.
echo FATTO: %OUT%\TweakDesk.exe
pause
exit /b 0

:fail
echo.
echo ERRORE: operazione non riuscita. Controlla i messaggi sopra.
pause
exit /b 1
