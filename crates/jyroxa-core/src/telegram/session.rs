//! Encrypted Telegram session persistence (DPAPI on Windows).

use std::collections::HashMap;

use grammers_session::storages::MemorySession;
use grammers_session::{Session, SessionData};
use grammers_session::types::DcOption;
use serde::{Deserialize, Serialize};

use super::error::{Result, TelegramError};

#[cfg(windows)]
use windows::Win32::Foundation::LocalFree;
#[cfg(windows)]
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
};

/// On-disk envelope (encrypted at rest on Windows).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedTelegramSession {
    pub api_id: i32,
    pub api_hash: String,
    pub phone: String,
    pub display_name: Option<String>,
    pub home_dc: i32,
    pub dc_options: Vec<DcOption>,
}

/// Load and store a grammers [`MemorySession`] in the app data directory.
pub struct SessionStore {
    path: std::path::PathBuf,
}

impl SessionStore {
    pub fn new(data_dir: &std::path::Path) -> Self {
        SessionStore {
            path: data_dir.join("telegram.session.enc"),
        }
    }

    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    pub fn load(&self) -> Result<PersistedTelegramSession> {
        let raw = std::fs::read(&self.path)?;
        let plain = decrypt_blob(&raw)?;
        serde_json::from_slice(&plain).map_err(|e| TelegramError::other(format!("session json: {e}")))
    }

    pub fn save(&self, blob: &PersistedTelegramSession) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let plain = serde_json::to_vec(blob)?;
        let enc = encrypt_blob(&plain)?;
        std::fs::write(&self.path, enc)?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        if self.path.is_file() {
            std::fs::remove_file(&self.path)?;
        }
        Ok(())
    }
}

pub async fn export_memory_session(session: &MemorySession) -> Result<PersistedTelegramSession> {
    let home_dc = session
        .home_dc_id()
        .map_err(|e| TelegramError::other(format!("home_dc: {e}")))?;
    let mut dc_options = Vec::new();
    for id in 1..=10 {
        if let Some(dc) = session
            .dc_option(id)
            .map_err(|e| TelegramError::other(format!("dc_option: {e}")))?
        {
            if dc.auth_key.is_some() {
                dc_options.push(dc);
            }
        }
    }
    Ok(PersistedTelegramSession {
        api_id: 0,
        api_hash: String::new(),
        phone: String::new(),
        display_name: None,
        home_dc,
        dc_options,
    })
}

pub fn memory_from_persisted(persisted: &PersistedTelegramSession) -> MemorySession {
    let mut dc_map: HashMap<i32, DcOption> = SessionData::default()
        .dc_options
        .into_iter()
        .collect();
    for opt in &persisted.dc_options {
        dc_map.insert(opt.id, opt.clone());
    }
    MemorySession::from(SessionData {
        home_dc: persisted.home_dc,
        dc_options: dc_map,
        peer_infos: HashMap::new(),
        updates_state: Default::default(),
    })
}

fn encrypt_blob(data: &[u8]) -> Result<Vec<u8>> {
    #[cfg(windows)]
    {
        dpapi_protect(data)
    }
    #[cfg(not(windows))]
    {
        Ok(data.to_vec())
    }
}

fn decrypt_blob(data: &[u8]) -> Result<Vec<u8>> {
    #[cfg(windows)]
    {
        dpapi_unprotect(data)
    }
    #[cfg(not(windows))]
    {
        Ok(data.to_vec())
    }
}

#[cfg(windows)]
fn dpapi_protect(data: &[u8]) -> Result<Vec<u8>> {
    let mut in_blob = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &mut in_blob,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
        .map_err(|e| TelegramError::other(format!("CryptProtectData failed: {e}")))?;
        let slice = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize);
        let out = slice.to_vec();
        let _ = LocalFree(windows::Win32::Foundation::HLOCAL(out_blob.pbData as _));
        Ok(out)
    }
}

#[cfg(windows)]
fn dpapi_unprotect(data: &[u8]) -> Result<Vec<u8>> {
    let mut in_blob = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &mut in_blob,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
        .map_err(|e| TelegramError::other(format!("CryptUnprotectData failed: {e}")))?;
        let slice = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize);
        let out = slice.to_vec();
        let _ = LocalFree(windows::Win32::Foundation::HLOCAL(out_blob.pbData as _));
        Ok(out)
    }
}
