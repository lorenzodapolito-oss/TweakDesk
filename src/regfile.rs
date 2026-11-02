//! Lettura e scrittura dei file .reg (valori DWORD e stringa).

use crate::model::{Entry, Value};
use regex::Regex;

#[derive(Clone, Debug, PartialEq)]
pub struct RegValue {
    pub key: String,
    pub name: String,
    pub value: Value,
}

/// `HKEY_CURRENT_USER\x` -> `HKCU\x`; altri rami restano invariati.
pub fn normalize_key(key: &str) -> String {
    let k = key.trim();
    for (long, short) in [
        ("HKEY_CURRENT_USER", "HKCU"),
        ("HKEY_LOCAL_MACHINE", "HKLM"),
    ] {
        if k.len() >= long.len() && k[..long.len()].eq_ignore_ascii_case(long) {
            return format!("{short}{}", &k[long.len()..]);
        }
    }
    k.to_string()
}

/// I file .reg di regedit sono UTF-16 LE con BOM; accetta anche UTF-8.
pub fn decode(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes))
            .into_owned()
    }
}

pub fn encode(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for u in text.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

/// Legge una stringa tra virgolette dall'inizio di `s` (che inizia con `"`).
/// Restituisce il testo senza escape e il resto dopo la virgoletta di chiusura.
fn read_quoted(s: &str) -> Option<(String, &str)> {
    let mut out = String::new();
    let mut chars = s.char_indices();
    chars.next().filter(|(_, c)| *c == '"')?;
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                let (_, n) = chars.next()?;
                out.push(n);
            }
            '"' => return Some((out, &s[i + 1..])),
            _ => out.push(c),
        }
    }
    None
}

fn join_continuations(text: &str) -> String {
    let norm = text.replace("\r\n", "\n");
    match Regex::new(r"\\\n[ \t]*") {
        Ok(re) => re.replace_all(&norm, "").into_owned(),
        Err(_) => norm,
    }
}

/// Operazione letta da un .reg: imposta un valore o lo cancella (`"nome"=-`).
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Set(RegValue),
    Delete { key: String, name: String },
}

pub fn parse(text: &str) -> Vec<RegValue> {
    parse_ops(text)
        .into_iter()
        .filter_map(|o| match o {
            Op::Set(v) => Some(v),
            Op::Delete { .. } => None,
        })
        .collect()
}

pub fn parse_ops(text: &str) -> Vec<Op> {
    let mut out = Vec::new();
    let mut key: Option<String> = None;
    for raw in join_continuations(text).lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix('[') {
            key = if rest.starts_with('-') {
                None
            } else {
                rest.strip_suffix(']').map(normalize_key)
            };
            continue;
        }
        let Some(k) = &key else { continue };
        let (name, rest) = if let Some(r) = line.strip_prefix('@') {
            (String::new(), r)
        } else if line.starts_with('"') {
            match read_quoted(line) {
                Some(x) => x,
                None => continue,
            }
        } else {
            continue;
        };
        let Some(data) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let data = data.trim();
        if data == "-" {
            out.push(Op::Delete {
                key: k.clone(),
                name,
            });
            continue;
        }
        let value = if let Some(h) = data.strip_prefix("dword:") {
            match u32::from_str_radix(h.trim(), 16) {
                Ok(n) => Value::Dword(n),
                Err(_) => continue,
            }
        } else if data.starts_with('"') {
            match read_quoted(data) {
                Some((s, _)) => Value::Sz(s),
                None => continue,
            }
        } else {
            continue;
        };
        out.push(Op::Set(RegValue {
            key: k.clone(),
            name,
            value,
        }));
    }
    out
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn long_key(key: &str) -> String {
    let up = key.to_uppercase();
    if up.starts_with("HKCU\\") {
        format!("HKEY_CURRENT_USER{}", &key[4..])
    } else if up.starts_with("HKLM\\") {
        format!("HKEY_LOCAL_MACHINE{}", &key[4..])
    } else {
        key.to_string()
    }
}

fn value_name(name: &str) -> String {
    if name.is_empty() {
        "@".to_string()
    } else {
        format!("\"{}\"", escape(name))
    }
}

/// Testo .reg con operazioni di impostazione e cancellazione, raggruppate per chiave.
pub fn build(ops: &[Op]) -> String {
    let key_of = |o: &Op| match o {
        Op::Set(v) => v.key.clone(),
        Op::Delete { key, .. } => key.clone(),
    };
    let mut keys: Vec<String> = Vec::new();
    for o in ops {
        let k = key_of(o);
        if !keys.iter().any(|x| x.eq_ignore_ascii_case(&k)) {
            keys.push(k);
        }
    }
    let mut s = String::from("Windows Registry Editor Version 5.00\r\n\r\n");
    for k in &keys {
        s.push_str(&format!("[{}]\r\n", long_key(k)));
        for o in ops.iter().filter(|o| key_of(o).eq_ignore_ascii_case(k)) {
            match o {
                Op::Set(v) => match &v.value {
                    Value::Dword(n) => {
                        s.push_str(&format!("{}=dword:{n:08x}\r\n", value_name(&v.name)))
                    }
                    Value::Sz(t) => {
                        s.push_str(&format!("{}=\"{}\"\r\n", value_name(&v.name), escape(t)))
                    }
                },
                Op::Delete { name, .. } => s.push_str(&format!("{}=-\r\n", value_name(name))),
            }
        }
        s.push_str("\r\n");
    }
    s
}

