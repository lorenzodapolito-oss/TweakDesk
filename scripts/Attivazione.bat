@echo off
setlocal enabledelayedexpansion

:: ===========================================================
:: ATTIVAZIONE WINDOWS - VERSIONE STABILE
:: Con gestione errori e PAUSE per evitare chiusure improvvise
:: ===========================================================

:: Controllo amministratore con messaggio chiaro
net session >nul 2>&1
if %errorLevel% neq 0 (
    cls
    echo ===========================================================
    echo                    ERRORE CRITICO
    echo ===========================================================
    echo.
    echo [!] Devi eseguire questo script come AMMINISTRATORE!
    echo.
    echo [i] Clicca con il tasto destro sul file
    echo [i] Seleziona "Esegui come amministratore"
    echo.
    echo ===========================================================
    echo.
    pause
    exit /b 1
)

:: HEADER
cls
echo ===========================================================
echo    ATTIVAZIONE WINDOWS - RIPRISTINO LICENZA
echo ===========================================================
echo.
echo [i] Questo script verifichera' e riparera' l'attivazione
echo.
pause

:: ===========================================================
:: PARTE 1: VERIFICA STATO ATTUALE
:: ===========================================================
cls
echo ===========================================================
echo [1/5] VERIFICA STATO ATTIVAZIONE WINDOWS
echo ===========================================================
echo.

:: Raccogli informazioni sistema (con gestione errori)
echo [i] Raccolta informazioni sistema...

for /f "tokens=2 delims==" %%a in ('wmic os get caption /value 2^>nul') do set "OS_NAME=%%a"
if not defined OS_NAME set "OS_NAME=Non rilevabile"

for /f "tokens=2 delims==" %%a in ('wmic os get version /value 2^>nul') do set "OS_VERSION=%%a"
if not defined OS_VERSION set "OS_VERSION=Non rilevabile"

for /f "tokens=2 delims==" %%a in ('wmic os get buildnumber /value 2^>nul') do set "OS_BUILD=%%a"
if not defined OS_BUILD set "OS_BUILD=Non rilevabile"

echo [i] Sistema: %OS_NAME%
echo [i] Versione: %OS_VERSION% (Build %OS_BUILD%)
echo.

:: Verifica stato attivazione con slmgr (metodo batch puro)
echo [i] Verifica stato attivazione in corso...

:: Crea file temporaneo per output (path sicuro, senza spazi)
set "TEMP_FILE=%SystemRoot%\Temp\activation_status.txt"

:: Esegui slmgr e cattura output
cscript //nologo %windir%\system32\slmgr.vbs /dli > "%TEMP_FILE%" 2>&1

if %errorLevel% neq 0 (
    echo [ERRORE] Impossibile leggere stato licenza
    echo [i] Codice errore: %errorLevel%
    set "ACTIVATION_STATUS=ERRORE"
) else (
    :: Analizza output (ITA e ENG)
    findstr /C:"Stato licenza:" /C:"License Status:" "%TEMP_FILE%" >nul
    if !errorLevel! equ 0 (
        echo [OK] Stato licenza trovato:
        echo.
        findstr /C:"Stato licenza:" /C:"License Status:" "%TEMP_FILE%"
        findstr /C:"Canale di prodotto" /C:"Product Channel" "%TEMP_FILE%"
        findstr /C:"Partially product key" /C:"Partial Product Key" "%TEMP_FILE%"

        :: Determina se attivato (ITA e ENG)
        findstr /C:"Stato licenza: concesso in licenza" /C:"License Status: Licensed" /C:"permanently activated" "%TEMP_FILE%" >nul
        if !errorLevel! equ 0 (
            set "ACTIVATION_STATUS=ATTIVATO"
        ) else (
            set "ACTIVATION_STATUS=NON_ATTIVATO"
        )
    ) else (
        echo [ERRORE] Formato output non riconosciuto
        type "%TEMP_FILE%"
        set "ACTIVATION_STATUS=ERRORE"
    )
)

echo.
echo [i] Verifica scadenza...
cscript //nologo %windir%\system32\slmgr.vbs /xpr
echo.

:: Se gia' attivato, chiedi se continuare
:: FIX: usare !ACTIVATION_STATUS! con delayed expansion in tutto il blocco
if "!ACTIVATION_STATUS!"=="ATTIVATO" (
    echo [OK] Windows risulta GIA' ATTIVATO
    echo.
    set /p CONTINUE="Vuoi comunque ripristinare i servizi di attivazione? (S/N): "
    if /i not "!CONTINUE!"=="S" (
        echo.
        echo [i] Uscita senza modifiche.
        echo.
        del "%TEMP_FILE%" 2>nul
        pause
        exit /b 0
    )
) else if "!ACTIVATION_STATUS!"=="NON_ATTIVATO" (
    echo [!!] Windows NON ATTIVATO
) else (
    echo [??] Stato licenza non determinabile
)

