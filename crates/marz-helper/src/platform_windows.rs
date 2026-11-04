//! Windows: a LocalSystem service with a named pipe for the signed-in user's app. Cross-compiled;
//! NOT run on real Windows yet (see SECURITY-TEST-RESULTS.md).
#[path = "wfp.rs"]
mod wfp;
use crate::supervisor::Shared;
use crate::{serve_client, Args};
use marz_engine::auth::Peer;
use marz_engine::ipc::WINDOWS_PIPE;
use std::ffi::{c_void, OsString};
use std::io::{Read, Write};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use windows_service::service::{
    Service, ServiceAccess, ServiceAction, ServiceActionType, ServiceControl, ServiceControlAccept, ServiceDependency, ServiceErrorControl, ServiceExitCode, ServiceFailureActions,
    ServiceFailureResetPeriod, ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::{define_windows_service, service_dispatcher};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, LocalFree, ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{FlushFileBuffers, ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TokenUser, TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER};
use windows_sys::Win32::System::Threading::{OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};

const SERVICE_NAME: &str = "MarzVpnHelper";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Full control for SYSTEM and administrators; read/write for signed-in interactive users, nobody else.
const PIPE_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)";

define_windows_service!(ffi_service_main, service_main);

static ARGS: std::sync::OnceLock<(String, PathBuf, PathBuf)> = std::sync::OnceLock::new();
static SHARED: std::sync::OnceLock<Arc<Shared>> = std::sync::OnceLock::new();
/// The engine belongs to this job: when the helper ends in any way, Windows ends the engine with it.
static JOB: std::sync::OnceLock<usize> = std::sync::OnceLock::new();

fn job() -> Option<HANDLE> {
    let h = *JOB.get_or_init(|| unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return 0;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = SetInformationJobObject(job, JobObjectExtendedLimitInformation, &info as *const _ as *const c_void, std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32);
        if ok == 0 {
            return 0;
        }
        job as usize
    });
    (h != 0).then_some(h as HANDLE)
}

pub fn after_spawn(child: &Child) {
    if let Some(j) = job() {
        unsafe { AssignProcessToJobObject(j, child.as_raw_handle() as HANDLE) };
    }
}

pub fn main(args: Args) {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_default();
    let engine = args.engine.unwrap_or_else(|| exe_dir.join("marzvpn-engine.exe"));
    let pipe = args.endpoint.unwrap_or_else(|| WINDOWS_PIPE.to_string());
    let state_dir = args.state_dir.unwrap_or_else(default_state_dir);
    wfp::set_engine_path(engine.clone());
    let _ = ARGS.set((pipe, engine, state_dir));
    let as_service = std::env::args().any(|a| a == "--service");
    if as_service {
        let _ = service_dispatcher::start(SERVICE_NAME, ffi_service_main);
    } else {
        let (pipe, engine, state_dir) = ARGS.get().unwrap();
        serve(pipe, engine.clone(), state_dir);
    }
}

pub fn default_state_dir() -> PathBuf {
    let base = std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    base.join("MARZ VPN")
}

pub fn ks_apply(endpoints: &[marz_engine::killswitch::Endpoint]) -> Result<(), &'static str> {
    wfp::apply(endpoints)
}

pub fn ks_remove() -> Result<(), &'static str> {
    wfp::remove()
}

pub fn ks_present() -> bool {
    wfp::present()
}

/// sing-box's strict_route adds Windows firewall rules against DNS over other adapters (sing-box documentation;
/// not verified here, see SECURITY-TEST-RESULTS.md).
pub fn dns_guard_on() -> Result<(), &'static str> {
    Ok(())
}

/// Wintun routes live on the adapter, and the engine goes with it.
pub fn tunnel_routes_intact() -> bool {
    true
}

pub fn ks_needs_refresh() -> bool {
    wfp::tun_changed()
}

