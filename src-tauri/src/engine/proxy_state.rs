use serde::{Deserialize, Serialize};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const PROXY_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
const AUTO_CONFIG_URL: &str = "AutoConfigURL";

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
    // Delete ProxyServer so we don't leave a stale string that would
    // activate on the next ProxyEnable flip by another tool.
    key.delete_value("ProxyServer").ok();
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

/// Delete the top-level `AutoConfigURL` registry value (belt-and-
/// suspenders cleanup; the per-connection API is the real mechanism
/// that browsers query).
///
/// We only need to *clear* AutoConfigURL — `clear_proxy` already takes
/// care of disabling the legacy ProxyEnable / ProxyServer pair, and
/// the per-connection API below is what actually flips browsers over
/// to (or off of) the PAC file. Writing the top-level `AutoConfigURL`
/// string is a belt-and-suspenders courtesy for tools that read the
/// registry directly (see the per-connection comment block below).
pub fn clear_pac_url() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let key = open_key()?;
    key.delete_value(AUTO_CONFIG_URL).ok();
    Ok(())
}

/// Apply the given PAC URL to the system proxy via the per-connection
/// binary blob (the real mechanism Edge/Chrome use), then notify
/// running browsers.
///
/// The top-level `AutoConfigURL` string value is the documentation
/// surface; the **actual** mechanism browsers (Edge, Chrome) use to
/// pick up the PAC is the per-connection binary blob at
/// `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings\
///  Connections\DefaultConnectionSettings`. Writing only the string
/// leaves browsers using the stale blob from before the focus session
/// started — symptom: bilibili.com still loads because the running
/// browser never re-reads the PAC.
///
/// The correct API is `InternetSetOptionW` with
/// `INTERNET_OPTION_PER_CONNECTION_OPTION` and an
/// `INTERNET_PER_CONN_OPTION_LISTW` containing one option whose
/// `dwOption = INTERNET_PER_CONN_AUTOCONFIG_URL`. `pszConnection = NULL`
/// targets the default (LAN) connection — which is exactly what
/// `Internet Options → Connections → LAN settings` writes to, and what
/// browsers query.
///
/// We still write the top-level string for compatibility with tools
/// that read it (the WinHTTP / `.NET GetSystemWebProxy` paths, plus
/// any direct registry inspectors). Belt and suspenders.
///
/// To clear: same API, but the option's value is set to an empty
/// string. `INTERNET_PER_CONN_FLAGS_UI` is *not* the right toggle —
/// that controls "show settings dialog on connect", not PAC presence.
#[cfg(windows)]
pub fn set_pac_url_per_connection(url: &str) {
    use windows::core::PWSTR;
    use windows::Win32::Networking::WinInet::{
        InternetSetOptionW, INTERNET_OPTION_PER_CONNECTION_OPTION,
        INTERNET_PER_CONN_AUTOCONFIG_URL, INTERNET_PER_CONN_FLAGS,
        INTERNET_PER_CONN_OPTIONW,
        INTERNET_PER_CONN_OPTION_LISTW,
    };
    unsafe {
        // Wide-string the URL. PWSTR points into this buffer for the
        // duration of the InternetSetOption call; we drop it after.
        let wide: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();

        // WinINET's per-connection blob tracks an access-type bitfield
        // separately from the URL strings. Just setting
        // INTERNET_PER_CONN_AUTOCONFIG_URL writes the string but does
        // NOT flip PROXY_TYPE_AUTO_PROXY_URL in the access-type flags
        // — and without that flag, the WinINET/IE resolver ignores
        // the URL and falls through to whatever proxy or DIRECT is
        // already in the blob. We have to set both in the same call.
        //
        // The flag values are the WinHTTP/WinINET standard set:
        //   PROXY_TYPE_DIRECT          = 0x01
        //   PROXY_TYPE_PROXY           = 0x02
        //   PROXY_TYPE_AUTO_PROXY_URL  = 0x04  ← needed
        //   PROXY_TYPE_AUTO_DETECT     = 0x08
        // We start from 0 (no flags) and set AUTO_PROXY_URL only.
        // DIRECT + AUTO_PROXY_URL is the documented combo for "use PAC,
        // fall back to direct if PAC fails".
        const PROXY_TYPE_AUTO_PROXY_URL: u32 = 0x04;

        let flags_option = INTERNET_PER_CONN_OPTIONW {
            dwOption: INTERNET_PER_CONN_FLAGS,
            Value: windows::Win32::Networking::WinInet::INTERNET_PER_CONN_OPTIONW_0 {
                dwValue: PROXY_TYPE_AUTO_PROXY_URL,
            },
        };
        let url_option = INTERNET_PER_CONN_OPTIONW {
            dwOption: INTERNET_PER_CONN_AUTOCONFIG_URL,
            Value: windows::Win32::Networking::WinInet::INTERNET_PER_CONN_OPTIONW_0 {
                pszValue: PWSTR(wide.as_ptr() as *mut _),
            },
        };
        let mut options = [flags_option, url_option];
        let mut list = INTERNET_PER_CONN_OPTION_LISTW {
            dwSize: std::mem::size_of::<INTERNET_PER_CONN_OPTION_LISTW>() as u32,
            pszConnection: PWSTR::null(),
            dwOptionCount: options.len() as u32,
            dwOptionError: 0,
            pOptions: options.as_mut_ptr(),
        };

        let result = InternetSetOptionW(
            None,
            INTERNET_OPTION_PER_CONNECTION_OPTION,
            Some(&mut list as *mut _ as *const _),
            std::mem::size_of::<INTERNET_PER_CONN_OPTION_LISTW>() as u32,
        );
        match result {
            Ok(()) => eprintln!("[proxy_state] set_pac_url_per_connection OK for {}", url),
            Err(e) => eprintln!("[proxy_state] set_pac_url_per_connection FAILED for {}: {:?}", url, e),
        }
        if list.dwOptionError != 0 {
            eprintln!("[proxy_state] per-connection option error code: {}", list.dwOptionError);
        }
    }
}