del "%TEMP_FILE%" 2>nul
echo.
pause

:: ===========================================================
:: PARTE 2: RIPRISTINO SERVIZI ATTIVAZIONE
:: ===========================================================
cls
echo ===========================================================
echo [2/5] RIPRISTINO SERVIZI ATTIVAZIONE
echo ===========================================================
echo.

:: 2.1 Ripristina Windows Update
echo [1/4] Ripristino Windows Update...

sc config wuauserv start= demand >nul 2>&1
if %errorLevel% equ 0 (echo [OK] wuauserv configurato) else (echo [WARN] wuauserv non configurato)

sc config bits start= demand >nul 2>&1
if %errorLevel% equ 0 (echo [OK] bits configurato) else (echo [WARN] bits non configurato)

sc config cryptsvc start= auto >nul 2>&1
if %errorLevel% equ 0 (echo [OK] cryptsvc configurato) else (echo [WARN] cryptsvc non configurato)

:: Avvia servizi
net start wuauserv >nul 2>&1
if %errorLevel% equ 0 (echo [OK] wuauserv avviato) else (echo [WARN] wuauserv non avviato ^(potrebbe essere gia' attivo^))

net start bits >nul 2>&1
if %errorLevel% equ 0 (echo [OK] bits avviato) else (echo [WARN] bits non avviato ^(potrebbe essere gia' attivo^))

net start cryptsvc >nul 2>&1
if %errorLevel% equ 0 (echo [OK] cryptsvc avviato) else (echo [WARN] cryptsvc non avviato ^(potrebbe essere gia' attivo^))

echo.

:: 2.2 Ripristina servizi licenza
echo [2/4] Ripristino servizi licenza...

sc config slsvc start= demand >nul 2>&1
sc config sppsvc start= auto >nul 2>&1
sc config LicenseManager start= demand >nul 2>&1

net start slsvc >nul 2>&1
if %errorLevel% equ 0 (echo [OK] slsvc avviato) else (echo [WARN] slsvc non avviato ^(potrebbe essere gia' attivo^))

net start sppsvc >nul 2>&1
if %errorLevel% equ 0 (echo [OK] sppsvc avviato) else (echo [WARN] sppsvc non avviato ^(potrebbe essere gia' attivo^))

echo.

:: 2.3 Reset licenza
echo [3/4] Reset licenza Windows...

cscript //nologo %windir%\system32\slmgr.vbs /rilc >nul 2>&1
if %errorLevel% equ 0 (
    echo [OK] Licenza reinstallata
) else (
    echo [WARN] Reinstallazione licenza fallita (codice: %errorLevel%)
)

echo.
pause

:: ===========================================================
:: PARTE 3: PULIZIA FILE HOSTS
:: ===========================================================
cls
echo ===========================================================
echo [3/5] PULIZIA FILE HOSTS
echo ===========================================================
echo.

set "HOSTS_PATH=%SystemRoot%\System32\drivers\etc\hosts"

:: FIX: costruzione backup path sicura senza caratteri illegali nei nomi file
:: %DATE% e %TIME% contengono '/' e ':' che non sono validi nei nomi file su Windows
:: Si usa una sostituzione progressiva con variabili intermedie
set "RAW_DATE=%DATE:/=-%"
set "RAW_TIME=%TIME::=-%"
set "RAW_TIME=!RAW_TIME: =0!"
set "BACKUP_PATH=%HOSTS_PATH%.backup.!RAW_DATE!_!RAW_TIME!"

echo [i] Verifica presenza blocchi Microsoft...

if not exist "%HOSTS_PATH%" (
    echo [WARN] File hosts non trovato
    goto :skip_hosts
)

:: Crea backup
copy "%HOSTS_PATH%" "!BACKUP_PATH!" >nul 2>&1
if %errorLevel% equ 0 (
    echo [OK] Backup hosts creato: !BACKUP_PATH!
) else (
    echo [ERRORE] Impossibile creare backup
    goto :skip_hosts
)

:: Crea file temporaneo pulito
set "NEW_HOSTS=%SystemRoot%\Temp\hosts.new"
findstr /V /I "microsoft.com windowsupdate.com delivery.mp.microsoft.com" "%HOSTS_PATH%" > "%NEW_HOSTS%"

:: Sostituisci file originale
copy "%NEW_HOSTS%" "%HOSTS_PATH%" >nul 2>&1
if %errorLevel% equ 0 (
    echo [OK] Blocchi Microsoft rimossi
) else (
    echo [ERRORE] Impossibile aggiornare hosts
)

del "%NEW_HOSTS%" 2>nul
echo.

:skip_hosts
pause

:: ===========================================================
:: PARTE 4: VERIFICA CHIAVE BIOS
:: ===========================================================
cls
echo ===========================================================
echo [4/5] VERIFICA CHIAVE BIOS
echo ===========================================================
echo.

echo [i] Lettura chiave OEM dal BIOS...

set "OEM_KEY="
for /f "tokens=2 delims==" %%a in ('wmic path softwarelicensingservice get OA3xOriginalProductKey /value 2^>nul') do set "OEM_KEY=%%a"

if defined OEM_KEY (
    echo [OK] Chiave OEM trovata: !OEM_KEY!
    echo.
    set /p INSTALL_OEM="Vuoi installare questa chiave OEM? (S/N): "
    if /i "!INSTALL_OEM!"=="S" (
        echo.
        echo [i] Installazione chiave OEM...
        :: FIX: usare !OEM_KEY! con delayed expansion
        slmgr /ipk !OEM_KEY! >nul 2>&1
        if !errorLevel! equ 0 (
            echo [OK] Chiave OEM installata
        ) else (
            echo [ERRORE] Impossibile installare chiave OEM (codice: !errorLevel!)
        )
    )
) else (
    echo [INFO] Nessuna chiave OEM trovata nel BIOS
)

echo.
pause

:: ===========================================================
:: PARTE 5: TENTATIVO ATTIVAZIONE
:: ===========================================================
cls
echo ===========================================================
echo [5/5] TENTATIVO ATTIVAZIONE
echo ===========================================================
echo.

echo Scegli il metodo di attivazione:
echo.
echo [1] Attivazione automatica
echo [2] Attivazione OEM (licenza BIOS)
echo [3] Inserisci chiave manualmente
echo [4] Apri impostazioni attivazione
echo [5] Salta attivazione
echo.
set /p ACTIVATION_METHOD="Scegli [1-5]: "

if "!ACTIVATION_METHOD!"=="1" (
    echo.
    echo [i] Tentativo attivazione automatica...
    cscript //nologo %windir%\system32\slmgr.vbs /ato
    if !errorLevel! equ 0 (
        echo [OK] Comando attivazione eseguito
    ) else (
        echo [WARN] Attivazione fallita (codice: !errorLevel!)
    )
)

if "!ACTIVATION_METHOD!"=="2" (
    echo.
    :: FIX: usare !OEM_KEY! con delayed expansion
    if defined OEM_KEY (
        echo [i] Attivazione con chiave OEM...
        slmgr /ipk !OEM_KEY! >nul 2>&1
        cscript //nologo %windir%\system32\slmgr.vbs /ato
    ) else (
        echo [ERRORE] Nessuna chiave OEM disponibile
    )
)

if "!ACTIVATION_METHOD!"=="3" (
    echo.
    set /p MANUAL_KEY="Inserisci chiave prodotto (25 caratteri): "
    if defined MANUAL_KEY (
        echo.
        echo [i] Installazione chiave...
        slmgr /ipk !MANUAL_KEY! >nul 2>&1
        if !errorLevel! equ 0 (
            echo [OK] Chiave installata
            echo [i] Tentativo attivazione...
            cscript //nologo %windir%\system32\slmgr.vbs /ato
        ) else (
            echo [ERRORE] Chiave non valida (codice: !errorLevel!)
        )
    )
)

if "!ACTIVATION_METHOD!"=="4" (
    echo.
    echo [i] Apertura impostazioni attivazione...
    start ms-settings:activation
)

if "!ACTIVATION_METHOD!"=="5" (
    echo [i] Attivazione saltata
)

echo.
pause

:: ===========================================================
:: VERIFICA FINALE
:: ===========================================================
cls
echo ===========================================================
echo                    VERIFICA FINALE
echo ===========================================================
echo.

echo [i] Nuova verifica stato attivazione...
:: FIX: findstr con piu' pattern usa /C: separati
cscript //nologo %windir%\system32\slmgr.vbs /dli > "%SystemRoot%\Temp\activation_status.txt" 2>&1
findstr /C:"Stato licenza:" /C:"License Status:" /C:"permanently activated" "%SystemRoot%\Temp\activation_status.txt"
findstr /C:"Canale di prodotto" /C:"Product Channel" "%SystemRoot%\Temp\activation_status.txt"
del "%SystemRoot%\Temp\activation_status.txt" 2>nul
echo.
cscript //nologo %windir%\system32\slmgr.vbs /xpr
echo.

:: ===========================================================
:: REPORT FINALE
:: ===========================================================
echo ===========================================================
echo                    OPERAZIONE COMPLETATA
echo ===========================================================
echo.
echo [i] E' stato creato un backup del file hosts in:
echo     !BACKUP_PATH!
echo.
echo [i] Se hai riscontrato errori, prova a:
echo     1. Riavviare il computer
echo     2. Verificare la connessione internet
echo     3. Eseguire nuovamente lo script
echo.
echo ===========================================================
echo.

pause
exit /b 0