/// The service manager's contract: answer the control at once, report STOP_PENDING while working, and STOPPED before the
/// process ends. A process that ends without STOPPED counts as a crash, and the recovery actions start it again a second
/// later. Up to 0.1.5 the helper did exactly that, so `sc stop` brought it back and the installer met a locked file.
fn service_main(_: Vec<OsString>) {
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let from_control = stop_tx.clone();
    let handler = service_control_handler::register(SERVICE_NAME, move |event| match event {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            let _ = from_control.send(());
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    });
    let Ok(handle) = handler else { return };
    let report = |state: ServiceState, accepts: ServiceControlAccept, checkpoint: u32, hint: Duration| {
        let _ = handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: accepts,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint,
            wait_hint: hint,
            process_id: None,
        });
    };
    report(ServiceState::Running, ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN, 0, Duration::default());
    std::thread::spawn(move || {
        let (pipe, engine, state_dir) = ARGS.get().unwrap();
        serve(pipe, engine.clone(), state_dir);
        let _ = stop_tx.send(());
    });
    let _ = stop_rx.recv();
    report(ServiceState::StopPending, ServiceControlAccept::empty(), 1, Duration::from_secs(10));
    // Service stop is not the user's OFF: the engine goes, the filters stay (they are persistent).
    if let Some(s) = SHARED.get() {
        s.shutdown();
        for i in 0..60u32 {
            if s.engine_gone() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
            report(ServiceState::StopPending, ServiceControlAccept::empty(), 2 + i, Duration::from_secs(10));
        }
    }
    report(ServiceState::Stopped, ServiceControlAccept::empty(), 0, Duration::default());
    std::process::exit(0);
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn serve(pipe: &str, engine: PathBuf, state_dir: &std::path::Path) {
    let shared = Shared::new(engine, state_dir);
    let _ = SHARED.set(shared.clone());
    {
        let s = shared.clone();
        std::thread::spawn(move || s.run());
    }
    {
        let s = shared.clone();
        std::thread::spawn(move || s.watchdog());
    }
    let mut sd: *mut c_void = std::ptr::null_mut();
    let sddl = wide(PIPE_SDDL);
    let ok = unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, std::ptr::null_mut()) };
    if ok == 0 {
        return;
    }
    let mut attrs = SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: sd, bInheritHandle: 0 };
    let name = wide(pipe);
    let mut first = FILE_FLAG_FIRST_PIPE_INSTANCE;
    loop {
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | first,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                65536,
                65536,
                0,
                &mut attrs,
            )
        };
        first = 0;
        if handle == INVALID_HANDLE_VALUE {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        let connected = unsafe { ConnectNamedPipe(handle, std::ptr::null_mut()) } != 0 || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
        if !connected {
            unsafe { CloseHandle(handle) };
            continue;
        }
        let s: Arc<Shared> = shared.clone();
        let raw = handle as usize;
        std::thread::spawn(move || {
            if let Some(peer) = peer_of_pipe(raw as HANDLE) {
                serve_client(&s, PipeStream(raw as HANDLE), peer);
            }
            unsafe {
                FlushFileBuffers(raw as HANDLE);
                DisconnectNamedPipe(raw as HANDLE);
                CloseHandle(raw as HANDLE);
            }
        });
    }
    #[allow(unreachable_code)]
    unsafe {
        LocalFree(sd);
    }
}

/// The pipe client's identity from its process token. CROSS-COMPILED ONLY, never run. None: refuse the client.
fn peer_of_pipe(pipe: HANDLE) -> Option<Peer> {
    unsafe {
        let mut pid = 0u32;
        if GetNamedPipeClientProcessId(pipe, &mut pid) == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut token: HANDLE = std::ptr::null_mut();
        let opened = OpenProcessToken(process, TOKEN_QUERY, &mut token);
        CloseHandle(process);
        if opened == 0 {
            return None;
        }
        let peer = peer_of_token(token);
        CloseHandle(token);
        peer
    }
}

unsafe fn peer_of_token(token: HANDLE) -> Option<Peer> {
    let mut buffer = [0u64; 64];
    let mut len = 0u32;
    if GetTokenInformation(token, TokenUser, buffer.as_mut_ptr() as *mut c_void, std::mem::size_of_val(&buffer) as u32, &mut len) == 0 {
        return None;
    }
    let user = &*(buffer.as_ptr() as *const TOKEN_USER);
    let mut text: *mut u16 = std::ptr::null_mut();
    if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
        return None;
    }
    let mut n = 0;
    while *text.add(n) != 0 {
        n += 1;
    }
    let id = String::from_utf16_lossy(std::slice::from_raw_parts(text, n));
    LocalFree(text as _);
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut len = 0u32;
    let elevated = GetTokenInformation(token, TokenElevation, &mut elevation as *mut _ as *mut c_void, std::mem::size_of::<TOKEN_ELEVATION>() as u32, &mut len) != 0 && elevation.TokenIsElevated != 0;
    // SYSTEM, or an administrator who went through UAC.
    Some(Peer { admin: elevated || id == "S-1-5-18", id })
}

