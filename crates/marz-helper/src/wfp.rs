//! Windows Filtering Platform side of the kill switch. Carries out marz_engine::killswitch::wfp_plan.
//! CROSS-COMPILED ONLY: this has never run on Windows, and nothing here counts as verified until it has
//! (OWNER-TEST-PLAN.md, Windows part).
//!
//! Filters are persistent: they live in the Base Filtering Engine, survive the helper and are restored by
//! Windows at boot. All changes to them happen in one WFP transaction.
use marz_engine::killswitch::{wfp_plan, Endpoint, WfpFilter, WfpLayer, WfpMatch};
use marz_engine::TUN_NAME;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Mutex;
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, FWP_E_ALREADY_EXISTS, HANDLE};
use windows_sys::Win32::NetworkManagement::IpHelper::ConvertInterfaceAliasToLuid;
use windows_sys::Win32::NetworkManagement::Ndis::NET_LUID_LH;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::*;

const PROVIDER: GUID = GUID::from_u128(0x6d61727a_766e_4b53_8000_000000000001);
const SUBLAYER: GUID = GUID::from_u128(0x6d61727a_766e_4b53_8000_000000000002);
const RPC_C_AUTHN_WINNT: u32 = 10;
const PROTO_ICMPV6: u8 = 58;
const PROTO_TCP: u8 = 6;
const PROTO_UDP: u8 = 17;

static ENGINE: Mutex<Option<PathBuf>> = Mutex::new(None);
/// The adapter the applied filters allow, so a new one is noticed.
static APPLIED_TUN: Mutex<Option<u64>> = Mutex::new(None);

pub fn set_engine_path(path: PathBuf) {
    *ENGINE.lock().unwrap() = Some(path);
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct Engine(HANDLE);

impl Engine {
    fn open() -> Result<Self, &'static str> {
        let mut h: HANDLE = std::ptr::null_mut();
        let rc = unsafe { FwpmEngineOpen0(std::ptr::null(), RPC_C_AUTHN_WINNT, std::ptr::null(), std::ptr::null(), &mut h) };
        if rc != ERROR_SUCCESS {
            return Err("killswitch_failed");
        }
        Ok(Engine(h))
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe { FwpmEngineClose0(self.0) };
    }
}

fn layer_key(l: WfpLayer) -> GUID {
    match l {
        WfpLayer::Connect4 => FWPM_LAYER_ALE_AUTH_CONNECT_V4,
        WfpLayer::Connect6 => FWPM_LAYER_ALE_AUTH_CONNECT_V6,
        WfpLayer::Accept4 => FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V4,
        WfpLayer::Accept6 => FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V6,
    }
}

const ALL_LAYERS: [WfpLayer; 4] = [WfpLayer::Connect4, WfpLayer::Connect6, WfpLayer::Accept4, WfpLayer::Accept6];

fn ensure_provider(e: &Engine) -> Result<(), &'static str> {
    let mut name = wide("MARZ VPN");
    let display = FWPM_DISPLAY_DATA0 { name: name.as_mut_ptr(), description: std::ptr::null_mut() };
    let provider = FWPM_PROVIDER0 {
        providerKey: PROVIDER,
        displayData: display,
        flags: FWPM_PROVIDER_FLAG_PERSISTENT,
        providerData: FWP_BYTE_BLOB { size: 0, data: std::ptr::null_mut() },
        serviceName: std::ptr::null_mut(),
    };
    let rc = unsafe { FwpmProviderAdd0(e.0, &provider, std::ptr::null_mut()) };
    if rc != ERROR_SUCCESS && rc != FWP_E_ALREADY_EXISTS as u32 {
        return Err("killswitch_failed");
    }
    let mut key = PROVIDER;
    let sublayer = FWPM_SUBLAYER0 {
        subLayerKey: SUBLAYER,
        displayData: display,
        flags: FWPM_SUBLAYER_FLAG_PERSISTENT,
        providerKey: &mut key,
        providerData: FWP_BYTE_BLOB { size: 0, data: std::ptr::null_mut() },
        weight: u16::MAX,
    };
    let rc = unsafe { FwpmSubLayerAdd0(e.0, &sublayer, std::ptr::null_mut()) };
    if rc != ERROR_SUCCESS && rc != FWP_E_ALREADY_EXISTS as u32 {
        return Err("killswitch_failed");
    }
    Ok(())
}