#[cfg(not(windows))]
pub fn set_pac_url_per_connection(_url: &str) {}

#[cfg(windows)]
pub fn clear_pac_url_per_connection() {
    set_pac_url_per_connection("");
}

/// Broadcast a proxy-changed notification so running browsers pick
/// up the new PAC / ProxyServer settings immediately.
///
/// WinINET-based clients (Chrome, Edge, the system's own HTTP stack)
/// cache proxy settings per-process. Writing to the registry is not
/// enough — without an explicit `InternetSetOptionW` broadcast, an
/// already-running browser will continue using whatever proxy state
/// it captured at launch until its cache TTL expires (often 30+
/// minutes) or the browser is restarted.
///
/// `INTERNET_OPTION_SETTINGS_CHANGED` is the documented broadcast: it
/// emits WM_SETTINGCHANGE so all interested processes re-read the
/// proxy registry. Clash, mitmproxy, and every Windows VPN client
/// uses this — leaving it out is the most common cause of "I set the
/// proxy but the browser still ignores it."
///
/// Per MS docs the call must be: hInternet=NULL, lpBuffer=NULL,
/// dwBufferLength=0. We ignore the BOOL return — failures here are
/// not actionable (the registry write already succeeded; worst case
/// the user has to wait for the cache TTL or restart the browser).
#[cfg(windows)]
pub fn notify_proxy_changed() {
    use windows::Win32::Networking::WinInet::{
        InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED,
    };
    unsafe {
        // Two broadcasts back-to-back. SETTINGS_CHANGED emits
        // WM_SETTINGCHANGE so WinINET clients re-read the registry;
        // REFRESH invalidates WinHTTP's per-process cache (Chromium,
        // msedge, WebView2, anything using winhttp.dll). Calling
        // neither is the most common cause of "I set the proxy but
        // the browser still ignores it."
        let _ = InternetSetOptionW(None, INTERNET_OPTION_SETTINGS_CHANGED, None, 0);
        let _ = InternetSetOptionW(None, INTERNET_OPTION_REFRESH, None, 0);
    }
}

#[cfg(not(windows))]
pub fn notify_proxy_changed() {
    // No-op on non-Windows targets.
}