struct PipeStream(HANDLE);
unsafe impl Send for PipeStream {}

impl Read for PipeStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut n = 0u32;
        let ok = unsafe { ReadFile(self.0, buf.as_mut_ptr(), buf.len() as u32, &mut n, std::ptr::null_mut()) };
        if ok == 0 {
            return Ok(0);
        }
        Ok(n as usize)
    }
}

impl Write for PipeStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut n = 0u32;
        let ok = unsafe { WriteFile(self.0, buf.as_ptr(), buf.len() as u32, &mut n, std::ptr::null_mut()) };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(n as usize)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn prepare_child(cmd: &mut Command) {
    cmd.creation_flags(CREATE_NO_WINDOW);
}

pub fn request_exit(child: &mut Child) {
    let _ = child.kill();
}

/// Wintun routes live on the adapter and disappear with it; nothing global is left to remove.
pub fn cleanup_network() {}

// ---- installer steps -----------------------------------------------------------------------------------------------

const SERVICE_DISPLAY: &str = "MARZ VPN helper";
const ERROR_SERVICE_DOES_NOT_EXIST: i32 = 1060;
const ERROR_SERVICE_MARKED_FOR_DELETE: i32 = 1072;
const SHARING_VIOLATION: i32 = 32;
const LOCK_VIOLATION: i32 = 33;
const STOP_WAIT: Duration = Duration::from_secs(20);
const DELETE_WAIT: Duration = Duration::from_secs(30);
const ANSWER_WAIT: Duration = Duration::from_secs(20);
const PIPE_ANSWER_WAIT: Duration = Duration::from_secs(15);

fn system_tool(name: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    root.join("System32").join(name)
}

fn taskkill(args: &[&str]) {
    let mut cmd = Command::new(system_tool("taskkill.exe"));
    cmd.args(args).args(["/F"]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    prepare_child(&mut cmd);
    let _ = cmd.status();
}

/// One request to whatever helper is running, of any version; a hung helper does not hang the installer.
fn pipe_call(request: &marz_engine::ipc::Request) -> Option<marz_engine::ipc::Response> {
    let line = serde_json::to_vec(request).ok()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let answer = (|| {
            let mut f = std::fs::OpenOptions::new().read(true).write(true).open(WINDOWS_PIPE).ok()?;
            f.write_all(&line).ok()?;
            f.write_all(b"\n").ok()?;
            let mut reply = Vec::new();
            std::io::BufRead::read_until(&mut std::io::BufReader::new(f.take(marz_engine::ipc::MAX_LINE as u64)), b'\n', &mut reply).ok()?;
            serde_json::from_slice(&reply).ok()
        })();
        let _ = tx.send(answer);
    });
    rx.recv_timeout(PIPE_ANSWER_WAIT).ok().flatten()
}

fn helper_status() -> Option<marz_engine::ipc::Status> {
    match pipe_call(&marz_engine::ipc::Request::Status)? {
        marz_engine::ipc::Response::Status(s) => Some(s),
        _ => None,
    }
}

struct WinHost {
    state_dir: PathBuf,
    log: std::sync::Mutex<Option<std::fs::File>>,
}

impl WinHost {
    fn new(state_dir: &std::path::Path) -> Self {
        let _ = std::fs::create_dir_all(state_dir);
        let log = std::fs::OpenOptions::new().create(true).append(true).open(state_dir.join("install.log")).ok();
        WinHost { state_dir: state_dir.to_path_buf(), log: std::sync::Mutex::new(log) }
    }

