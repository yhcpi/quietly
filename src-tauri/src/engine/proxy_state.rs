use serde::{Deserialize, Serialize};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const PROXY_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxySnapshot {
    pub enable: bool,
    pub server: Option<String>,
    pub override_str: Option<String>,
}

fn open_key() -> Result<RegKey, Box<dyn std::error::Error + Send + Sync>> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey_with_flags(PROXY_KEY_PATH, KEY_READ | KEY_WRITE)?;
    Ok(key)
}

pub fn read_current() -> Result<ProxySnapshot, Box<dyn std::error::Error + Send + Sync>> {
    let key = open_key()?;
    let enable: u32 = key.get_value("ProxyEnable").unwrap_or(0);
    let server: String = key.get_value("ProxyServer").unwrap_or_default();
    let override_str: String = key.get_value("ProxyOverride").unwrap_or_default();
    Ok(ProxySnapshot {
        enable: enable != 0,
        server: if server.is_empty() { None } else { Some(server) },
        override_str: if override_str.is_empty() { None } else { Some(override_str) },
    })
}

pub fn set_proxy(server: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let key = open_key()?;
    key.set_value("ProxyEnable", &1u32)?;
    key.set_value("ProxyServer", &server.to_string())?;
    Ok(())
}

pub fn clear_proxy() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let key = open_key()?;
    key.set_value("ProxyEnable", &0u32)?;
    Ok(())
}

pub fn restore(snap: &ProxySnapshot) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let key = open_key()?;
    key.set_value("ProxyEnable", &(snap.enable as u32))?;
    match &snap.server {
        Some(s) => key.set_value("ProxyServer", s)?,
        None => { key.delete_value("ProxyServer").ok(); }
    }
    match &snap.override_str {
        Some(s) => key.set_value("ProxyOverride", s)?,
        None => { key.delete_value("ProxyOverride").ok(); }
    }
    Ok(())
}
