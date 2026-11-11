//! Accesso alla rete dietro un trait, così i test non escono su Internet.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

pub trait Http: Send + Sync {
    fn get_text(&self, url: &str) -> Result<String, String>;
    /// Scarica in un file; restituisce i byte scritti.
    fn download(&self, url: &str, dest: &Path) -> Result<u64, String>;
}

/// Limite di sicurezza per un singolo download.
pub const MAX_DOWNLOAD: u64 = 4 * 1024 * 1024 * 1024;

pub struct RealHttp {
    agent: ureq::Agent,
}

impl RealHttp {
    pub fn new() -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(3600)))
            .timeout_connect(Some(Duration::from_secs(20)))
            .user_agent("LiberDesk")
            .build()
            .into();
        Self { agent }
    }
}

impl Default for RealHttp {
    fn default() -> Self {
        Self::new()
    }
}

impl Http for RealHttp {
    fn get_text(&self, url: &str) -> Result<String, String> {
        let mut resp = self
            .agent
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(|e| format!("{url}: {e}"))?;
        resp.body_mut()
            .with_config()
            .limit(8 * 1024 * 1024)
            .read_to_string()
            .map_err(|e| format!("{url}: {e}"))
    }

    fn download(&self, url: &str, dest: &Path) -> Result<u64, String> {
        let mut resp = self
            .agent
            .get(url)
            .call()
            .map_err(|e| format!("{url}: {e}"))?;
        let mut reader = resp.body_mut().with_config().limit(MAX_DOWNLOAD).reader();
        let mut out = File::create(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut total = 0u64;
        loop {
            let n = reader.read(&mut buf).map_err(|e| format!("{url}: {e}"))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])
                .map_err(|e| format!("{}: {e}", dest.display()))?;
            total += n as u64;
        }
        Ok(total)
    }
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use std::sync::Mutex;

    type Texts = Vec<(String, Result<String, String>)>;

    #[derive(Default)]
    pub struct FakeHttp {
        pub texts: Mutex<Texts>,
        pub files: Mutex<Vec<(String, Vec<u8>)>>,
        pub requested: Mutex<Vec<String>>,
    }

    impl Http for FakeHttp {
        fn get_text(&self, url: &str) -> Result<String, String> {
            self.requested.lock().unwrap().push(url.to_string());
            self.texts
                .lock()
                .unwrap()
                .iter()
                .find(|(u, _)| url.contains(u.as_str()))
                .map(|(_, r)| r.clone())
                .unwrap_or_else(|| Err("404".into()))
        }
        fn download(&self, url: &str, dest: &Path) -> Result<u64, String> {
            self.requested.lock().unwrap().push(url.to_string());
            let files = self.files.lock().unwrap();
            let (_, data) = files
                .iter()
                .find(|(u, _)| url.contains(u.as_str()))
                .ok_or_else(|| "404".to_string())?;
            std::fs::write(dest, data).map_err(|e| e.to_string())?;
            Ok(data.len() as u64)
        }
    }
}
