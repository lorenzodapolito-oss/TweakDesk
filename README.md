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

## Pulizia e avvio (alla CCleaner)

Terza sezione, **Pulizia e avvio**:

- **Pulizia**: «1 Analizza» mostra quanto spazio si libera, «2 Pulisci» elimina. Voci: file temporanei (utente e Windows), cestino, miniature, rapporti di errore, dump di memoria, registri, download di Windows Update, cache DNS e shader, cache di Chrome, Edge, Brave, Firefox (cookie e cronologia a parte, spenti di default), Discord, Spotify, Teams, Office, VS Code, Steam, NVIDIA. Nel gruppo **Avanzate**: componenti superati di WinSxS e `Windows.old` (spenti di default, con avviso).
- **Avvio automatico**: elenca i programmi che partono con Windows (chiavi Run, cartelle Esecuzione automatica, attività pianificate all'accesso). La casella li **disattiva/riattiva** come il Task Manager (reversibile); «Elimina» li rimuove salvando prima una copia in `BackupAvvio`.
- **Backup di sistema (install.wim)**: crea una copia ombra (VSS) del disco e la salva con `dism /Capture-Image` in un file `.wim` (come Dism++). Non sovrascrive file esistenti; esclude pagefile, hiberfil e il file stesso.

Sicurezza: ogni percorso viene espanso da variabili d'ambiente (se mancano, la voce è saltata), `..` e radici dei dischi sono rifiutati, i collegamenti simbolici non vengono seguiti, e i file in uso vengono saltati senza errori. Le voci che richiedono l'amministratore sono segnate e saltate se non lo sei.

## Debloat e ottimizzazione

Scheda **Debloat e ottimizzazione**, con due sottoschede:

- **Windows installato**: applica le voci scelte al sistema in uso. «Annulla» ripristina registro e servizi; app, funzionalità e file tolti non si recuperano. Non crea punti di ripristino.
- **ISO di Windows 11**: copia la ISO, applica le voci a ogni edizione scelta, pulisce `boot.wim`, sceglie la compressione (WIM o ESD), aggiunge `autounattend.xml` (account locale, lingua, salto dei controlli TPM/RAM, schermate iniziali, Compact OS) e ricrea la ISO avviabile (BIOS + UEFI) con lo strumento già presente in Windows (IMAPI), senza scaricare nulla. Se IMAPI non riesce, ripiega su `oscdimg` del Windows ADK (c’è il pulsante per scaricarlo e installarlo). Non integra driver.

Tre livelli: **Sicuro**, **Standard** (include il Sicuro) ed **Estremo** (tutto, anche Edge, Store, Defender e aggiornamenti: ogni voce rischiosa ha il suo avviso e si spunta a mano). .NET Framework non è mai rimovibile.

Servono i permessi di amministratore. Provare sempre prima in una macchina virtuale.

- **Cartella di lavoro sicura**: se il percorso scelto ha spazi, accenti o è troppo lungo, TweakDesk usa `X:\TDwork` sullo stesso disco e controlla lo spazio libero.
- **Aggiornamenti nella ISO**: puoi aggiungere file `.msu`/`.cab` (dal Catalogo di Microsoft Update); vengono integrati in ogni edizione con DISM prima delle voci scelte.
- **ISO da UUP dump**: indichi lo zip scaricato da uupdump.net, TweakDesk lo estrae in una cartella corta ed esegue lo script; la ISO prodotta diventa quella di partenza.