    fn manager() -> Option<ServiceManager> {
        ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE).ok()
    }

    fn open(access: ServiceAccess) -> Result<Service, i32> {
        let manager = Self::manager().ok_or(0)?;
        manager.open_service(SERVICE_NAME, access).map_err(|e| match e {
            windows_service::Error::Winapi(io) => io.raw_os_error().unwrap_or(0),
            _ => 0,
        })
    }

    fn all_access() -> ServiceAccess {
        ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::START | ServiceAccess::DELETE | ServiceAccess::CHANGE_CONFIG | ServiceAccess::QUERY_CONFIG
    }

    fn stopped(service: &Service) -> bool {
        service.query_status().is_ok_and(|s| s.current_state == ServiceState::Stopped)
    }

    fn service_info(exe: &std::path::Path) -> ServiceInfo {
        ServiceInfo {
            name: SERVICE_NAME.into(),
            display_name: SERVICE_DISPLAY.into(),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: exe.to_path_buf(),
            launch_arguments: vec!["--service".into()],
            dependencies: vec![ServiceDependency::Service("BFE".into())],
            account_name: None,
            account_password: None,
        }
    }
}

impl crate::install::Host for WinHost {
    fn log(&self, line: &str) {
        println!("{line}");
        if let Some(f) = self.log.lock().unwrap().as_mut() {
            let _ = writeln!(f, "{line}");
        }
    }

    fn close_app(&self) {
        taskkill(&["/IM", "marzvpn.exe"]);
    }

    fn helper_off(&self) -> bool {
        match pipe_call(&marz_engine::ipc::Request::EmergencyOff) {
            Some(marz_engine::ipc::Response::Status(_)) => true,
            Some(marz_engine::ipc::Response::Error { code }) => {
                crate::install::Host::log(self, &format!("the helper answered: {code}"));
                false
            }
            _ => false,
        }
    }

    fn service_installed(&self) -> bool {
        Self::open(ServiceAccess::QUERY_STATUS).is_ok()
    }

