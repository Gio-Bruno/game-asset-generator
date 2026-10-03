//! Explicit application updates, separate from the game workspace and Codex login.
//! Network/download work runs on a background thread. A copied CLI helper installs
//! only after the owning application has exited and released its workspace lease.
use crate::error::{ApiError, Result};
use reqwest::{Url, blocking::Client, redirect::Policy};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

const REPOSITORY: &str = "Gio-Bruno/game-asset-generator";
const RELEASE_API: &str =
    "https://api.github.com/repos/Gio-Bruno/game-asset-generator/releases/latest";
const MAX_DOWNLOAD: u64 = 512 * 1024 * 1024;
const MAX_EXPANDED: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Platform {
    MacosArm64,
    WindowsX64,
}
impl Platform {
    fn current() -> Result<Self> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => Ok(Self::MacosArm64),
            ("windows", "x86_64") => Ok(Self::WindowsX64),
            _ => Err(failure(
                "Automatic updates are available for Apple Silicon Mac and Windows x64.",
            )),
        }
    }
    fn asset_name(self) -> &'static str {
        match self {
            Self::MacosArm64 => "Asset-Forge-macOS.zip",
            Self::WindowsX64 => "Asset-Forge-Windows-x64-Setup.exe",
        }
    }
    fn payload_name(self) -> &'static str {
        match self {
            Self::MacosArm64 => "update.zip",
            Self::WindowsX64 => "Setup.exe",
        }
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<ReleaseAsset>,
}
#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}
#[derive(Debug)]
struct Offer {
    version: String,
    url: String,
    size: u64,
    sha256: String,
}