/// Ids of the filters of our provider on one layer.
fn our_filters(e: &Engine, layer: WfpLayer) -> Result<Vec<u64>, &'static str> {
    let mut key = PROVIDER;
    let mut template: FWPM_FILTER_ENUM_TEMPLATE0 = unsafe { std::mem::zeroed() };
    template.providerKey = &mut key;
    template.layerKey = layer_key(layer);
    template.enumType = FWP_FILTER_ENUM_OVERLAPPING;
    template.actionMask = 0xFFFF_FFFF;
    let mut handle: HANDLE = std::ptr::null_mut();
    if unsafe { FwpmFilterCreateEnumHandle0(e.0, &template, &mut handle) } != ERROR_SUCCESS {
        return Err("killswitch_failed");
    }
    let mut ids = Vec::new();
    let mut failed = false;
    loop {
        let mut entries: *mut *mut FWPM_FILTER0 = std::ptr::null_mut();
        let mut n = 0u32;
        if unsafe { FwpmFilterEnum0(e.0, handle, 256, &mut entries, &mut n) } != ERROR_SUCCESS {
            failed = true;
            break;
        }
        for i in 0..n as usize {
            ids.push(unsafe { (**entries.add(i)).filterId });
        }
        if !entries.is_null() {
            let mut p = entries as *mut core::ffi::c_void;
            unsafe { FwpmFreeMemory0(&mut p) };
        }
        if n < 256 {
            break;
        }
    }
    unsafe { FwpmFilterDestroyEnumHandle0(e.0, handle) };
    if failed {
        Err("killswitch_failed")
    } else {
        Ok(ids)
    }
}

/// Holds everything a condition points at, alive until the filter has been added.
#[derive(Default)]
struct Conditions {
    list: Vec<FWPM_FILTER_CONDITION0>,
    luid: Vec<Box<u64>>,
    addr6: Vec<Box<FWP_BYTE_ARRAY16>>,
}

