use std::ffi::c_void;
use std::path::PathBuf;

use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle,
    WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetTimeouts,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};

use snowhush_core::sha256;
use snowhush_core::version::{asset_field, field, listed_digest, newer};

pub const REPOSITORY: &str = "clavicular-alt/DesktopFlora";
/// The repository holds other projects' releases too; Snowhush's tags start with this.
const TAG_PREFIX: &str = "v";
const INSTALLER: &str = "DesktopFlora-Setup.exe";
const CHECKSUMS: &str = "SHA256SUMS.txt";
/// Far more than the installer ever weighs; anything bigger isn't it.
const MAX_DOWNLOAD: usize = 64 * 1024 * 1024;

/// A release newer than this build that carries an installer for Windows.
#[derive(Clone)]
pub struct Release {
    pub version: String,
    /// The release's page, for opening in the browser.
    pub page: String,
    /// The installer itself.
    pub url: String,
    /// The installer's SHA-256 as the release publishes it, if it does.
    pub digest: Option<String>,
}

/// Asks GitHub for the latest Snowhush release. Releases with nothing for Windows don't count as updates.
pub fn check() -> Result<Option<Release>, String> {
    let list = text(fetch("api.github.com", &format!("/repos/{REPOSITORY}/releases?per_page=50"), "application/vnd.github+json")?);
    // Releases come newest first; the first Snowhush one is the latest, up to where the next release begins.
    let Some(start) = list.find(&format!("\"tag_name\":\"{TAG_PREFIX}")).or_else(|| list.find(&format!("\"tag_name\": \"{TAG_PREFIX}"))) else {
        return Ok(None);
    };
    let body = &list[start..];
    let body = &body[..body[1..].find("\"tag_name\"").map_or(body.len(), |i| i + 1)];
    let tag = field(body, "tag_name").ok_or("the latest release has no tag")?;
    let version = tag.trim_start_matches(TAG_PREFIX);
    if !body.contains(&format!("\"{INSTALLER}\"")) || !newer(version, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    // GitHub records each upload's digest itself; a checksum file in the release is the fallback.
    let digest = asset_field(body, INSTALLER, "digest")
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .map(str::to_ascii_lowercase)
        .or_else(|| {
            let url = asset_field(body, CHECKSUMS, "browser_download_url")?;
            let (host, path) = split_url(url)?;
            listed_digest(&text(fetch(host, path, "application/octet-stream").ok()?), INSTALLER)
        });
    Ok(Some(Release {
        version: version.to_string(),
        page: format!("https://github.com/{REPOSITORY}/releases/tag/{tag}"),
        url: format!("https://github.com/{REPOSITORY}/releases/download/{tag}/{INSTALLER}"),
        digest,
    }))
}

/// Downloads the installer, checks it against the published digest and starts it to update quietly and reopen Snowhush.
/// Nothing runs unless the digest matches.
pub fn install(release: &Release) -> Result<(), String> {
    let expected = release.digest.as_deref().ok_or("the release doesn't publish a checksum for its installer")?;
    let (host, path) = split_url(&release.url).ok_or("the download address isn't valid")?;
    let installer = fetch(host, path, "application/octet-stream")?;
    let actual = sha256::hex(&installer);
    if actual != expected {
        return Err(format!("the download doesn't match its checksum (expected {expected}, got {actual}), so it wasn't run"));
    }
    let file: PathBuf = std::env::temp_dir().join(format!("DesktopFlora-Setup-{}.exe", release.version));
    std::fs::write(&file, &installer).map_err(|error| format!("couldn't save the update: {error}"))?;
    let file = HSTRING::from(file.as_os_str());
    unsafe {
        let result = ShellExecuteW(None, w!("open"), &file, w!("--silent --launch"), PCWSTR::null(), SW_SHOWNORMAL);
        // Anything above 32 means it started.
        if result.0 as isize <= 32 {
            return Err("couldn't start the installer".into());
        }
    }
    Ok(())
}

pub fn open(url: &str) {
    let url = HSTRING::from(url);
    unsafe {
        ShellExecuteW(None, w!("open"), &url, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

/// `https://host/path` into its host and path.
fn split_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("https://")?;
    let slash = rest.find('/')?;
    Some((&rest[..slash], &rest[slash..]))
}

fn text(body: Vec<u8>) -> String {
    String::from_utf8_lossy(&body).into_owned()
}

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

/// A GET over HTTPS, following redirects, as from a release download to where GitHub stores the file.
pub fn fetch(host: &str, path: &str, accept: &str) -> Result<Vec<u8>, String> {
    let failed = |what: &str| format!("{what} failed: {}", windows::core::Error::from_thread().message());
    let host = HSTRING::from(host);
    let path = HSTRING::from(path);
    unsafe {
        let session = Handle(WinHttpOpen(w!("DesktopFlora"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0));
        if session.0.is_null() {
            return Err(failed("Opening a connection"));
        }
        let _ = WinHttpSetTimeouts(session.0, 10_000, 10_000, 10_000, 30_000);
        let connection = Handle(WinHttpConnect(session.0, &host, 443, 0));
        if connection.0.is_null() {
            return Err(failed(&format!("Connecting to {host}")));
        }
        let request = Handle(WinHttpOpenRequest(connection.0, w!("GET"), &path, PCWSTR::null(), PCWSTR::null(), std::ptr::null(), WINHTTP_FLAG_SECURE));
        if request.0.is_null() {
            return Err(failed("Preparing the request"));
        }
        let headers: Vec<u16> = format!("Accept: {accept}\r\n").encode_utf16().collect();
        WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0).map_err(|error| error.message())?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|error| error.message())?;

        let mut status = 0u32;
        let mut size = 4u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&raw mut status).cast()),
            &mut size,
            std::ptr::null_mut(),
        )
        .map_err(|error| error.message())?;
        if status != 200 {
            return Err(format!("{host} answered {status}"));
        }

        let mut body = Vec::new();
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let mut read = 0u32;
            WinHttpReadData(request.0, buffer.as_mut_ptr().cast(), buffer.len() as u32, &mut read).map_err(|error| error.message())?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&buffer[..read as usize]);
            if body.len() > MAX_DOWNLOAD {
                return Err(format!("{host} sent more than expected"));
            }
        }
        Ok(body)
    }
}