/// A verified download ready for an explicit restart. Its install paths are private.
#[derive(Debug)]
pub struct PreparedUpdate {
    pub version: String,
    plan_path: PathBuf,
}
pub enum CheckResult {
    Current { installed: String, latest: String },
    Ready(PreparedUpdate),
}
pub enum Progress {
    Downloading(u8),
    Verifying,
    Finished(Result<CheckResult>),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InstallPlan {
    schema_version: u32,
    version: String,
    platform: Platform,
    stage: PathBuf,
    destination: PathBuf,
    parent_pid: u32,
    size: u64,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema_version: u32,
    version: String,
    platform: String,
    sha256: std::collections::HashMap<String, String>,
}

fn failure(message: impl Into<String>) -> ApiError {
    ApiError::new("UPDATE_FAILED", message)
}
fn io_failure(error: impl std::fmt::Display) -> ApiError {
    failure(format!("Could not prepare the update: {error}"))
}
fn version(value: &str) -> Result<Version> {
    let parsed =
        Version::parse(value).map_err(|_| failure("The release has an invalid version."))?;
    if !parsed.pre.is_empty() || !parsed.build.is_empty() {
        return Err(failure("Only stable releases can be installed."));
    }
    Ok(parsed)
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}
fn require_newer(next: &str, installed: &str) -> Result<()> {
    if version(next)? <= version(installed)? {
        return Err(failure(
            "This prepared update is no longer newer than the installed app. Check for updates again.",
        ));
    }
    Ok(())
}
fn select_offer(
    release: Release,
    installed: &str,
    platform: Platform,
) -> Result<(String, Option<Offer>)> {
    if release.draft || release.prerelease {
        return Err(failure(
            "The latest release is not a published stable version.",
        ));
    }
    let latest = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    if version(latest)? <= version(installed)? {
        return Ok((latest.into(), None));
    }
    let expected_name = platform.asset_name();
    let mut matches = release
        .assets
        .into_iter()
        .filter(|a| a.name == expected_name);
    let asset = matches
        .next()
        .ok_or_else(|| failure("The latest release has no installer for this computer yet."))?;
    if matches.next().is_some() {
        return Err(failure("The release contains duplicate update files."));
    }
    let expected_url = format!(
        "https://github.com/{REPOSITORY}/releases/download/{}/{expected_name}",
        release.tag_name
    );
    if asset.browser_download_url != expected_url || asset.size == 0 || asset.size > MAX_DOWNLOAD {
        return Err(failure("The release contains an invalid update file."));
    }
    let hash = asset
        .digest
        .as_deref()
        .and_then(|s| s.strip_prefix("sha256:"))
        .filter(|s| valid_hash(s))
        .ok_or_else(|| failure("This release has no SHA-256 verification information."))?;
    Ok((
        latest.into(),
        Some(Offer {
            version: latest.into(),
            url: expected_url,
            size: asset.size,
            sha256: hash.to_ascii_lowercase(),
        }),
    ))
}
fn trusted_host(url: &Url) -> bool {
    url.scheme() == "https"
        && matches!(
            url.host_str(),
            Some(
                "api.github.com"
                    | "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
                    | "github-releases.githubusercontent.com"
            )
        )
}
fn client() -> Result<Client> {
    Client::builder()
        .user_agent(concat!("Asset-Forge/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(900))
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || !trusted_host(attempt.url()) {
                attempt.error("Unexpected update redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|_| failure("Could not initialize the secure update connection."))
}
fn read_bounded(mut reader: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(io_failure)?;
    if bytes.len() as u64 > limit {
        return Err(failure("The update response is too large."));
    }
    Ok(bytes)
}
fn cache_root() -> Result<PathBuf> {
    dirs::cache_dir()
        .map(|p| p.join("AssetForge").join("updates"))
        .ok_or_else(|| failure("Could not locate the application update cache."))
}
fn install_destination(executable: &Path, platform: Platform) -> Result<PathBuf> {
    let executable = executable.canonicalize().map_err(io_failure)?;
    let destination = match platform {
        Platform::MacosArm64 => {
            let macos = executable
                .parent()
                .ok_or_else(|| failure("Could not locate the application bundle."))?;
            let contents = macos
                .parent()
                .ok_or_else(|| failure("Could not locate the application bundle."))?;
            let bundle = contents
                .parent()
                .ok_or_else(|| failure("Could not locate the application bundle."))?;
            if macos.file_name().and_then(|v| v.to_str()) != Some("MacOS")
                || contents.file_name().and_then(|v| v.to_str()) != Some("Contents")
                || bundle.extension().and_then(|v| v.to_str()) != Some("app")
                || bundle
                    .components()
                    .any(|v| v.as_os_str() == "AppTranslocation")
                || bundle.starts_with("/Volumes")
            {
                return Err(failure(
                    "Move Asset Forge out of the disk image into Applications, then try Update again.",
                ));
            }
            bundle.to_path_buf()
        }
        Platform::WindowsX64 => {
            let folder = executable
                .parent()
                .ok_or_else(|| failure("Could not locate the installed application."))?;
            if !folder.join("release-manifest.json").is_file() {
                return Err(failure(
                    "Use the packaged Asset Forge app to install updates.",
                ));
            }
            folder.to_path_buf()
        }
    };
    // Prove the replacement directory is writable before offering a restart.
    let parent = if platform == Platform::MacosArm64 {
        destination.parent().unwrap()
    } else {
        &destination
    };
    let probe = parent.join(format!(".asset-forge-update-{}", uuid::Uuid::new_v4()));
    File::create(&probe).map_err(|_| failure("The application folder is read-only. Move Asset Forge to a folder you own before updating."))?;
    fs::remove_file(probe).map_err(io_failure)?;
    Ok(destination)
}
fn stream_verified(
    mut input: impl Read,
    mut output: impl Write,
    size: u64,
    expected: &str,
    mut progress: impl FnMut(u8),
) -> Result<()> {
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut previous = u8::MAX;
    let mut buffer = [0u8; 65536];
    loop {
        let count = input.read(&mut buffer).map_err(io_failure)?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        if bytes > size || bytes > MAX_DOWNLOAD {
            return Err(failure("The downloaded update is larger than expected."));
        }
        output.write_all(&buffer[..count]).map_err(io_failure)?;
        hash.update(&buffer[..count]);
        let percent = ((bytes * 100) / size) as u8;
        if percent != previous {
            progress(percent);
            previous = percent;
        }
    }
    if bytes != size || format!("{:x}", hash.finalize()) != expected {
        return Err(failure(
            "Update verification failed. The installed app was not changed. Try Update again.",
        ));
    }
    output.flush().map_err(io_failure)
}
fn file_hash(path: &Path) -> Result<String> {
    let mut reader = File::open(path).map_err(io_failure)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = reader.read(&mut buffer).map_err(io_failure)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Call on a background OS thread, not inside an async runtime or the UI thread.
pub fn prepare_latest(
    executable: &Path,
    mut progress: impl FnMut(Progress),
) -> Result<CheckResult> {
    let platform = Platform::current()?;
    let client = client()?;
    let response = client
        .get(RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .timeout(Duration::from_secs(30))
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|_| {
            failure("Could not check for updates. Check your connection and try again.")
        })?;
    let release: Release = serde_json::from_slice(&read_bounded(response, 2 * 1024 * 1024)?)
        .map_err(|_| failure("GitHub returned invalid release information."))?;
    let installed = env!("CARGO_PKG_VERSION");
    let (latest, offer) = select_offer(release, installed, platform)?;
    let Some(offer) = offer else {
        return Ok(CheckResult::Current {
            installed: installed.into(),
            latest,
        });
    };
    let destination = install_destination(executable, platform)?;
    let root = cache_root()?;
    fs::create_dir_all(&root).map_err(io_failure)?;
    let stage = root.join(uuid::Uuid::new_v4().to_string());
    fs::create_dir(&stage).map_err(io_failure)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&stage, fs::Permissions::from_mode(0o700)).map_err(io_failure)?;
    }
    let result = (|| {
        let response = client
            .get(&offer.url)
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|_| {
                failure("Could not download the update. The installed app was not changed.")
            })?;
        if response
            .content_length()
            .is_some_and(|size| size != offer.size)
        {
            return Err(failure("The release file size has changed."));
        }
        let payload = stage.join(platform.payload_name());
        let output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&payload)
            .map_err(io_failure)?;
        stream_verified(response, output, offer.size, &offer.sha256, |p| {
            progress(Progress::Downloading(p))
        })?;
        progress(Progress::Verifying);
        if platform == Platform::MacosArm64 {
            extract_mac_zip(&payload, &stage.join("payload"))?;
            verify_mac_payload(&stage.join("payload"), &offer.version)?;
        }
        let plan = InstallPlan {
            schema_version: 1,
            version: offer.version.clone(),
            platform,
            stage: stage.clone(),
            destination,
            parent_pid: std::process::id(),
            size: offer.size,
            sha256: offer.sha256,
        };
        let plan_path = stage.join("install-plan.json");
        fs::write(&plan_path, serde_json::to_vec(&plan).map_err(io_failure)?)
            .map_err(io_failure)?;
        Ok(CheckResult::Ready(PreparedUpdate {
            version: offer.version,
            plan_path,
        }))
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}

fn extract_mac_zip(source: &Path, destination: &Path) -> Result<()> {
    let mut archive =
        zip::ZipArchive::new(File::open(source).map_err(io_failure)?).map_err(io_failure)?;
    if archive.len() > 10000 {
        return Err(failure("The update archive contains too many files."));
    }
    fs::create_dir_all(destination).map_err(io_failure)?;
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(io_failure)?;
        let name = entry
            .enclosed_name()
            .ok_or_else(|| failure("The update archive contains an unsafe path."))?
            .to_path_buf();
        let mode = entry.unix_mode().unwrap_or(0o644);
        if mode & 0o170000 == 0o120000 {
            return Err(failure("The update archive contains a symbolic link."));
        }
        if name.starts_with("__MACOSX") {
            continue;
        }
        if name != Path::new("release-manifest.json") && !name.starts_with("Asset Forge.app") {
            return Err(failure("The update archive contains an unexpected file."));
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| failure("The update archive is too large."))?;
        if total > MAX_EXPANDED {
            return Err(failure("The update archive is too large."));
        }
        let target = destination.join(name);
        if entry.is_dir() {
            fs::create_dir_all(target).map_err(io_failure)?;
            continue;
        }
        fs::create_dir_all(target.parent().unwrap()).map_err(io_failure)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(io_failure)?;
        std::io::copy(&mut entry, &mut file).map_err(io_failure)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&target, fs::Permissions::from_mode(mode & 0o777))
                .map_err(io_failure)?;
        }
    }
    Ok(())
}
fn verify_mac_payload(payload: &Path, expected_version: &str) -> Result<()> {
    let manifest: Manifest = serde_json::from_slice(&read_bounded(
        File::open(payload.join("release-manifest.json")).map_err(io_failure)?,
        65536,
    )?)
    .map_err(|_| failure("The update has an invalid package manifest."))?;
    if manifest.schema_version != 1
        || manifest.platform != "macos-arm64"
        || manifest.version != expected_version
    {
        return Err(failure(
            "The downloaded package does not match the requested Mac version.",
        ));
    }
    let app = payload.join("Asset Forge.app");
    for name in ["asset-forge", "asset-forge-studio"] {
        if manifest.sha256.get(name) != Some(&file_hash(&app.join("Contents/MacOS").join(name))?) {
            return Err(failure(
                "The update executable does not match its package manifest.",
            ));
        }
    }
    #[cfg(target_os = "macos")]
    {
        for (key, expected) in [
            ("CFBundleIdentifier", "dev.assetforge.studio"),
            ("CFBundleShortVersionString", expected_version),
        ] {
            let output = Command::new("/usr/bin/plutil")
                .args(["-extract", key, "raw", "-o", "-"])
                .arg(app.join("Contents/Info.plist"))
                .output()
                .map_err(io_failure)?;
            if !output.status.success()
                || String::from_utf8_lossy(&output.stdout).trim() != expected
            {
                return Err(failure(
                    "The downloaded application has incorrect bundle metadata.",
                ));
            }
        }
        let status = Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&app)
            .status()
            .map_err(io_failure)?;
        if !status.success() {
            return Err(failure(
                "The downloaded application's code signature is invalid.",
            ));
        }
    }
    Ok(())
}