impl Conditions {
    fn raw(&mut self, field: GUID, kind: FWP_MATCH_TYPE, value: FWP_CONDITION_VALUE0) {
        self.list.push(FWPM_FILTER_CONDITION0 { fieldKey: field, matchType: kind, conditionValue: value });
    }
    fn u8(&mut self, field: GUID, v: u8) {
        self.raw(field, FWP_MATCH_EQUAL, FWP_CONDITION_VALUE0 { r#type: FWP_UINT8, Anonymous: FWP_CONDITION_VALUE0_0 { uint8: v } });
    }
    fn u16(&mut self, field: GUID, v: u16) {
        self.raw(field, FWP_MATCH_EQUAL, FWP_CONDITION_VALUE0 { r#type: FWP_UINT16, Anonymous: FWP_CONDITION_VALUE0_0 { uint16: v } });
    }
    fn tun(&mut self, luid: u64) {
        self.luid.push(Box::new(luid));
        let p = self.luid.last_mut().unwrap().as_mut() as *mut u64;
        self.raw(FWPM_CONDITION_IP_LOCAL_INTERFACE, FWP_MATCH_EQUAL, FWP_CONDITION_VALUE0 { r#type: FWP_UINT64, Anonymous: FWP_CONDITION_VALUE0_0 { uint64: p } });
    }
    fn remote(&mut self, ip: IpAddr) {
        match ip {
            IpAddr::V4(a) => self.raw(
                FWPM_CONDITION_IP_REMOTE_ADDRESS,
                FWP_MATCH_EQUAL,
                FWP_CONDITION_VALUE0 { r#type: FWP_UINT32, Anonymous: FWP_CONDITION_VALUE0_0 { uint32: u32::from(a) } },
            ),
            IpAddr::V6(a) => {
                self.addr6.push(Box::new(FWP_BYTE_ARRAY16 { byteArray16: a.octets() }));
                let p = self.addr6.last_mut().unwrap().as_mut() as *mut FWP_BYTE_ARRAY16;
                self.raw(FWPM_CONDITION_IP_REMOTE_ADDRESS, FWP_MATCH_EQUAL, FWP_CONDITION_VALUE0 { r#type: FWP_BYTE_ARRAY16_TYPE, Anonymous: FWP_CONDITION_VALUE0_0 { byteArray16: p } });
            }
        }
    }
}

fn add_filter(e: &Engine, f: &WfpFilter, app_id: *mut FWP_BYTE_BLOB, tun: Option<u64>) -> Result<(), &'static str> {
    let mut c = Conditions::default();
    match &f.matches {
        WfpMatch::All => {}
        WfpMatch::Loopback => c.raw(
            FWPM_CONDITION_FLAGS,
            FWP_MATCH_FLAGS_ALL_SET,
            FWP_CONDITION_VALUE0 { r#type: FWP_UINT32, Anonymous: FWP_CONDITION_VALUE0_0 { uint32: FWP_CONDITION_FLAG_IS_LOOPBACK } },
        ),
        WfpMatch::TunAdapter => c.tun(tun.ok_or("killswitch_failed")?),
        WfpMatch::EngineTo(ep) => {
            if app_id.is_null() {
                return Err("killswitch_failed");
            }
            c.raw(FWPM_CONDITION_ALE_APP_ID, FWP_MATCH_EQUAL, FWP_CONDITION_VALUE0 { r#type: FWP_BYTE_BLOB_TYPE, Anonymous: FWP_CONDITION_VALUE0_0 { byteBlob: app_id } });
            c.u8(FWPM_CONDITION_IP_PROTOCOL, PROTO_TCP);
            c.remote(ep.ip);
            c.u16(FWPM_CONDITION_IP_REMOTE_PORT, ep.port);
        }
        WfpMatch::DhcpV4 | WfpMatch::DhcpV6 => {
            let (client, server) = if f.matches == WfpMatch::DhcpV4 { (68, 67) } else { (546, 547) };
            c.u8(FWPM_CONDITION_IP_PROTOCOL, PROTO_UDP);
            c.u16(FWPM_CONDITION_IP_LOCAL_PORT, client);
            c.u16(FWPM_CONDITION_IP_REMOTE_PORT, server);
        }
        // On the ALE layers the ICMP type is reported in the local port field.
        WfpMatch::NeighbourDiscovery(kind) => {
            c.u8(FWPM_CONDITION_IP_PROTOCOL, PROTO_ICMPV6);
            c.u16(FWPM_CONDITION_IP_LOCAL_PORT, *kind as u16);
        }
    }
    let mut name = wide(if f.permit { "MARZ VPN kill switch: permit" } else { "MARZ VPN kill switch: block" });
    let mut provider = PROVIDER;
    let mut filter: FWPM_FILTER0 = unsafe { std::mem::zeroed() };
    filter.displayData.name = name.as_mut_ptr();
    filter.flags = FWPM_FILTER_FLAG_PERSISTENT | if f.permit { 0 } else { FWPM_FILTER_FLAG_CLEAR_ACTION_RIGHT };
    filter.providerKey = &mut provider;
    filter.layerKey = layer_key(f.layer);
    filter.subLayerKey = SUBLAYER;
    filter.weight = FWP_VALUE0 { r#type: FWP_UINT8, Anonymous: FWP_VALUE0_0 { uint8: f.weight } };
    filter.numFilterConditions = c.list.len() as u32;
    filter.filterCondition = if c.list.is_empty() { std::ptr::null_mut() } else { c.list.as_mut_ptr() };
    filter.action.r#type = if f.permit { FWP_ACTION_PERMIT } else { FWP_ACTION_BLOCK };
    let mut id = 0u64;
    let rc = unsafe { FwpmFilterAdd0(e.0, &filter, std::ptr::null_mut(), &mut id) };
    if rc == ERROR_SUCCESS {
        Ok(())
    } else {
        Err("killswitch_failed")
    }
}

fn tun_luid() -> Option<u64> {
    let mut luid: NET_LUID_LH = unsafe { std::mem::zeroed() };
    let name = wide(TUN_NAME);
    let rc = unsafe { ConvertInterfaceAliasToLuid(name.as_ptr(), &mut luid) };
    (rc == 0).then(|| unsafe { luid.Value })
}

fn engine_app_id() -> *mut FWP_BYTE_BLOB {
    let Some(path) = ENGINE.lock().unwrap().clone() else { return std::ptr::null_mut() };
    let wide_path: Vec<u16> = path.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
    let mut blob: *mut FWP_BYTE_BLOB = std::ptr::null_mut();
    if unsafe { FwpmGetAppIdFromFileName0(wide_path.as_ptr(), &mut blob) } != ERROR_SUCCESS {
        return std::ptr::null_mut();
    }
    blob
}

/// Replaces our filters in one transaction: any failure aborts it, and what was in force stays in force.
pub fn apply(endpoints: &[Endpoint]) -> Result<(), &'static str> {
    let e = Engine::open()?;
    ensure_provider(&e)?;
    let tun = tun_luid();
    let plan = wfp_plan(endpoints, tun.is_some());
    let app_id = engine_app_id();
    if unsafe { FwpmTransactionBegin0(e.0, 0) } != ERROR_SUCCESS {
        return Err("killswitch_failed");
    }
    let work = || -> Result<(), &'static str> {
        for layer in ALL_LAYERS {
            for id in our_filters(&e, layer)? {
                if unsafe { FwpmFilterDeleteById0(e.0, id) } != ERROR_SUCCESS {
                    return Err("killswitch_failed");
                }
            }
        }
        for f in &plan {
            add_filter(&e, f, app_id, tun)?;
        }
        Ok(())
    };
    let done = work();
    let finished = match done {
        Ok(()) if unsafe { FwpmTransactionCommit0(e.0) } == ERROR_SUCCESS => Ok(()),
        Ok(()) => Err("killswitch_failed"),
        Err(code) => {
            unsafe { FwpmTransactionAbort0(e.0) };
            Err(code)
        }
    };
    if !app_id.is_null() {
        let mut p = app_id as *mut core::ffi::c_void;
        unsafe { FwpmFreeMemory0(&mut p) };
    }
    if finished.is_ok() {
        *APPLIED_TUN.lock().unwrap() = tun;
    }
    finished
}

pub fn remove() -> Result<(), &'static str> {
    let e = Engine::open()?;
    // Listing filters of an unknown provider may be an error; with nothing to remove that must still succeed.
    ensure_provider(&e)?;
    if unsafe { FwpmTransactionBegin0(e.0, 0) } != ERROR_SUCCESS {
        return Err("killswitch_failed");
    }
    let work = || -> Result<(), &'static str> {
        for layer in ALL_LAYERS {
            for id in our_filters(&e, layer)? {
                if unsafe { FwpmFilterDeleteById0(e.0, id) } != ERROR_SUCCESS {
                    return Err("killswitch_failed");
                }
            }
        }
        Ok(())
    };
    match work() {
        Ok(()) if unsafe { FwpmTransactionCommit0(e.0) } == ERROR_SUCCESS => {}
        Ok(()) => return Err("killswitch_failed"),
        Err(code) => {
            unsafe { FwpmTransactionAbort0(e.0) };
            return Err(code);
        }
    }
    // Best effort: an empty provider and sublayer block nothing.
    unsafe {
        FwpmSubLayerDeleteByKey0(e.0, &SUBLAYER);
        FwpmProviderDeleteByKey0(e.0, &PROVIDER);
    }
    *APPLIED_TUN.lock().unwrap() = None;
    Ok(())
}

/// The block filter is there on each layer.
pub fn present() -> bool {
    let Ok(e) = Engine::open() else { return false };
    ALL_LAYERS.iter().all(|l| matches!(our_filters(&e, *l), Ok(ids) if !ids.is_empty()))
}

/// The tunnel adapter appeared, went away or changed: the filters must name the current one.
pub fn tun_changed() -> bool {
    tun_luid() != *APPLIED_TUN.lock().unwrap()
}
