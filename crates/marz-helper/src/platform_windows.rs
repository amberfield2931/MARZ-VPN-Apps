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
use windows_service::service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType};
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

fn service_main(_: Vec<OsString>) {
    let handler = service_control_handler::register(SERVICE_NAME, move |event| match event {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            // Service stop is not the user's OFF: the engine goes, the filters stay (they are persistent).
            if let Some(s) = SHARED.get() {
                s.shutdown();
                for _ in 0..60 {
                    if s.engine_gone() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            std::process::exit(0);
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    });
    let Ok(handle) = handler else { return };
    let _ = handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    });
    let (pipe, engine, state_dir) = ARGS.get().unwrap();
    serve(pipe, engine.clone(), state_dir);
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