/// Copy the already-installed CLI out of the replacement directory before quitting.
pub fn launch_installer(update: &PreparedUpdate) -> Result<()> {
    let plan = read_plan(&update.plan_path)?;
    require_newer(&plan.version, env!("CARGO_PKG_VERSION"))?;
    let cli = match plan.platform {
        Platform::MacosArm64 => plan.destination.join("Contents/MacOS/asset-forge"),
        Platform::WindowsX64 => plan.destination.join("asset-forge.exe"),
    };
    let helper = plan.stage.join(if cfg!(windows) {
        "update-helper.exe"
    } else {
        "update-helper"
    });
    fs::copy(cli, &helper).map_err(io_failure)?;
    let log = File::create(plan.stage.join("install.log")).map_err(io_failure)?;
    let mut command = Command::new(helper);
    command
        .args(["apply-update", "--plan"])
        .arg(&update.plan_path)
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(io_failure)?)
        .stderr(log);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    command.spawn().map_err(|_| {
        failure("Could not start the update installer. The current app is still running.")
    })?;
    Ok(())
}
fn read_plan(path: &Path) -> Result<InstallPlan> {
    let plan: InstallPlan =
        serde_json::from_slice(&read_bounded(File::open(path).map_err(io_failure)?, 65536)?)
            .map_err(|_| failure("Invalid application update plan."))?;
    let root = cache_root()?.canonicalize().map_err(io_failure)?;
    let stage = plan.stage.canonicalize().map_err(io_failure)?;
    if plan.schema_version != 1
        || plan.platform != Platform::current()?
        || stage.parent() != Some(root.as_path())
        || uuid::Uuid::parse_str(stage.file_name().and_then(|s| s.to_str()).unwrap_or("")).is_err()
        || path.canonicalize().map_err(io_failure)? != stage.join("install-plan.json")
        || plan.parent_pid == 0
        || plan.parent_pid > i32::MAX as u32
        || plan.size == 0
        || plan.size > MAX_DOWNLOAD
        || !valid_hash(&plan.sha256)
        || !plan.destination.is_absolute()
        || !plan.destination.is_dir()
    {
        return Err(failure("Invalid application update plan."));
    }
    version(&plan.version)?;
    let payload = stage.join(plan.platform.payload_name());
    if fs::metadata(&payload).map_err(io_failure)?.len() != plan.size
        || file_hash(&payload)? != plan.sha256
    {
        return Err(failure("The staged update changed. Download it again."));
    }
    Ok(plan)
}

