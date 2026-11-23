@echo off
title Menu contestuale classico
setlocal

echo.
echo Attivo il menu contestuale classico (tasto destro completo)...
reg add "HKCU\Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32" /ve /t REG_SZ /d "" /f >nul
if errorlevel 1 (
    echo.
    echo ERRORE: non sono riuscito a scrivere nel registro.
    pause
    exit /b 1
)

echo Riavvio Esplora risorse per applicare la modifica...
taskkill /f /im explorer.exe >nul 2>&1
timeout /t 2 /nobreak >nul
tasklist /fi "imagename eq explorer.exe" 2>nul | find /i "explorer.exe" >nul
if errorlevel 1 start "" explorer.exe

echo.
echo Fatto: il tasto destro ora mostra subito il menu completo.
echo Per tornare al menu di Windows 11 usa "Menu contestuale di Windows 11".
echo.
pause
exit /b 0
