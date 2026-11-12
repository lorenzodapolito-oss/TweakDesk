//! Scansione e pulizia: espande i percorsi, calcola lo spazio, elimina i file senza mai uscire dai percorsi previsti.

use crate::clean::sys::Sys;
use crate::clean::targets::{Action, Item, Target};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Variabili d'ambiente (sostituibili nei test).
pub trait Env {
    fn var(&self, name: &str) -> Option<String>;
}

pub struct RealEnv;

impl Env for RealEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|v| !v.trim().is_empty())
    }
}

const MAX_DEPTH: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub bytes: u64,
    pub files: u64,
    pub failed: u64,
}

impl Stats {
    pub fn add(&mut self, o: Stats) {
        self.bytes = self.bytes.saturating_add(o.bytes);
        self.files = self.files.saturating_add(o.files);
        self.failed = self.failed.saturating_add(o.failed);
    }
}

fn expand_vars(pattern: &str, env: &dyn Env) -> Option<String> {
    let mut out = String::new();
    let mut rest = pattern;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let j = after.find('%')?;
        let name = &after[..j];
        if name.is_empty() {
            return None;
        }
        out.push_str(&env.var(name)?);
        rest = &after[j + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// Confronto con `*` (qualsiasi sequenza) senza distinguere maiuscole.
pub fn wild_match(pat: &str, name: &str) -> bool {
    let p: Vec<char> = pat.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    let (mut pi, mut ni) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while ni < n.len() {
        if p.get(pi) == Some(&'*') {
            star = Some(pi);
            mark = ni;
            pi += 1;
        } else if p.get(pi).is_some_and(|c| *c == n[ni]) {
            pi += 1;
            ni += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    while p.get(pi) == Some(&'*') {
        pi += 1;
    }
    pi == p.len()
}

fn split_segments(s: &str) -> Vec<&str> {
    s.split(['\\', '/']).collect()
}

fn expand_segments(base: PathBuf, segs: &[&str], out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 16 {
        return;
    }
    let Some((first, rest)) = segs.split_first() else {
        if base.symlink_metadata().is_ok() {
            out.push(base);
        }
        return;
    };
    if first.is_empty() {
        expand_segments(base, rest, out, depth + 1);
        return;
    }
    if first.contains('*') {
        let Ok(rd) = fs::read_dir(&base) else {
            return;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if wild_match(first, &name) {
                expand_segments(base.join(&name), rest, out, depth + 1);
            }
        }
    } else {
        expand_segments(base.join(first), rest, out, depth + 1);
    }
}

/// Espande un percorso con variabili e `*`. Rifiuta `..`, variabili mancanti e percorsi troppo vicini alla radice.
pub fn resolve(pattern: &str, env: &dyn Env) -> Vec<PathBuf> {
    let Some(full) = expand_vars(pattern, env) else {
        return Vec::new();
    };
    let segs = split_segments(&full);
    if segs.contains(&"..") {
        return Vec::new();
    }
    // La radice può essere "C:" (prima parte) oppure "/" (prova su Linux).
    let (start, rest): (PathBuf, &[&str]) = match segs.first() {
        Some(&"") => (PathBuf::from("/"), segs.get(1..).unwrap_or(&[])),
        Some(first) if first.ends_with(':') => (
            PathBuf::from(format!("{first}\\")),
            segs.get(1..).unwrap_or(&[]),
        ),
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    expand_segments(start, rest, &mut out, 0);
    out.retain(|p| {
        p.components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .count()
            >= 2
    });
    out
}

fn len_of(m: &fs::Metadata) -> u64 {
    if m.is_file() {
        m.len()
    } else {
        0
    }
}

fn walk(dir: &Path, depth: usize, s: &mut Stats) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else {
        s.failed += 1;
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(m) = p.symlink_metadata() else {
            continue;
        };
        if m.is_dir() && !m.file_type().is_symlink() {
            walk(&p, depth + 1, s);
        } else {
            s.files += 1;
            s.bytes = s.bytes.saturating_add(len_of(&m));
        }
    }
}

fn remove_file_any(p: &Path) -> bool {
    if fs::remove_file(p).is_ok() {
        return true;
    }
    if let Ok(m) = p.symlink_metadata() {
        let mut perm = m.permissions();
        if perm.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            if fs::set_permissions(p, perm).is_ok() && fs::remove_file(p).is_ok() {
                return true;
            }
        }
        if m.file_type().is_symlink() && fs::remove_dir(p).is_ok() {
            return true;
        }
    }
    false
}

fn empty_dir(dir: &Path, depth: usize, s: &mut Stats) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else {
        s.failed += 1;
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(m) = p.symlink_metadata() else {
            continue;
        };
        if m.is_dir() && !m.file_type().is_symlink() {
            empty_dir(&p, depth + 1, s);
            let _ = fs::remove_dir(&p);
        } else if remove_file_any(&p) {
            s.files += 1;
            s.bytes = s.bytes.saturating_add(len_of(&m));
        } else {
            s.failed += 1;
        }
    }
}

fn each_path(item: &Item, env: &dyn Env) -> Vec<PathBuf> {
    match item {
        Item::Path(p) => resolve(p, env),
        Item::Action(_) => Vec::new(),
    }
}

/// Spazio che libererebbe la voce (le azioni non hanno una dimensione nota).
pub fn scan(target: &Target, env: &dyn Env) -> Stats {
    let mut s = Stats::default();
    for item in target.items {
        if let Item::Action(Action::WindowsOld) = item {
            if let Some(d) = env.var("SystemDrive") {
                walk(&PathBuf::from(format!("{d}\\Windows.old")), 0, &mut s);
            }
        }
        for p in each_path(item, env) {
            match p.symlink_metadata() {
                Ok(m) if m.is_dir() && !m.file_type().is_symlink() => walk(&p, 0, &mut s),
                Ok(m) => {
                    s.files += 1;
                    s.bytes = s.bytes.saturating_add(len_of(&m));
                }
                Err(_) => {}
            }
        }
    }
    s
}

fn run_action(a: Action, sys: &dyn Sys, env: &dyn Env) -> Result<(), String> {
    match a {
        Action::RecycleBin => sys
            .run(
                "powershell",
                &[
                    "-NoProfile",
                    "-Command",
                    "Clear-RecycleBin -Force -ErrorAction SilentlyContinue",
                ],
            )
            .map(|_| ()),
        Action::FlushDns => sys.run("ipconfig", &["/flushdns"]).map(|_| ()),
        Action::ComponentCleanup => sys
            .run(
                "dism",
                &["/Online", "/Cleanup-Image", "/StartComponentCleanup"],
            )
            .map(|_| ()),
        Action::WindowsOld => {
            let drive = env
                .var("SystemDrive")
                .ok_or("unità di sistema sconosciuta")?;
            let dir = format!("{drive}\\Windows.old");
            if !Path::new(&dir).exists() {
                return Ok(());
            }
            // Senza testo localizzato: SID degli amministratori (S-1-5-32-544).
            sys.run(
                "icacls",
                &[&dir, "/setowner", "*S-1-5-32-544", "/t", "/c", "/q"],
            )?;
            sys.run(
                "icacls",
                &[&dir, "/grant", "*S-1-5-32-544:F", "/t", "/c", "/q"],
            )?;
            let cmd = format!(
                "Remove-Item -LiteralPath '{}' -Recurse -Force -ErrorAction SilentlyContinue",
                dir.replace('\'', "''")
            );
            sys.run("powershell", &["-NoProfile", "-Command", &cmd])
                .map(|_| ())
        }
    }
}

/// Risultato della pulizia di una voce.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub stats: Stats,
    pub error: Option<String>,
}

pub fn clean(target: &Target, env: &dyn Env, sys: &dyn Sys) -> Outcome {
    let mut out = Outcome::default();
    if target.admin && !sys.is_admin() {
        out.error = Some("richiede i permessi di amministratore".into());
        return out;
    }
    // Ferma solo i servizi che girano e li riaccende solo se girano: un servizio spento
    // (per esempio Windows Update bloccato dall'utente) non deve mai essere avviato da qui.
    let running: Vec<&str> = target
        .services
        .iter()
        .copied()
        .filter(|s| is_running(sys, s))
        .collect();
    for s in &running {
        let _ = sys.run("net", &["stop", s]);
    }
    for item in target.items {
        match item {
            Item::Action(a) => {
                let before = if *a == Action::WindowsOld {
                    scan(target, env)
                } else {
                    Stats::default()
                };
                match run_action(*a, sys, env) {
                    Ok(()) => out.stats.add(before),
                    Err(e) => out.error = Some(e),
                }
            }
            Item::Path(_) => {
                for p in each_path(item, env) {
                    match p.symlink_metadata() {
                        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {
                            empty_dir(&p, 0, &mut out.stats)
                        }
                        Ok(m) => {
                            if remove_file_any(&p) {
                                out.stats.files += 1;
                                out.stats.bytes = out.stats.bytes.saturating_add(len_of(&m));
                            } else {
                                out.stats.failed += 1;
                            }
                        }
                        Err(_) => {}
                    }
                }
            }
        }
    }
    for s in &running {
        let _ = sys.run("net", &["start", s]);
    }
    out
}

/// True se il servizio è in esecuzione (`sc query`); nel dubbio, falso: meglio non toccarlo.
fn is_running(sys: &dyn Sys, service: &str) -> bool {
    sys.run("sc", &["query", service])
        .is_ok_and(|o| o.contains("RUNNING"))
}

/// «1,5 GB», «320 KB»...
pub fn human(bytes: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", U[i]).replace('.', ",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::sys::fake::FakeSys;
    use crate::clean::targets::{find, Group};
    use std::collections::HashMap;

    struct TestEnv(HashMap<String, String>);
    impl Env for TestEnv {
        fn var(&self, name: &str) -> Option<String> {
            self.0.get(name).cloned()
        }
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tdclean_{}_{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn env_of(pairs: &[(&str, &Path)]) -> TestEnv {
        TestEnv(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string_lossy().replace('\\', "/")))
                .collect(),
        )
    }

    fn write(p: &Path, n: usize) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![b'x'; n]).unwrap();
    }

    #[test]
    fn wildcard_matching() {
        assert!(wild_match("thumbcache_*.db", "ThumbCache_256.db"));
        assert!(!wild_match("thumbcache_*.db", "thumbcache_256.dat"));
        assert!(wild_match("*", "qualsiasi"));
        assert!(wild_match("a*b*c", "aXXbYYc"));
        assert!(!wild_match("a*b*c", "aXXbYY"));
    }

    #[test]
    fn missing_variable_or_dotdot_resolves_to_nothing() {
        let env = TestEnv(HashMap::new());
        assert!(resolve("%TEMP%", &env).is_empty());
        let d = tmp("dd");
        let env = env_of(&[("X", &d)]);
        assert!(resolve("%X%/../etc", &env).is_empty());
        assert!(resolve("niente", &env).is_empty());
    }

    #[test]
    fn root_is_never_a_target() {
        let env = TestEnv([("TEMP".to_string(), "/".to_string())].into());
        assert!(resolve("%TEMP%", &env).is_empty());
    }

    #[test]
    fn scan_then_clean_user_temp() {
        let d = tmp("temp");
        let temp = d.join("Temp");
        write(&temp.join("a.tmp"), 1000);
        write(&temp.join("sub/b.tmp"), 500);
        write(&temp.join("sub/deep/c.tmp"), 250);
        let env = env_of(&[("TEMP", &temp)]);
        let t = find("temp_user").unwrap();
        let s = scan(t, &env);
        assert_eq!((s.files, s.bytes), (3, 1750));
        let out = clean(t, &env, &FakeSys::default());
        assert_eq!(
            (out.stats.files, out.stats.bytes, out.stats.failed),
            (3, 1750, 0)
        );
        assert!(temp.exists(), "la cartella Temp resta");
        assert_eq!(fs::read_dir(&temp).unwrap().count(), 0);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn wildcard_profiles_only_touch_cache_folders() {
        let d = tmp("chrome");
        let la = d.join("Local");
        let ud = la.join("Google/Chrome/User Data");
        write(&ud.join("Default/Cache/f1"), 100);
        write(&ud.join("Profile 1/Cache/f2"), 200);
        write(&ud.join("Default/Bookmarks"), 50);
        write(&ud.join("Default/History"), 70);
        let env = env_of(&[("LOCALAPPDATA", &la)]);
        let t = find("chrome_cache").unwrap();
        let out = clean(t, &env, &FakeSys::default());
        assert_eq!(out.stats.bytes, 300);
        assert!(ud.join("Default/Bookmarks").exists());
        assert!(ud.join("Default/History").exists());
        assert!(ud.join("Default/Cache").exists());
        assert!(!ud.join("Default/Cache/f1").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn history_target_removes_only_history_files() {
        let d = tmp("hist");
        let la = d.join("Local");
        let ud = la.join("Google/Chrome/User Data");
        write(&ud.join("Default/History"), 70);
        write(&ud.join("Default/Bookmarks"), 50);
        let env = env_of(&[("LOCALAPPDATA", &la)]);
        let out = clean(find("chrome_history").unwrap(), &env, &FakeSys::default());
        assert_eq!(out.stats.files, 1);
        assert!(!ud.join("Default/History").exists());
        assert!(ud.join("Default/Bookmarks").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn single_files_by_pattern() {
        let d = tmp("thumb");
        let la = d.join("L");
        let ex = la.join("Microsoft/Windows/Explorer");
        write(&ex.join("thumbcache_256.db"), 10);
        write(&ex.join("thumbcache_idx.db"), 10);
        write(&ex.join("altro.txt"), 10);
        let env = env_of(&[("LOCALAPPDATA", &la)]);
        let out = clean(find("thumbs").unwrap(), &env, &FakeSys::default());
        assert_eq!(out.stats.files, 2);
        assert!(ex.join("altro.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn admin_targets_are_refused_without_admin() {
        let d = tmp("adm");
        let w = d.join("Windows");
        write(&w.join("Temp/x"), 10);
        let env = env_of(&[("SystemRoot", &w)]);
        let sys = FakeSys::default();
        let out = clean(find("temp_win").unwrap(), &env, &sys);
        assert!(out.error.is_some());
        assert!(w.join("Temp/x").exists());
        let sys = FakeSys {
            admin: true,
            ..FakeSys::default()
        };
        let out = clean(find("temp_win").unwrap(), &env, &sys);
        assert!(out.error.is_none());
        assert!(!w.join("Temp/x").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn running_services_are_stopped_and_restarted() {
        let d = tmp("svc");
        let w = d.join("Windows");
        write(&w.join("SoftwareDistribution/Download/u.cab"), 10);
        let env = env_of(&[("SystemRoot", &w)]);
        let sys = FakeSys {
            admin: true,
            ..FakeSys::default()
        };
        sys.responses
            .borrow_mut()
            .push(("sc query wuauserv".into(), "STATE : 4  RUNNING".into()));
        sys.responses
            .borrow_mut()
            .push(("sc query bits".into(), "STATE : 4  RUNNING".into()));
        clean(find("wu_cache").unwrap(), &env, &sys);
        let cmds: Vec<String> = sys
            .commands
            .borrow()
            .iter()
            .filter(|c| c.starts_with("net "))
            .cloned()
            .collect();
        assert_eq!(
            cmds,
            vec![
                "net stop wuauserv",
                "net stop bits",
                "net start wuauserv",
                "net start bits"
            ]
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn stopped_services_are_never_started() {
        let d = tmp("svc_off");
        let w = d.join("Windows");
        write(&w.join("SoftwareDistribution/Download/u.cab"), 10);
        let env = env_of(&[("SystemRoot", &w)]);
        let sys = FakeSys {
            admin: true,
            ..FakeSys::default()
        };
        sys.responses
            .borrow_mut()
            .push(("sc query".into(), "STATE : 1  STOPPED".into()));
        clean(find("wu_cache").unwrap(), &env, &sys);
        assert!(
            sys.commands.borrow().iter().all(|c| !c.starts_with("net ")),
            "Windows Update spento deve restare spento"
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn actions_run_commands_and_report_errors() {
        let env = TestEnv(HashMap::new());
        let sys = FakeSys::default();
        let out = clean(find("dns").unwrap(), &env, &sys);
        assert!(out.error.is_none());
        assert_eq!(sys.commands.borrow().as_slice(), ["ipconfig /flushdns"]);
        *sys.fail_run.borrow_mut() = true;
        let out = clean(find("dns").unwrap(), &env, &sys);
        assert!(out.error.is_some());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed() {
        let d = tmp("sym");
        let outside = d.join("fuori");
        write(&outside.join("importante.txt"), 99);
        let temp = d.join("Temp");
        fs::create_dir_all(&temp).unwrap();
        std::os::unix::fs::symlink(&outside, temp.join("link")).unwrap();
        let env = env_of(&[("TEMP", &temp)]);
        clean(find("temp_user").unwrap(), &env, &FakeSys::default());
        assert!(outside.join("importante.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn locked_or_missing_is_not_fatal() {
        let d = tmp("none");
        let env = env_of(&[("TEMP", &d.join("non-esiste"))]);
        let out = clean(find("temp_user").unwrap(), &env, &FakeSys::default());
        assert_eq!(out.stats, Stats::default());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn human_sizes() {
        assert_eq!(human(0), "0 B");
        assert_eq!(human(1536), "1,5 KB");
        assert_eq!(human(3 * 1024 * 1024 * 1024), "3,0 GB");
    }

    #[test]
    fn groups_all_have_targets() {
        for g in Group::ALL {
            assert!(crate::clean::targets::TARGETS.iter().any(|t| t.group == g));
        }
    }
}