#[cfg(unix)]
fn wait_for_parent(pid: u32) -> Result<()> {
    for _ in 0..120 {
        // Probe only; never send a signal. EPERM also means the process is alive.
        if unsafe { libc::kill(pid as i32, 0) } == -1
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err(failure(
        "Asset Forge did not close. The installed app was not changed.",
    ))
}
#[cfg(windows)]
fn wait_for_parent(pid: u32) -> Result<()> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, GetLastError, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return if unsafe { GetLastError() } == ERROR_INVALID_PARAMETER {
            Ok(())
        } else {
            Err(failure(
                "Could not wait for Asset Forge to close. The installed app was not changed.",
            ))
        };
    }
    let waited = unsafe { WaitForSingleObject(handle, 60000) };
    unsafe {
        CloseHandle(handle);
    }
    if waited != WAIT_OBJECT_0 {
        return Err(failure(
            "Asset Forge did not close. The installed app was not changed.",
        ));
    }
    Ok(())
}

#[cfg(any(target_os = "macos", test))]
fn replace_with_backup(incoming: &Path, destination: &Path, backup: &Path) -> Result<()> {
    if backup.exists() {
        return Err(failure("An update backup already exists."));
    }
    fs::rename(destination, backup).map_err(io_failure)?;
    if let Err(error) = fs::rename(incoming, destination) {
        fs::rename(backup, destination).map_err(|restore| {
            failure(format!(
                "Could not restore the previous app: {restore}. Its backup is at {}.",
                backup.display()
            ))
        })?;
        return Err(io_failure(error));
    }
    Ok(())
}
#[cfg(target_os = "macos")]
fn install_mac(plan: &InstallPlan) -> Result<()> {
    let payload = plan.stage.join("payload");
    verify_mac_payload(&payload, &plan.version)?;
    if plan.destination.extension().and_then(|s| s.to_str()) != Some("app") {
        return Err(failure("Invalid Mac application destination."));
    }
    let suffix = plan.stage.file_name().unwrap().to_string_lossy();
    let parent = plan.destination.parent().unwrap();
    let incoming = parent.join(format!(".AssetForge-incoming-{suffix}.app"));
    let backup = parent.join(format!(".AssetForge-previous-{suffix}.app"));
    if incoming.exists() {
        return Err(failure(
            "An update is already staged in the application folder.",
        ));
    }
    let copied = Command::new("/usr/bin/ditto")
        .arg(payload.join("Asset Forge.app"))
        .arg(&incoming)
        .status()
        .map_err(io_failure)?;
    if !copied.success() {
        let _ = fs::remove_dir_all(&incoming);
        return Err(failure(
            "Could not copy the new application. The installed app was not changed.",
        ));
    }
    // Keep normal OS download protection. Do not remove quarantine or bypass Gatekeeper.
    let quarantine = Command::new("/usr/bin/xattr")
        .args(["-w", "com.apple.quarantine", "0081;00000000;Asset Forge;"])
        .arg(&incoming)
        .status()
        .map_err(io_failure)?;
    if !quarantine.success() {
        let _ = fs::remove_dir_all(&incoming);
        return Err(failure("Could not preserve macOS download protection."));
    }
    let result = replace_with_backup(&incoming, &plan.destination, &backup);
    if result.is_err() {
        let _ = fs::remove_dir_all(&incoming);
    }
    result
}
#[cfg(windows)]
fn install_windows(plan: &InstallPlan) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::{
            Com::{
                COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
            },
            Threading::{GetExitCodeProcess, WaitForSingleObject},
        },
        UI::{
            Shell::{
                SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
            },
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    };
    let installer = plan.stage.join("Setup.exe");
    if unsafe {
        CoInitializeEx(
            std::ptr::null(),
            (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
        )
    } < 0
    {
        return Err(failure(
            "Could not initialize the Windows installer launcher.",
        ));
    }
    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    let _com = ComGuard;
    // Mark the inbound executable as downloaded; ShellExecute preserves Windows approval checks.
    fs::write(
        format!("{}:Zone.Identifier", installer.display()),
        "[ZoneTransfer]\r\nZoneId=3\r\n",
    )
    .map_err(io_failure)?;
    let file: Vec<u16> = installer.as_os_str().encode_wide().chain(Some(0)).collect();
    let parameters: Vec<u16> =
        std::ffi::OsStr::new(&format!("/S /D={}", plan.destination.display()))
            .encode_wide()
            .chain(Some(0))
            .collect();
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
    info.lpFile = file.as_ptr();
    info.lpParameters = parameters.as_ptr();
    info.nShow = SW_SHOWNORMAL;
    if unsafe { ShellExecuteExW(&mut info) } == 0 || info.hProcess.is_null() {
        return Err(failure("Windows did not start the update installer."));
    }
    let waited = unsafe { WaitForSingleObject(info.hProcess, 600000) };
    let mut exit_code = 1;
    unsafe {
        GetExitCodeProcess(info.hProcess, &mut exit_code);
        CloseHandle(info.hProcess);
    }
    if waited != WAIT_OBJECT_0 || exit_code != 0 {
        return Err(failure(
            "The Windows update installer did not complete successfully.",
        ));
    }
    let output = Command::new(plan.destination.join("asset-forge.exe"))
        .arg("--version")
        .output()
        .map_err(io_failure)?;
    if !output.status.success()
        || String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .last()
            != Some(plan.version.as_str())
    {
        return Err(failure(
            "The Windows installer did not install the expected version.",
        ));
    }
    Ok(())
}
fn relaunch(plan: &InstallPlan) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("/usr/bin/open")
            .arg(&plan.destination)
            .status()
            .map_err(io_failure)?;
        if !status.success() {
            return Err(failure(
                "The update is installed. Open Asset Forge and complete any macOS security approval.",
            ));
        }
    }
    #[cfg(windows)]
    {
        Command::new(plan.destination.join("asset-forge-studio.exe"))
            .spawn()
            .map_err(io_failure)?;
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = plan;
        return Err(failure("This platform cannot install application updates."));
    }
    #[allow(unreachable_code)]
    Ok(())
}
/// Hidden CLI helper. Never opens or modifies the game workspace.
pub fn apply_update(path: &Path, should_relaunch: bool) -> Result<()> {
    let plan = read_plan(path)?;
    require_newer(&plan.version, env!("CARGO_PKG_VERSION"))?;
    if plan.parent_pid == std::process::id() {
        return Err(failure(
            "The update helper must run separately from the application.",
        ));
    }
    // A second invocation must not repeat an installation whose result is unknown.
    OpenOptions::new().write(true).create_new(true).open(plan.stage.join("installation-started"))
        .map_err(|_| failure("This update installation has already started. Reopen Asset Forge to check its version."))?;
    let result: Result<()> = (|| {
        wait_for_parent(plan.parent_pid)?;
        #[cfg(target_os = "macos")]
        install_mac(&plan)?;
        #[cfg(windows)]
        install_windows(&plan)?;
        #[cfg(not(any(target_os = "macos", windows)))]
        return Err(failure("This platform cannot install application updates."));
        #[allow(unreachable_code)]
        Ok(())
    })();
    let message = match &result {
        Ok(()) => format!("Updated to Asset Forge {}.", plan.version),
        Err(error) => error.message.clone(),
    };
    if let Ok(root) = cache_root() {
        let _ = fs::write(
            root.join("last-result.json"),
            serde_json::json!({"message":message,"isError":result.is_err()}).to_string(),
        );
    }
    if should_relaunch {
        relaunch(&plan)?;
    }
    result
}
/// Show the helper's success/failure once after restart.
pub fn take_install_result() -> Option<(String, bool)> {
    let path = cache_root().ok()?.join("last-result.json");
    let value: serde_json::Value =
        serde_json::from_slice(&read_bounded(File::open(&path).ok()?, 65536).ok()?).ok()?;
    let message = value["message"].as_str()?.to_string();
    let error = value["isError"].as_bool()?;
    let _ = fs::remove_file(path);
    Some((message, error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    fn release(v: &str, platform: Platform, bytes: &[u8]) -> Release {
        Release {
            tag_name: format!("v{v}"),
            draft: false,
            prerelease: false,
            assets: vec![ReleaseAsset {
                name: platform.asset_name().into(),
                browser_download_url: format!(
                    "https://github.com/{REPOSITORY}/releases/download/v{v}/{}",
                    platform.asset_name()
                ),
                size: bytes.len() as u64,
                digest: Some(format!("sha256:{:x}", Sha256::digest(bytes))),
            }],
        }
    }
    #[test]
    fn only_newer_stable_matching_verified_releases_are_offered() {
        let p = Platform::MacosArm64;
        assert!(require_newer("0.1.1", "0.1.2").is_err());
        assert!(require_newer("0.1.2", "0.1.2").is_err());
        assert!(require_newer("0.1.10", "0.1.2").is_ok());
        assert!(
            select_offer(release("0.1.1", p, b"zip"), "0.1.2", p)
                .unwrap()
                .1
                .is_none()
        );
        assert!(
            select_offer(release("0.1.2", p, b"zip"), "0.1.2", p)
                .unwrap()
                .1
                .is_none()
        );
        assert_eq!(
            select_offer(release("0.1.10", p, b"zip"), "0.1.2", p)
                .unwrap()
                .1
                .unwrap()
                .version,
            "0.1.10"
        );
        let mut r = release("0.2.0", p, b"zip");
        r.prerelease = true;
        assert!(select_offer(r, "0.1.2", p).is_err());
        let mut r = release("0.2.0", p, b"zip");
        r.assets[0].browser_download_url = "https://example.com/evil.zip".into();
        assert!(select_offer(r, "0.1.2", p).is_err());
        let mut r = release("0.2.0", p, b"zip");
        r.assets[0].digest = None;
        assert!(select_offer(r, "0.1.2", p).is_err());
        assert!(select_offer(release("0.2.0", p, b"zip"), "0.1.2", Platform::WindowsX64).is_err());
        assert!(!trusted_host(
            &Url::parse("https://github.com.evil.example/update").unwrap()
        ));
        assert!(!trusted_host(
            &Url::parse("http://github.com/update").unwrap()
        ));
    }
    #[test]
    fn incomplete_corrupt_or_oversized_downloads_never_verify() {
        let bytes = b"complete update";
        let digest = format!("{:x}", Sha256::digest(bytes));
        let mut output = Vec::new();
        let mut progress = Vec::new();
        stream_verified(
            Cursor::new(bytes),
            &mut output,
            bytes.len() as u64,
            &digest,
            |p| progress.push(p),
        )
        .unwrap();
        assert_eq!(output, bytes);
        assert_eq!(progress.last(), Some(&100));
        assert!(
            stream_verified(
                Cursor::new(b"short"),
                Vec::new(),
                bytes.len() as u64,
                &digest,
                |_| {}
            )
            .is_err()
        );
        assert!(
            stream_verified(
                Cursor::new(bytes),
                Vec::new(),
                bytes.len() as u64,
                &"0".repeat(64),
                |_| {}
            )
            .is_err()
        );
        assert!(stream_verified(Cursor::new(bytes), Vec::new(), 1, &digest, |_| {}).is_err());
        assert!(read_bounded(Cursor::new(b"too long"), 3).is_err());
    }
    fn zip(path: &Path, name: &str) {
        let mut writer = zip::ZipWriter::new(File::create(path).unwrap());
        writer
            .start_file(name, zip::write::FileOptions::default())
            .unwrap();
        writer.write_all(b"contents").unwrap();
        writer.finish().unwrap();
    }
    #[test]
    fn archives_cannot_escape_or_substitute_another_application() {
        let root = tempfile::tempdir().unwrap();
        for (i, name) in ["../escape", "/absolute", "Other.app/Contents/executable"]
            .iter()
            .enumerate()
        {
            let source = root.path().join(format!("{i}.zip"));
            zip(&source, name);
            assert!(extract_mac_zip(&source, &root.path().join(format!("stage{i}"))).is_err());
        }
        let source = root.path().join("valid.zip");
        zip(&source, "Asset Forge.app/Contents/Resources/readme");
        extract_mac_zip(&source, &root.path().join("valid")).unwrap();
        assert_eq!(
            fs::read(
                root.path()
                    .join("valid/Asset Forge.app/Contents/Resources/readme")
            )
            .unwrap(),
            b"contents"
        );
        assert!(!root.path().join("escape").exists());
    }
    #[test]
    fn replacement_keeps_backup_and_restores_on_failure() {
        let root = tempfile::tempdir().unwrap();
        let current = root.path().join("current.app");
        let incoming = root.path().join("new.app");
        let backup = root.path().join("backup.app");
        fs::create_dir(&current).unwrap();
        fs::write(current.join("version"), b"old").unwrap();
        assert!(replace_with_backup(&incoming, &current, &backup).is_err());
        assert_eq!(fs::read(current.join("version")).unwrap(), b"old");
        assert!(!backup.exists());
        fs::create_dir(&incoming).unwrap();
        fs::write(incoming.join("version"), b"new").unwrap();
        replace_with_backup(&incoming, &current, &backup).unwrap();
        assert_eq!(fs::read(current.join("version")).unwrap(), b"new");
        assert_eq!(fs::read(backup.join("version")).unwrap(), b"old");
        assert!(replace_with_backup(&incoming, &current, &backup).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn signed_mac_package_installs_without_touching_workspace_and_rejects_tampering() {
        let root = tempfile::tempdir().unwrap();
        let stage = root.path().join(uuid::Uuid::new_v4().to_string());
        let payload = stage.join("payload");
        let app = payload.join("Asset Forge.app");
        let executables = app.join("Contents/MacOS");
        fs::create_dir_all(&executables).unwrap();
        for name in ["asset-forge", "asset-forge-studio"] {
            // A harmless existing Mach-O makes a real signed fixture; no GUI is launched.
            fs::copy("/usr/bin/true", executables.join(name)).unwrap();
        }
        fs::write(
            app.join("Contents/Info.plist"),
            r#"<?xml version="1.0"?><plist version="1.0"><dict>
            <key>CFBundleIdentifier</key><string>dev.assetforge.studio</string>
            <key>CFBundleExecutable</key><string>asset-forge-studio</string>
            <key>CFBundlePackageType</key><string>APPL</string>
            <key>CFBundleShortVersionString</key><string>0.2.0</string>
            <key>CFBundleVersion</key><string>0.2.0</string></dict></plist>"#,
        )
        .unwrap();
        assert!(
            Command::new("/usr/bin/codesign")
                .args(["--force", "--deep", "--sign", "-"])
                .arg(&app)
                .status()
                .unwrap()
                .success()
        );
        let hashes: std::collections::HashMap<_, _> = ["asset-forge", "asset-forge-studio"]
            .into_iter()
            .map(|name| (name, file_hash(&executables.join(name)).unwrap()))
            .collect();
        fs::write(payload.join("release-manifest.json"), serde_json::json!({"schemaVersion":1,"version":"0.2.0","platform":"macos-arm64","sha256":hashes}).to_string()).unwrap();
        let destination = root.path().join("Installed.app");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("previous-version"), b"preserve old app").unwrap();
        let workspace = root.path().join("workspace.sqlite");
        fs::write(&workspace, b"game data is separate").unwrap();
        let plan = InstallPlan {
            schema_version: 1,
            version: "0.2.0".into(),
            platform: Platform::MacosArm64,
            stage,
            destination: destination.clone(),
            parent_pid: 1,
            size: 1,
            sha256: "0".repeat(64),
        };
        let original = fs::read(executables.join("asset-forge")).unwrap();
        fs::write(executables.join("asset-forge"), b"tampered").unwrap();
        assert!(install_mac(&plan).is_err());
        assert!(destination.join("previous-version").is_file());
        fs::write(executables.join("asset-forge"), original).unwrap();
        install_mac(&plan).unwrap();
        assert!(
            destination
                .join("Contents/MacOS/asset-forge-studio")
                .is_file()
        );
        let backup = root.path().join(format!(
            ".AssetForge-previous-{}.app",
            plan.stage.file_name().unwrap().to_string_lossy()
        ));
        assert_eq!(
            fs::read(backup.join("previous-version")).unwrap(),
            b"preserve old app"
        );
        assert_eq!(fs::read(workspace).unwrap(), b"game data is separate");
        assert!(
            Command::new("/usr/bin/codesign")
                .args(["--verify", "--deep", "--strict"])
                .arg(destination)
                .status()
                .unwrap()
                .success()
        );
    }
}