    fn forbid_restart(&self) {
        let Ok(service) = Self::open(Self::all_access()) else { return };
        let none = ServiceFailureActions { reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(0)), reboot_msg: None, command: None, actions: Some(vec![]) };
        if service.update_failure_actions(none).is_err() {
            crate::install::Host::log(self, "could not clear the recovery actions");
        }
    }

    fn stop_service(&self) -> bool {
        let service = match Self::open(Self::all_access()) {
            Ok(service) => service,
            Err(code) => return code == ERROR_SERVICE_DOES_NOT_EXIST,
        };
        let begun = Instant::now();
        let mut pid = None;
        while begun.elapsed() < STOP_WAIT {
            match service.query_status() {
                Ok(s) if s.current_state == ServiceState::Stopped => break,
                Ok(s) => pid = s.process_id.or(pid),
                Err(_) => break,
            }
            // An old helper ends its process inside this call, so this reports an error. Only the state counts.
            let _ = service.stop();
            std::thread::sleep(Duration::from_millis(250));
        }
        if !Self::stopped(&service) {
            crate::install::Host::log(self, "the service did not stop in time: ending its process");
            if let Some(pid) = service.query_status().ok().and_then(|s| s.process_id).or(pid) {
                taskkill(&["/PID", &pid.to_string()]);
            }
            for _ in 0..40 {
                if Self::stopped(&service) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        }
        // It belongs to the helper's job and ends with it. Only an engine left by a damaged install stays.
        taskkill(&["/IM", "marzvpn-engine.exe"]);
        Self::stopped(&service)
    }

    fn recover_network(&self) -> Result<(), &'static str> {
        crate::killswitch::recover(&self.state_dir)
    }

    fn filters_present(&self) -> Result<bool, &'static str> {
        wfp::any_present()
    }

    fn file_locked(&self, path: &std::path::Path) -> bool {
        use std::os::windows::fs::OpenOptionsExt;
        match std::fs::OpenOptions::new().write(true).share_mode(0).open(path) {
            Err(e) => matches!(e.raw_os_error(), Some(SHARING_VIOLATION | LOCK_VIOLATION)),
            Ok(_) => false,
        }
    }

    fn delete_service(&self) -> bool {
        match Self::open(Self::all_access()) {
            Ok(service) => {
                if service.delete().is_err() {
                    return false;
                }
            }
            Err(ERROR_SERVICE_DOES_NOT_EXIST) => return true,
            Err(_) => return false,
        }
        // Deleted means marked: it is gone only when the last handle closes, and a new one cannot be created before that.
        let begun = Instant::now();
        while begun.elapsed() < DELETE_WAIT {
            match Self::open(ServiceAccess::QUERY_STATUS) {
                Err(ERROR_SERVICE_DOES_NOT_EXIST) => return true,
                _ => std::thread::sleep(Duration::from_millis(250)),
            }
        }
        false
    }

    fn create_service(&self, exe: &std::path::Path) -> bool {
        let info = Self::service_info(exe);
        let service = match Self::open(Self::all_access()) {
            Ok(service) => service.change_config(&info).is_ok().then_some(service),
            Err(_) => {
                let Some(manager) = Self::manager() else { return false };
                let begun = Instant::now();
                loop {
                    match manager.create_service(&info, Self::all_access()) {
                        Ok(service) => break Some(service),
                        Err(windows_service::Error::Winapi(e)) if e.raw_os_error() == Some(ERROR_SERVICE_MARKED_FOR_DELETE) && begun.elapsed() < DELETE_WAIT => {
                            std::thread::sleep(Duration::from_millis(250));
                        }
                        Err(_) => break None,
                    }
                }
            }
        };
        let Some(service) = service else { return false };
        // A real crash still brings the service back: the crash-restart recovery that the block relies on.
        let restart = |ms| ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_millis(ms) };
        let actions = ServiceFailureActions {
            reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(60)),
            reboot_msg: None,
            command: None,
            actions: Some(vec![restart(1000), restart(1000), restart(5000)]),
        };
        if service.update_failure_actions(actions).is_err() {
            crate::install::Host::log(self, "could not set the recovery actions");
        }
        true
    }

    fn start_service(&self) -> bool {
        let Ok(service) = Self::open(Self::all_access()) else { return false };
        service.start::<OsString>(&[]).is_ok()
    }

    /// Antivirus scanners open newly written programs for a moment, and a rename fails meanwhile.
    fn rename(&self, from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
        let mut last = None;
        for _ in 0..40 {
            match std::fs::rename(from, to) {
                Ok(()) => return Ok(()),
                Err(e) if matches!(e.raw_os_error(), Some(5 | SHARING_VIOLATION | LOCK_VIOLATION)) => {
                    last = Some(e);
                    std::thread::sleep(Duration::from_millis(250));
                }
                Err(e) => return Err(e),
            }
        }
        Err(last.unwrap_or_else(|| std::io::Error::other("rename")))
    }

    fn running_version(&self) -> Option<String> {
        let begun = Instant::now();
        while begun.elapsed() < ANSWER_WAIT {
            if let Some(status) = helper_status() {
                return Some(status.version);
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        None
    }
}

/// `--upgrade-inspect`, `--install-from-stage DIR`, `--remove-service DIR`. The exit code is the whole answer to the installer.
pub fn installer_step(args: &Args, state_dir: &std::path::Path) -> i32 {
    use crate::install::{self, Host};
    let host = WinHost::new(state_dir);
    match (args.action, &args.install_dir) {
        (crate::Action::UpgradeInspect, _) => {
            // Whatever cannot be established counts as on: a VPN is never turned off on a guess.
            let running = WinHost::open(ServiceAccess::QUERY_STATUS).is_ok_and(|s| s.query_status().is_ok_and(|q| q.current_state != ServiceState::Stopped));
            let status = helper_status();
            let silent_service = running && status.is_none();
            let asked = status.is_some_and(|s| s.wanted);
            let saved = state_dir.join("killswitch.json").exists();
            let filters = wfp::any_present().unwrap_or(true);
            host.log(&format!("inspect: vpn wanted {asked}, service not answering {silent_service}, saved state {saved}, filters {filters}"));
            if asked || silent_service || saved || filters { install::EXIT_ACTIVE } else { 0 }
        }
        (crate::Action::InstallFromStage, Some(dir)) => {
            host.log(&format!("install {} into {}", env!("CARGO_PKG_VERSION"), dir.display()));
            let code = match install::install(&host, dir, env!("CARGO_PKG_VERSION")) {
                Ok(()) => 0,
                Err(fail) => fail as i32,
            };
            host.log(&format!("result {code}"));
            code
        }
        (crate::Action::RemoveService, Some(dir)) => match install::remove(&host, dir) {
            Ok(()) => 0,
            Err(fail) => fail as i32,
        },
        _ => 2,
    }
}
