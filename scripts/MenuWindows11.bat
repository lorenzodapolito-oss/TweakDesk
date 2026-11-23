@echo off
title Menu contestuale di Windows 11
setlocal

echo.
reg query "HKCU\Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}" >nul 2>&1
if errorlevel 1 (
    echo Il menu contestuale e' gia' quello di Windows 11: niente da fare.
    echo.
    pause
    exit /b 0
)

echo Ripristino il menu contestuale di Windows 11...
reg delete "HKCU\Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}" /f >nul
if errorlevel 1 (
    echo.
    echo ERRORE: non sono riuscito a modificare il registro.
    pause
    exit /b 1
)

echo Riavvio Esplora risorse per applicare la modifica...
taskkill /f /im explorer.exe >nul 2>&1
timeout /t 2 /nobreak >nul
tasklist /fi "imagename eq explorer.exe" 2>nul | find /i "explorer.exe" >nul
if errorlevel 1 start "" explorer.exe

echo.
echo Fatto: il tasto destro mostra di nuovo il menu di Windows 11.
echo.
pause
exit /b 0
