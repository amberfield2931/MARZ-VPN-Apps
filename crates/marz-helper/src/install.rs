//! Windows install, upgrade and removal, as one tested sequence. Everything that can be decided without Windows lives
//! here; `platform_windows::WinHost` only carries out single steps. The tests below run this on any system.
//!
//! The order is what makes an upgrade safe: nothing is touched until the new files are whole, nothing is replaced while
//! anything of the old version runs, and every failure ends with one complete version installed and the internet open.
use sha2::{Digest, Sha256};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const STAGE: &str = ".update-new";
pub const BACKUP: &str = ".update-old";
pub const MANIFEST: &str = "install-manifest.txt";
/// Files an older version installed and this one does not.
pub const LEGACY: &[&str] = &["sing-box.exe"];
pub const HELPER_EXE: &str = "marzvpn-helper.exe";
/// `--upgrade-inspect`: the VPN or the block is on, and the upgrade will turn them off.
pub const EXIT_ACTIVE: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fail {
    /// The new files are damaged. Nothing was touched.
    BadStage = 20,
    /// The old service would not stop. The old version is back in service.
    ServiceStuck = 21,
    /// Filters could not be removed: the computer may be blocked, see RECOVERY.txt.
    FiltersLeft = 22,
    /// Something still holds a program file. The old version is back in service.
    FileLocked = 23,
    /// Windows has not finished removing the old service. Nothing was replaced.
    ServiceDeletePending = 24,
    /// The new version did not start. The old version is back in service.
    RolledBack = 25,
    /// The new version did not start and the old one could not be fully restored.
    RollbackFailed = 26,
}

