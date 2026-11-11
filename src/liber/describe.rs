//! Breve descrizione della funzione di un programma, riconosciuta dal nome della cartella o
//! dell'installer (senza versione né maiuscole).

use crate::liber::catalog::normalize;

/// (parte del nome normalizzato, descrizione). Vince la parte più lunga che compare nel nome.
const TABLE: &[(&str, &str)] = &[
    ("1by1", "Lettore audio leggero"),
    ("duplicatemp3finder", "Trova i file MP3 doppi"),
    ("lyricsfinder", "Cerca i testi delle canzoni"),
    ("minilyrics", "Mostra i testi mentre ascolti musica"),
    ("mediamonkey", "Gestore e lettore di musica"),
    ("musicbee", "Gestore e lettore di musica"),
    ("mp3gain", "Porta al giusto volume gli MP3"),
    ("obs", "Registra e trasmette lo schermo"),
    ("similarity", "Trova brani audio simili o doppi"),
    ("wavepad", "Editor audio"),
    ("audacity", "Editor audio"),
    ("dvddecrypter", "Copia i DVD sul disco"),
    ("dvdshrink", "Riduce e copia i DVD"),
    ("datarecovery", "Recupera file cancellati"),
    ("recupero", "Recupera file cancellati"),
    ("getdataback", "Recupera dati da dischi danneggiati"),
    ("everything", "Ricerca istantanea dei file"),
    ("lupasrename", "Rinomina molti file insieme"),
    ("renamer", "Rinomina molti file insieme"),
    ("rinomina", "Rinomina molti file insieme"),
    ("pathlengthchecker", "Trova percorsi di file troppo lunghi"),
    ("ausolig", "Ottimizza la connessione a internet"),
    (
        "avastbrowsercleanup",
        "Pulisce il browser da estensioni indesiderate",
    ),
    ("chrome", "Browser web"),
    ("firefox", "Browser web"),
    ("thorium", "Browser web veloce basato su Chromium"),
    ("thunderbird", "Programma di posta elettronica"),
    (
        "completeinternetrepair",
        "Ripara i problemi di rete e internet",
    ),
    ("dnsjumper", "Cambia il server DNS"),
    ("emulemorph", "Condivisione di file peer-to-peer"),
    ("emule", "Condivisione di file peer-to-peer"),
    ("netscan", "Scansiona i dispositivi della rete"),
    ("networx", "Misura il traffico di rete"),
    ("qbittorrent", "Scarica file con il protocollo torrent"),
    ("tcpoptimizer", "Ottimizza i parametri della connessione"),
    ("tor", "Navigazione anonima con Tor"),
    ("camstudio", "Registra lo schermo in un video"),
    ("eoswebcam", "Usa una fotocamera Canon come webcam"),
    ("registrawebcam", "Registra video dalla webcam"),
    ("photodesk", "Fotoritocco"),
    ("windowsmoviemaker", "Montaggio video"),
    ("xmediarecode", "Converte audio e video"),
    ("xvid4psp", "Converte video"),
    ("produkey", "Mostra i codici prodotto di Windows e Office"),
    ("windowskeyfinder", "Mostra il codice prodotto di Windows"),
    ("onlyoffice", "Documenti, fogli di calcolo e presentazioni"),
    ("pdfreducer", "Riduce il peso dei PDF"),
    ("checkdisk", "Controlla e ripara il disco"),
    ("dism", "Gestisce e pulisce Windows (DISM)"),
    ("keepassxc", "Gestore di password"),
    ("processhacker", "Monitor avanzato dei processi"),
    ("pulizia", "Elimina i residui dei programmi disinstallati"),
    ("ramrush", "Libera la memoria RAM"),
    ("rufus", "Crea chiavette USB avviabili"),
    ("starter", "Gestisce i programmi all'avvio"),
    ("ventoy", "Chiavetta USB con più ISO avviabili"),
    ("notepad++", "Editor di testo e di codice"),
    ("sharex", "Cattura lo schermo e condivide le immagini"),
    ("ytdlp", "Scarica video da siti web"),
    ("bulkcrap", "Disinstalla i programmi in blocco"),
    ("bcuninstaller", "Disinstalla i programmi in blocco"),
    ("dbbrowser", "Gestisce database SQLite"),
    ("7zip", "Crea ed estrae archivi compressi"),
    ("vlc", "Lettore di video e audio"),
    ("winrar", "Crea ed estrae archivi compressi"),
];

/// Descrizione del programma, se è tra quelli noti.
pub fn of(name: &str) -> Option<&'static str> {
    let n = normalize(name);
    TABLE
        .iter()
        // Le chiavi corte («obs», «tor») devono aprire il nome, altrimenti «Monitor» o «Jobs»
        // verrebbero scambiati per un altro programma.
        .filter(|(k, _)| {
            if k.len() <= 4 {
                n.starts_with(k)
            } else {
                n.contains(k)
            }
        })
        .max_by_key(|(k, _)| k.len())
        .map(|(_, d)| *d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_programs_are_recognised_with_versions_and_noise() {
        assert_eq!(of("KeePassXC-2.7.11-Win64"), Some("Gestore di password"));
        assert_eq!(of("Rufus 2.11"), Some("Crea chiavette USB avviabili"));
        assert_eq!(
            of("ventoy-1.1.10"),
            Some("Chiavetta USB con più ISO avviabili")
        );
        assert_eq!(
            of("qBittorrentPortable_5.1.4.paf"),
            Some("Scarica file con il protocollo torrent")
        );
        assert_eq!(
            of("processhacker-2.39-bin"),
            Some("Monitor avanzato dei processi")
        );
        assert_eq!(of("Notepad++"), Some("Editor di testo e di codice"));
        assert_eq!(of("Firefox Setup 149.0"), Some("Browser web"));
        assert_eq!(of("Programma sconosciuto"), None);
        assert_eq!(of("tor -15.0.7"), Some("Navigazione anonima con Tor"));
        assert_eq!(of("OBSPortable"), Some("Registra e trasmette lo schermo"));
        assert_eq!(
            of("Dism++10.1.1002.1B"),
            Some("Gestisce e pulisce Windows (DISM)")
        );
        // Chiavi corte: solo a inizio nome.
        assert_eq!(of("Monitor"), None);
        assert_eq!(of("Doctor Web"), None);
        assert_eq!(of("Jobs"), None);
        assert_eq!(of(""), None);
    }

    #[test]
    fn the_longest_match_wins() {
        // «emulemorph» contiene anche «emule»; «lupasrename» anche «rename».
        assert_eq!(of("emuleMorph"), Some("Condivisione di file peer-to-peer"));
        assert_eq!(of("Mediamonkey 5"), Some("Gestore e lettore di musica"));
    }

    #[test]
    fn keys_are_normalized_and_descriptions_are_short() {
        for (k, d) in TABLE {
            assert_eq!(&normalize(k), k, "chiave non normalizzata: {k}");
            assert!(d.chars().count() <= 48, "troppo lunga: {d}");
        }
    }
}