/// Testo .reg con le voci indicate, raggruppate per chiave.
pub fn export(entries: &[&Entry]) -> String {
    let mut keys: Vec<&str> = Vec::new();
    for e in entries {
        if !keys.iter().any(|k| k.eq_ignore_ascii_case(&e.key)) {
            keys.push(&e.key);
        }
    }
    let mut s = String::from("Windows Registry Editor Version 5.00\r\n\r\n");
    for k in keys {
        s.push_str(&format!("[{}]\r\n", long_key(k)));
        for e in entries.iter().filter(|e| e.key.eq_ignore_ascii_case(k)) {
            let name = if e.name.is_empty() {
                "@".to_string()
            } else {
                format!("\"{}\"", escape(&e.name))
            };
            match &e.value {
                Value::Dword(n) => s.push_str(&format!("{name}=dword:{n:08x}\r\n")),
                Value::Sz(v) => s.push_str(&format!("{name}=\"{}\"\r\n", escape(v))),
            }
        }
        s.push_str("\r\n");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Baseline, Risk};

    fn e(key: &str, name: &str, value: Value) -> Entry {
        Entry {
            group: "g".into(),
            label: "l".into(),
            key: key.into(),
            name: name.into(),
            value,
            baseline: Baseline::Absent,
            risk: Risk::Normal,
        }
    }

    #[test]
    fn parses_dword_string_default_and_escapes() {
        let text = "Windows Registry Editor Version 5.00\r\n\r\n[HKEY_CURRENT_USER\\Software\\X]\r\n\"Hidden\"=dword:00000001\r\n\"Path\"=\"C:\\\\Dir\\\\\"\r\n\"Q\\\"uote\"=\"a\\\"b\"\r\n@=\"def\"\r\n\"Bin\"=hex:01,02,\\\r\n  03\r\n\r\n[HKEY_LOCAL_MACHINE\\SOFTWARE\\Y]\r\n\"N\"=dword:ffffffff\r\n[-HKEY_CURRENT_USER\\Gone]\r\n\"Z\"=dword:00000001\r\n";
        let v = parse(text);
        assert_eq!(v.len(), 5);
        assert_eq!(
            v[0],
            RegValue {
                key: "HKCU\\Software\\X".into(),
                name: "Hidden".into(),
                value: Value::Dword(1)
            }
        );
        assert_eq!(v[1].value, Value::Sz("C:\\Dir\\".into()));
        assert_eq!(v[2].name, "Q\"uote");
        assert_eq!(v[2].value, Value::Sz("a\"b".into()));
        assert_eq!(
            v[3],
            RegValue {
                key: "HKCU\\Software\\X".into(),
                name: String::new(),
                value: Value::Sz("def".into())
            }
        );
        assert_eq!(
            v[4],
            RegValue {
                key: "HKLM\\SOFTWARE\\Y".into(),
                name: "N".into(),
                value: Value::Dword(u32::MAX)
            }
        );
    }

    #[test]
    fn utf16_with_bom_and_utf8_decode() {
        let text = "Windows Registry Editor Version 5.00\r\n[HKEY_CURRENT_USER\\àè]\r\n\"x\"=dword:00000002\r\n";
        let bytes = encode(text);
        assert_eq!(decode(&bytes), text);
        assert_eq!(decode(text.as_bytes()), text);
        let mut with_bom = vec![0xEF, 0xBB, 0xBF];
        with_bom.extend_from_slice(text.as_bytes());
        assert_eq!(decode(&with_bom), text);
        assert_eq!(parse(&decode(&bytes))[0].key, "HKCU\\àè");
    }

    #[test]
    fn export_then_parse_round_trips() {
        let entries = [
            e("HKCU\\Software\\A", "One", Value::Dword(7)),
            e("HKLM\\SOFTWARE\\B", "Two", Value::Sz("C:\\x \"q\"".into())),
            e("HKCU\\Software\\A", "", Value::Sz(String::new())),
        ];
        let refs: Vec<&Entry> = entries.iter().collect();
        let text = export(&refs);
        assert!(text.starts_with("Windows Registry Editor Version 5.00\r\n"));
        assert!(text.contains("[HKEY_CURRENT_USER\\Software\\A]"));
        assert!(text.contains("\"One\"=dword:00000007"));
        let back = parse(&text);
        assert_eq!(back.len(), 3);
        for (orig, got) in [(0, 0), (1, 2), (2, 1)]
            .iter()
            .map(|&(o, g)| (&entries[o], &back[g]))
        {
            assert_eq!(
                (&orig.key, &orig.name, &orig.value),
                (&got.key, &got.name, &got.value)
            );
        }
    }

    #[test]
    fn deletions_are_parsed_and_built() {
        let ops = vec![
            Op::Set(RegValue {
                key: "HKCU\\A".into(),
                name: "x".into(),
                value: Value::Dword(1),
            }),
            Op::Delete {
                key: "HKCU\\A".into(),
                name: "y".into(),
            },
            Op::Delete {
                key: "HKLM\\B".into(),
                name: String::new(),
            },
        ];
        let text = build(&ops);
        assert!(text.contains("\"y\"=-\r\n") && text.contains("@=-\r\n"));
        assert!(text.contains("[HKEY_LOCAL_MACHINE\\B]"));
        assert_eq!(parse_ops(&text), ops);
        assert_eq!(parse(&text).len(), 1);
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let v = parse("[HKEY_CURRENT_USER\\A]\n\"x\"=dword:zz\n\"y\"=\"aperta\n\"ok\"=dword:00000001\nspazzatura\n");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name, "ok");
        assert!(parse("\"x\"=dword:00000001\n").is_empty());
    }
}
