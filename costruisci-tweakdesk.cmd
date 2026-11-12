@echo off
rem Scarica (o aggiorna) e compila TweakDesk. Metti questo file in AmbienteRust.
title Costruisci TweakDesk
set "ROOT=%~dp0"
if "%ROOT:~-1%"=="\" set "ROOT=%ROOT:~0,-1%"
set "CARGO_HOME=%ROOT%\rust\cargo"
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

set "OUT=%PROG%\TweakDesk\dist\TweakDesk-portable"
if exist "%OUT%" rmdir /s /q "%OUT%"
mkdir "%OUT%"
copy /y "target\release\tweakdesk.exe" "%OUT%\TweakDesk.exe" >nul
if errorlevel 1 goto fail

echo.
echo FATTO: %OUT%\TweakDesk.exe
pause
exit /b 0

:fail
echo.
echo ERRORE: operazione non riuscita. Controlla i messaggi sopra.
pause
exit /b 1