pub trait Host {
    fn log(&self, line: &str);
    /// Ends the GUI in every session: it could otherwise turn the VPN on again during the upgrade.
    fn close_app(&self);
    /// The provided OFF: asks the running helper, as an administrator. False if it could not be asked.
    fn helper_off(&self) -> bool;
    fn service_installed(&self) -> bool;
    /// Windows must not bring the service back while it is being replaced.
    fn forbid_restart(&self);
    /// Stops the service and whatever remains of its processes. True only when none of them runs.
    fn stop_service(&self) -> bool;
    /// Saved ON state, then the filters.
    fn recover_network(&self) -> Result<(), &'static str>;
    /// Any filter of ours on any layer. Err: could not be determined, which never counts as "none".
    fn filters_present(&self) -> Result<bool, &'static str>;
    fn file_locked(&self, path: &Path) -> bool;
    /// Deleted and gone from the service list, not merely marked for deletion.
    fn delete_service(&self) -> bool;
    /// Creates the service, or updates it if it exists, with its recovery actions.
    fn create_service(&self, exe: &Path) -> bool;
    fn start_service(&self) -> bool;
    /// The version the running service reports over its pipe. None: it does not answer.
    fn running_version(&self) -> Option<String>;
    fn wait(&self, d: Duration) {
        std::thread::sleep(d);
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

/// `<sha256>  <size>  <name>` per line. A name is one plain file name: a manifest can never point outside the folder.
pub fn parse_manifest(text: &str) -> Option<Vec<Entry>> {
    let mut out = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let mut p = line.splitn(3, "  ");
        let (sha256, size, name) = (p.next()?, p.next()?, p.next()?.trim_end());
        let plain = !name.is_empty() && !name.contains(['/', '\\', ':']) && name != "." && name != "..";
        if !plain || sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        out.push(Entry { name: name.to_string(), size: size.parse().ok()?, sha256: sha256.to_ascii_lowercase() });
    }
    (!out.is_empty() && out.iter().any(|e| e.name == HELPER_EXE)).then_some(out)
}

pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut hash = Sha256::new();
    io::copy(&mut std::fs::File::open(path)?, &mut hash)?;
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Every new file present, whole and the one that was built.
pub fn read_stage(dir: &Path) -> Option<Vec<Entry>> {
    let stage = dir.join(STAGE);
    let entries = parse_manifest(&std::fs::read_to_string(stage.join(MANIFEST)).ok()?)?;
    for e in &entries {
        let path = stage.join(&e.name);
        let ok = std::fs::metadata(&path).is_ok_and(|m| m.len() == e.size) && sha256_file(&path).is_ok_and(|h| h == e.sha256);
        if !ok {
            return None;
        }
    }
    Some(entries)
}

/// The filters are gone, confirmed by reading them back. Stops at the first sure answer.
fn clear_filters<H: Host>(h: &H) -> Result<(), Fail> {
    for attempt in 0..3 {
        if attempt > 0 {
            h.wait(Duration::from_secs(1));
        }
        if let Err(code) = h.recover_network() {
            h.log(&format!("removing filters: {code}"));
            continue;
        }
        match h.filters_present() {
            Ok(false) => return Ok(()),
            Ok(true) => h.log("filters still present"),
            Err(code) => h.log(&format!("reading filters: {code}")),
        }
    }
    Err(Fail::FiltersLeft)
}

fn first_locked<H: Host>(h: &H, dir: &Path, names: &[String]) -> Option<String> {
    let mut locked = None;
    for attempt in 0..10 {
        if attempt > 0 {
            h.close_app();
            h.wait(Duration::from_secs(1));
        }
        locked = names.iter().find(|n| h.file_locked(&dir.join(n))).cloned();
        if locked.is_none() {
            return None;
        }
    }
    locked
}

/// A power cut during an earlier replacement left the old files aside: put them back before doing anything else.
fn recover_interrupted<H: Host>(h: &H, dir: &Path) {
    let Ok(list) = std::fs::read_dir(dir.join(BACKUP)) else { return };
    h.log("an earlier upgrade did not finish: restoring its saved files");
    for f in list.flatten() {
        if let Err(e) = h.rename(&f.path(), &dir.join(f.file_name())) {
            h.log(&format!("restoring {:?}: {e}", f.file_name()));
        }
    }
}

/// Moves the old files aside and the new ones in. Returns what was done, for undoing. On Err it has tried to undo it, and the
/// flag says whether every file went back.
fn swap_in<H: Host>(h: &H, dir: &Path, names: &[String], removed: &[&str]) -> Result<Vec<String>, (io::Error, bool)> {
    let (stage, backup) = (dir.join(STAGE), dir.join(BACKUP));
    std::fs::create_dir_all(&backup).map_err(|e| (e, true))?;
    let mut done = Vec::new();
    let mut step = || -> io::Result<()> {
        for name in removed {
            if dir.join(name).exists() {
                h.rename(&dir.join(name), &backup.join(name))?;
            }
        }
        for name in names {
            done.push(name.clone());
            if dir.join(name).exists() {
                h.rename(&dir.join(name), &backup.join(name))?;
            }
            h.rename(&stage.join(name), &dir.join(name))?;
        }
        Ok(())
    };
    match step() {
        Ok(()) => Ok(done),
        Err(e) => {
            let whole = swap_out(h, dir, &done);
            Err((e, whole))
        }
    }
}

/// The new files back to the stage, every saved one back in place. True if nothing is left to restore.
fn swap_out<H: Host>(h: &H, dir: &Path, done: &[String]) -> bool {
    let (stage, backup) = (dir.join(STAGE), dir.join(BACKUP));
    for name in done.iter().rev() {
        if !stage.join(name).exists() && dir.join(name).exists() {
            let _ = h.rename(&dir.join(name), &stage.join(name));
        }
    }
    let mut whole = true;
    if let Ok(list) = std::fs::read_dir(&backup) {
        for f in list.flatten() {
            if h.rename(&f.path(), &dir.join(f.file_name())).is_err() {
                whole = false;
            }
        }
    }
    let _ = std::fs::remove_dir(&backup);
    whole
}

/// Named files only, then the empty folders.
fn tidy(dir: &Path) {
    let (stage, backup) = (dir.join(STAGE), dir.join(BACKUP));
    if let Ok(list) = std::fs::read_dir(&backup) {
        for f in list.flatten() {
            // A scanner may hold a file for a moment. One left behind does no harm: the next installer replaces it.
            for _ in 0..20 {
                if std::fs::remove_file(f.path()).is_ok() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        }
    }
    let _ = std::fs::remove_dir(&backup);
    let _ = std::fs::remove_file(stage.join(MANIFEST));
    let _ = std::fs::remove_dir(&stage);
}

fn back_in_service<H: Host>(h: &H, dir: &Path, was_installed: bool) -> bool {
    if !was_installed {
        return true;
    }
    let exe: PathBuf = dir.join(HELPER_EXE);
    exe.exists() && h.create_service(&exe) && h.start_service()
}

/// Install or upgrade from `<dir>/.update-new`; the same path for both, so an upgrade is never a special case.
pub fn install<H: Host>(h: &H, dir: &Path, version: &str) -> Result<(), Fail> {
    let Some(entries) = read_stage(dir) else {
        h.log("the new files are incomplete or damaged; nothing was changed");
        return Err(Fail::BadStage);
    };
    let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
    let was_installed = h.service_installed();
    h.close_app();
    if was_installed {
        // Failing to ask is not fatal: the service may simply be stopped already. The filters are checked below.
        let asked = h.helper_off();
        h.log(if asked { "the running helper turned the VPN off" } else { "the running helper did not answer" });
        h.forbid_restart();
        if !h.stop_service() {
            h.log("the old service did not stop");
            back_in_service(h, dir, true);
            return Err(Fail::ServiceStuck);
        }
    }
    recover_interrupted(h, dir);
    if let Err(fail) = clear_filters(h) {
        back_in_service(h, dir, was_installed);
        return Err(fail);
    }
    let check: Vec<String> = names.iter().cloned().chain(LEGACY.iter().map(|s| s.to_string())).collect();
    if let Some(name) = first_locked(h, dir, &check) {
        h.log(&format!("{name} is in use"));
        back_in_service(h, dir, was_installed);
        return Err(Fail::FileLocked);
    }
    if was_installed && !h.delete_service() {
        h.log("Windows has not finished removing the old service");
        // Refused rather than pending: the old service still exists and can go back to work. Pending: this changes nothing.
        back_in_service(h, dir, true);
        return Err(Fail::ServiceDeletePending);
    }
    let removed: Vec<&str> = LEGACY.iter().filter(|l| !names.iter().any(|n| n == *l)).copied().collect();
    let done = match swap_in(h, dir, &names, &removed) {
        Ok(done) => done,
        Err((e, whole)) => {
            h.log(&format!("replacing files: {e}"));
            // Files that did not go back stay in the backup folder, and the next installer starts by restoring them. Until
            // then no service runs from a mix: the app would refuse it anyway, by version.
            return Err(if whole && back_in_service(h, dir, was_installed) { Fail::RolledBack } else { Fail::RollbackFailed });
        }
    };
    let started = h.create_service(&dir.join(HELPER_EXE)) && h.start_service();
    let answered = started && h.running_version().as_deref() == Some(version);
    if answered && matches!(h.filters_present(), Ok(false)) {
        tidy(dir);
        h.log("installed and answering");
        return Ok(());
    }
    h.log("the new service did not come up; restoring the previous version");
    h.forbid_restart();
    if !h.stop_service() {
        // It still runs from these files. Moving them now would leave old files under a new process: keep one whole version.
        h.log("the new service does not stop either; the new files stay, whole");
        return Err(Fail::RollbackFailed);
    }
    let deleted = !h.service_installed() || h.delete_service();
    let whole = swap_out(h, dir, &done);
    if deleted && whole && back_in_service(h, dir, was_installed) {
        Err(Fail::RolledBack)
    } else {
        Err(Fail::RollbackFailed)
    }
}

/// Uninstall: after this nothing of MARZ VPN runs or blocks. Files are the uninstaller's to delete.
pub fn remove<H: Host>(h: &H, dir: &Path) -> Result<(), Fail> {
    h.close_app();
    if h.service_installed() {
        h.helper_off();
        h.forbid_restart();
        if !h.stop_service() {
            return Err(Fail::ServiceStuck);
        }
    }
    clear_filters(h)?;
    if h.service_installed() && !h.delete_service() {
        return Err(Fail::ServiceDeletePending);
    }
    let names: Vec<String> = [HELPER_EXE, "marzvpn.exe", "marzvpn-engine.exe", "wintun.dll"].iter().map(|s| s.to_string()).collect();
    // The program running this is the helper file itself: it has to be left out of the check.
    let others: Vec<String> = names.into_iter().filter(|n| n != HELPER_EXE).collect();
    if let Some(name) = first_locked(h, dir, &others) {
        h.log(&format!("{name} is in use"));
        return Err(Fail::FileLocked);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::HashSet;

    /// The Windows side as it behaves. The old helper's bug is modelled exactly: its process ends without ever reporting
    /// STOPPED, so the service manager counts a crash and runs the recovery actions unless they were cleared first.
    struct Win {
        installed: Cell<bool>,
        running: Cell<bool>,
        reports_stopped: bool,
        restart_on_crash: Cell<bool>,
        filters: Cell<bool>,
        filters_stick: bool,
        filters_unknown: bool,
        version_running: RefCell<Option<String>>,
        new_version: String,
        new_starts: bool,
        stop_hangs: bool,
        delete_pending: bool,
        delete_refused: bool,
        new_stop_hangs: bool,
        /// Every rename after this many fails.
        renames_left: Cell<Option<u32>>,
        locked: RefCell<HashSet<String>>,
        rename_fails: RefCell<HashSet<String>>,
        creates: RefCell<Vec<PathBuf>>,
        order: RefCell<Vec<String>>,
        dir: PathBuf,
    }

    impl Win {
        fn new(dir: &Path) -> Self {
            Win {
                installed: Cell::new(true),
                running: Cell::new(true),
                reports_stopped: false,
                restart_on_crash: Cell::new(true),
                filters: Cell::new(false),
                filters_stick: false,
                filters_unknown: false,
                version_running: RefCell::new(Some("old".into())),
                new_version: "new".into(),
                new_starts: true,
                stop_hangs: false,
                delete_pending: false,
                delete_refused: false,
                new_stop_hangs: false,
                renames_left: Cell::new(None),
                locked: RefCell::new(HashSet::new()),
                rename_fails: RefCell::new(HashSet::new()),
                creates: RefCell::new(vec![]),
                order: RefCell::new(vec![]),
                dir: dir.to_path_buf(),
            }
        }
        fn note(&self, s: &str) {
            self.order.borrow_mut().push(s.to_string());
        }
        fn at(&self, s: &str) -> usize {
            self.order.borrow().iter().position(|x| x == s).unwrap_or_else(|| panic!("{s} not in {:?}", self.order.borrow()))
        }
    }

    impl Host for Win {
        fn log(&self, _: &str) {}
        fn close_app(&self) {
            self.note("close_app");
        }
        fn helper_off(&self) -> bool {
            self.note("helper_off");
            if self.running.get() {
                self.filters.set(self.filters_stick);
            }
            self.running.get()
        }
        fn service_installed(&self) -> bool {
            self.installed.get()
        }
        fn forbid_restart(&self) {
            self.note("forbid_restart");
            self.restart_on_crash.set(false);
        }
        fn stop_service(&self) -> bool {
            self.note("stop_service");
            let new_running = self.running.get() && self.version_running.borrow().as_deref() == Some(self.new_version.as_str());
            if self.stop_hangs || (self.new_stop_hangs && new_running) {
                return false;
            }
            self.running.set(!self.reports_stopped && self.restart_on_crash.get());
            !self.running.get()
        }
        fn recover_network(&self) -> Result<(), &'static str> {
            self.note("recover_network");
            assert!(!self.running.get(), "a running helper would put the block back");
            if self.filters_stick {
                return Err("killswitch_failed");
            }
            self.filters.set(false);
            Ok(())
        }
        fn filters_present(&self) -> Result<bool, &'static str> {
            if self.filters_unknown {
                return Err("killswitch_failed");
            }
            Ok(self.filters.get())
        }
        fn file_locked(&self, path: &Path) -> bool {
            let name = path.file_name().unwrap().to_str().unwrap();
            (self.running.get() && name == HELPER_EXE) || self.locked.borrow().contains(name)
        }
        fn delete_service(&self) -> bool {
            self.note("delete_service");
            if self.delete_pending || self.delete_refused {
                return false;
            }
            assert!(!self.running.get());
            self.installed.set(false);
            true
        }
        fn create_service(&self, exe: &Path) -> bool {
            self.note("create_service");
            if self.delete_pending {
                return false; // marked for deletion: Windows refuses any change until it is gone
            }
            self.creates.borrow_mut().push(exe.to_path_buf());
            self.installed.set(true);
            self.restart_on_crash.set(true);
            true
        }
        fn start_service(&self) -> bool {
            self.note("start_service");
            let content = std::fs::read_to_string(self.dir.join(HELPER_EXE)).unwrap_or_default();
            if content == "new-helper" && !self.new_starts {
                return false;
            }
            self.running.set(true);
            *self.version_running.borrow_mut() = Some(if content == "new-helper" { self.new_version.clone() } else { "old".into() });
            true
        }
        fn running_version(&self) -> Option<String> {
            self.version_running.borrow().clone().filter(|_| self.running.get())
        }
        fn wait(&self, _: Duration) {}
        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            let name = to.file_name().unwrap().to_str().unwrap();
            if let Some(left) = self.renames_left.get() {
                if left == 0 {
                    return Err(io::Error::other("held by a scanner"));
                }
                self.renames_left.set(Some(left - 1));
            }
            if from.file_name().unwrap() == HELPER_EXE && self.running.get() && from.parent() == Some(self.dir.as_path()) {
                panic!("replacing the helper while it runs");
            }
            if self.rename_fails.borrow().contains(name) {
                return Err(io::Error::other("denied"));
            }
            std::fs::rename(from, to)
        }
    }

