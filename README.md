# TweakDesk

Due sezioni in una sola finestra: **Impostazioni di Windows** (estrai e applica le tue impostazioni) e **Programmi, estensioni e script** (ex InstallDesk: scansione e download dei programmi installati con winget, installazione silenziosa, estensioni di Chrome, script di configurazione di Windows). Grafica a colori pastello.

Programma Windows (Rust, egui) che **estrae le tue impostazioni personali** dal registro
e le **applica su qualsiasi altro PC**, con copia di sicurezza e possibilità di annullare.

## Come si usa
1. **Estrai da questo PC**: legge circa 100 impostazioni note (Explorer, barra, privacy, reattività, criteri, energia...) e mostra solo quelle **diverse da Windows di fabbrica**.
2. **Salva profilo…**: salva le voci selezionate in un file `.tweakdesk.json` (oppure **Esporta .reg…** per un normale file di registro).
3. Sull'altro PC: **Apri profilo o .reg…** (anche un `.reg` qualsiasi: vengono lette solo voci DWORD e stringa di HKCU/HKLM), scegli le voci e premi **Applica su questo PC**.
4. **Annulla…** ripristina i valori com'erano prima (e cancella quelli che non esistevano).

## Sicurezza
- Prima di ogni applicazione i valori attuali vengono salvati in `Backup\<data>\annulla.json`, insieme a un `.reg` di ogni chiave toccata.
- Dopo la scrittura ogni voce viene riletta e verificata.
- I gruppi **Aggiornamenti Windows** e **Sicurezza ridotta** sono deselezionati di default e chiedono conferma.
- Un profilo può scrivere solo in `HKCU` e `HKLM`.
- Le voci `HKLM` richiedono i privilegi di amministratore (pulsante «Riavvia come amministratore»).

## Note
- Usa `reg.exe` (indipendente dalla lingua di Windows). Compilazione: `costruisci-tweakdesk.cmd`.
- Si può passare un profilo o un `.reg` come argomento: `tweakdesk.exe profilo.tweakdesk.json`.
- Non copia file (per esempio i cursori «Elegant Dark» in `C:\Windows\Cursors`) né modifica servizi o attività pianificate.

## Programmi, estensioni e script (ex InstallDesk)

- **1 Scansiona il PC / 2 Scarica / 3 Installa**: elenca i programmi installati che winget sa scaricare, ne salva gli installer nella cartella predefinita (saltando quelli già aggiornati) e li installa in silenzio.
- **Estensioni Chrome**: estrae le estensioni di tutti i profili, salva un elenco (`.json` + pagina `.html`) e le reinstalla aprendo il Web Store a gruppi di 8 oppure con i criteri di Chrome (amministratore; `Rimuovi-installazione-forzata.reg` li annulla).
- **Configurazione Windows**: script incorporati (MegaConfig, Italiano + orario, Ottimizza, Attivazione, Blocca Windows Update, sposta cartelle su D:, esporta/importa menu Start e taskbar, Gallery). Quelli ad alto impatto chiedono conferma. `W11ClassicMenu.exe` va copiato nella cartella `Script`.