    struct Dir(PathBuf);
    impl Drop for Dir {
        fn drop(&mut self) {
            for sub in [STAGE, BACKUP] {
                if let Ok(l) = std::fs::read_dir(self.0.join(sub)) {
                    l.flatten().for_each(|f| drop(std::fs::remove_file(f.path())));
                }
                let _ = std::fs::remove_dir(self.0.join(sub));
            }
            if let Ok(l) = std::fs::read_dir(&self.0) {
                l.flatten().for_each(|f| drop(std::fs::remove_file(f.path())));
            }
            let _ = std::fs::remove_dir(&self.0);
        }
    }

    fn setup(tag: &str) -> Dir {
        let dir = std::env::temp_dir().join(format!("marz-install-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(dir.join(STAGE)).unwrap();
        let mut manifest = String::new();
        for (name, body) in [(HELPER_EXE, "new-helper"), ("marzvpn.exe", "new-app"), ("marzvpn-engine.exe", "new-engine")] {
            let p = dir.join(STAGE).join(name);
            std::fs::write(&p, body).unwrap();
            manifest += &format!("{}  {}  {name}\n", sha256_file(&p).unwrap(), body.len());
            std::fs::write(dir.join(name), body.replace("new", "old")).unwrap();
        }
        std::fs::write(dir.join("sing-box.exe"), "old-legacy").unwrap();
        std::fs::write(dir.join(STAGE).join(MANIFEST), manifest).unwrap();
        Dir(dir)
    }

    fn read(d: &Dir, name: &str) -> String {
        std::fs::read_to_string(d.0.join(name)).unwrap()
    }

    fn all_old(d: &Dir) -> bool {
        read(d, HELPER_EXE) == "old-helper" && read(d, "marzvpn.exe") == "old-app" && read(d, "marzvpn-engine.exe") == "old-engine"
    }

    #[test]
    fn upgrade_replaces_a_running_helper_that_never_reports_stopped() {
        let d = setup("running");
        let w = Win::new(&d.0);
        assert_eq!(install(&w, &d.0, "new"), Ok(()));
        assert_eq!(read(&d, HELPER_EXE), "new-helper");
        assert_eq!(read(&d, "marzvpn-engine.exe"), "new-engine");
        assert!(w.running.get() && w.restart_on_crash.get(), "new service running, crash recovery back on");
        assert!(!d.0.join(BACKUP).exists() && !d.0.join(STAGE).exists() && !d.0.join("sing-box.exe").exists());
        assert!(w.at("close_app") < w.at("helper_off") && w.at("helper_off") < w.at("forbid_restart"));
        assert!(w.at("forbid_restart") < w.at("stop_service") && w.at("stop_service") < w.at("recover_network"));
        assert!(w.at("recover_network") < w.at("delete_service") && w.at("delete_service") < w.at("create_service"));
    }

    #[test]
    fn without_clearing_recovery_the_old_order_really_fails() {
        // Documents the failure in the field: the helper "comes back" and holds its file.
        let d = setup("field");
        let w = Win::new(&d.0);
        assert!(!w.stop_service(), "a stop without clearing the recovery actions leaves it running");
        assert!(w.file_locked(&d.0.join(HELPER_EXE)));
    }

    #[test]
    fn fresh_install_needs_no_old_service() {
        let d = setup("fresh");
        for f in [HELPER_EXE, "marzvpn.exe", "marzvpn-engine.exe", "sing-box.exe"] {
            std::fs::remove_file(d.0.join(f)).unwrap();
        }
        let w = Win::new(&d.0);
        w.installed.set(false);
        w.running.set(false);
        assert_eq!(install(&w, &d.0, "new"), Ok(()));
        assert_eq!(read(&d, HELPER_EXE), "new-helper");
        assert!(w.running.get());
    }

    #[test]
    fn repeating_the_same_version_works() {
        let d = setup("same");
        let w = Win::new(&d.0);
        *w.version_running.borrow_mut() = Some("new".into());
        assert_eq!(install(&w, &d.0, "new"), Ok(()));
    }

    #[test]
    fn damaged_stage_changes_nothing() {
        let d = setup("damaged");
        std::fs::write(d.0.join(STAGE).join("marzvpn.exe"), "new-apX").unwrap();
        let w = Win::new(&d.0);
        assert_eq!(install(&w, &d.0, "new"), Err(Fail::BadStage));
        assert!(all_old(&d));
        assert!(w.order.borrow().is_empty(), "not even the app was closed");
        assert!(w.running.get());
    }

    #[test]
    fn missing_stage_file_and_bad_manifests_are_refused() {
        let d = setup("missing");
        std::fs::remove_file(d.0.join(STAGE).join("marzvpn.exe")).unwrap();
        assert!(read_stage(&d.0).is_none());
        let sha = "a".repeat(64);
        assert!(parse_manifest(&format!("{sha}  1  ..\\evil.exe\n{sha}  1  {HELPER_EXE}\n")).is_none());
        assert!(parse_manifest(&format!("{sha}  1  ../evil\n{sha}  1  {HELPER_EXE}\n")).is_none());
        assert!(parse_manifest(&format!("{sha}  1  C:evil\n{sha}  1  {HELPER_EXE}\n")).is_none());
        assert!(parse_manifest(&format!("{sha}  1  other.exe\n")).is_none(), "a manifest without the helper is not an install");
        assert!(parse_manifest("").is_none());
        assert!(parse_manifest(&format!("{sha}  1  {HELPER_EXE}\n")).is_some());
    }

    #[test]
    fn a_service_that_will_not_stop_aborts_with_old_files() {
        let d = setup("stuck");
        let mut w = Win::new(&d.0);
        w.stop_hangs = true;
        assert_eq!(install(&w, &d.0, "new"), Err(Fail::ServiceStuck));
        assert!(all_old(&d));
        assert!(w.creates.borrow().iter().all(|p| p == &d.0.join(HELPER_EXE)));
        assert!(w.restart_on_crash.get(), "recovery actions are back");
    }

    #[test]
    fn a_kill_switch_block_is_removed_and_verified_before_anything_is_replaced() {
        let d = setup("armed");
        let w = Win::new(&d.0);
        w.filters.set(true);
        *w.version_running.borrow_mut() = None; // the old helper does not answer: the filters still go
        w.running.set(false);
        assert_eq!(install(&w, &d.0, "new"), Ok(()));
        assert!(!w.filters.get());
    }

    #[test]
    fn filters_that_stay_abort_before_anything_is_replaced() {
        let d = setup("filters");
        let mut w = Win::new(&d.0);
        w.filters.set(true);
        w.filters_stick = true;
        assert_eq!(install(&w, &d.0, "new"), Err(Fail::FiltersLeft));
        assert!(all_old(&d));
        assert!(w.order.borrow().iter().all(|s| s != "delete_service"));
    }

    #[test]
    fn unknown_filter_state_is_not_treated_as_clean() {
        let d = setup("unknown");
        let mut w = Win::new(&d.0);
        w.filters_unknown = true;
        assert_eq!(install(&w, &d.0, "new"), Err(Fail::FiltersLeft));
        assert!(all_old(&d));
    }

    #[test]
    fn a_locked_file_aborts_with_the_old_version_back_in_service() {
        let d = setup("locked");
        let w = Win::new(&d.0);
        w.locked.borrow_mut().insert("marzvpn-engine.exe".into());
        assert_eq!(install(&w, &d.0, "new"), Err(Fail::FileLocked));
        assert!(all_old(&d));
        assert!(w.running.get());
        assert_eq!(w.version_running.borrow().as_deref(), Some("old"));
    }

    #[test]
    fn a_service_stuck_in_deletion_replaces_nothing() {
        let d = setup("pending");
        let mut w = Win::new(&d.0);
        w.delete_pending = true;
        assert_eq!(install(&w, &d.0, "new"), Err(Fail::ServiceDeletePending));
        assert!(all_old(&d));
    }

    #[test]
    fn a_refused_deletion_puts_the_old_service_back_to_work() {
        let d = setup("refused");
        let mut w = Win::new(&d.0);
        w.delete_refused = true;
        assert_eq!(install(&w, &d.0, "new"), Err(Fail::ServiceDeletePending));
        assert!(all_old(&d));
        assert!(w.running.get() && w.restart_on_crash.get());
    }

    #[test]
    fn a_replacement_that_cannot_be_undone_says_so_and_the_next_installer_repairs_it() {
        let d = setup("noundo");
        let w = Win::new(&d.0);
        w.renames_left.set(Some(3)); // the legacy file, the old helper aside, the new helper in; then everything fails
        assert_eq!(install(&w, &d.0, "new"), Err(Fail::RollbackFailed), "never reported as restored");
        assert!(d.0.join(BACKUP).join("sing-box.exe").exists() || d.0.join(BACKUP).join(HELPER_EXE).exists(), "the old files are kept");
        assert!(!w.running.get(), "no service runs from a mix");
        w.renames_left.set(None);
        // The next installer writes its stage anew before anything else.
        std::fs::write(d.0.join(STAGE).join(HELPER_EXE), "new-helper").unwrap();
        assert_eq!(install(&w, &d.0, "new"), Ok(()));
        assert_eq!(read(&d, HELPER_EXE), "new-helper");
        assert_eq!(read(&d, "marzvpn.exe"), "new-app");
        assert_eq!(read(&d, "marzvpn-engine.exe"), "new-engine");
        assert!(!d.0.join(BACKUP).exists());
    }

    #[test]
    fn a_new_service_that_will_not_stop_keeps_one_whole_new_version() {
        let d = setup("newstuck");
        let mut w = Win::new(&d.0);
        w.new_stop_hangs = true;
        w.new_version = "other".into(); // answers, but not as expected: a rollback is needed
        assert_eq!(install(&w, &d.0, "new"), Err(Fail::RollbackFailed));
        assert_eq!(read(&d, HELPER_EXE), "new-helper");
        assert_eq!(read(&d, "marzvpn.exe"), "new-app");
        assert_eq!(read(&d, "marzvpn-engine.exe"), "new-engine");
    }

    #[test]
    fn a_failed_replacement_midway_restores_every_file() {
        let d = setup("swap");
        let w = Win::new(&d.0);
        // The rename into place of the last file fails, after the others were already replaced.
        std::fs::remove_file(d.0.join("marzvpn-engine.exe")).unwrap();
        w.rename_fails.borrow_mut().insert("marzvpn-engine.exe".into());
        assert_eq!(install(&w, &d.0, "new"), Err(Fail::RolledBack));
        assert_eq!(read(&d, HELPER_EXE), "old-helper");
        assert_eq!(read(&d, "marzvpn.exe"), "old-app");
        assert!(w.running.get());
        assert_eq!(w.version_running.borrow().as_deref(), Some("old"));
    }

    #[test]
    fn a_new_service_that_does_not_start_rolls_back() {
        let d = setup("nostart");
        let mut w = Win::new(&d.0);
        w.new_starts = false;
        assert_eq!(install(&w, &d.0, "new"), Err(Fail::RolledBack));
        assert!(all_old(&d));
        assert!(d.0.join("sing-box.exe").exists(), "the legacy file returned too");
        assert!(w.running.get());
        assert_eq!(w.version_running.borrow().as_deref(), Some("old"));
        assert!(!d.0.join(BACKUP).exists());
    }

    #[test]
    fn a_new_service_of_the_wrong_version_rolls_back() {
        let d = setup("wrong");
        let mut w = Win::new(&d.0);
        w.new_version = "older".into();
        assert_eq!(install(&w, &d.0, "new"), Err(Fail::RolledBack));
        assert!(all_old(&d));
    }

    #[test]
    fn an_interrupted_upgrade_is_finished_by_the_next_installer() {
        let d = setup("power");
        std::fs::create_dir_all(d.0.join(BACKUP)).unwrap();
        // Power cut: helper already replaced, the old one aside; no matching engine.
        std::fs::write(d.0.join(BACKUP).join(HELPER_EXE), "old-helper").unwrap();
        std::fs::write(d.0.join(HELPER_EXE), "half").unwrap();
        let w = Win::new(&d.0);
        assert_eq!(install(&w, &d.0, "new"), Ok(()));
        assert_eq!(read(&d, HELPER_EXE), "new-helper");
        assert!(!d.0.join(BACKUP).exists());
    }

    #[test]
    fn user_data_is_outside_everything_this_touches() {
        // Sign-in, subscription and settings live in the user's %APPDATA%\MARZ VPN and the vault, never in the install folder.
        let d = setup("data");
        std::fs::write(d.0.join("unrelated.txt"), "keep").unwrap();
        let w = Win::new(&d.0);
        assert_eq!(install(&w, &d.0, "new"), Ok(()));
        assert_eq!(read(&d, "unrelated.txt"), "keep");
    }

    #[test]
    fn removal_leaves_nothing_running_or_blocking() {
        let d = setup("remove");
        let w = Win::new(&d.0);
        w.filters.set(true);
        assert_eq!(remove(&w, &d.0), Ok(()));
        assert!(!w.running.get() && !w.filters.get() && !w.installed.get());
    }

    #[test]
    fn removal_reports_a_service_it_could_not_stop() {
        let d = setup("remove-stuck");
        let mut w = Win::new(&d.0);
        w.stop_hangs = true;
        assert_eq!(remove(&w, &d.0), Err(Fail::ServiceStuck));
        assert!(w.installed.get());
    }
}
