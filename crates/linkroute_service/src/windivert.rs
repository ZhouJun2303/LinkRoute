pub mod runtime;
pub use runtime::WindivertRuntime;

use std::path::PathBuf;

use crate::backend::{BackendStatus, CaptureBackend};
use linkroute_core::{FlowDescriptor, LinkState, RouteAction, RouteConfig, RouteDecision};

pub struct WindivertBackend {
    running: bool,
    active_flows: usize,
}

impl WindivertBackend {
    pub fn new() -> Self {
        Self { running: false, active_flows: 0 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindivertProbe {
    pub search_dir: PathBuf,
    pub dll_present: bool,
    pub driver_present: bool,
}

impl WindivertProbe {
    pub fn probe_near_exe() -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let search_dir = exe
            .parent()
            .map(PathBuf::from)
            .ok_or_else(|| String::from("could not determine executable directory"))?;
        Ok(Self::probe_dir(search_dir))
    }

    pub fn probe_dir(search_dir: PathBuf) -> Self {
        Self {
            dll_present: search_dir.join("WinDivert.dll").is_file(),
            driver_present: search_dir.join("WinDivert64.sys").is_file(),
            search_dir,
        }
    }

    pub fn ready(&self) -> bool {
        self.dll_present && self.driver_present
    }

    pub fn missing_files(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if !self.dll_present {
            missing.push("WinDivert.dll");
        }
        if !self.driver_present {
            missing.push("WinDivert64.sys");
        }
        missing
    }
}

impl CaptureBackend for WindivertBackend {
    fn name(&self) -> &'static str {
        "windivert"
    }

    fn start(&mut self, _config: &RouteConfig) -> Result<(), String> {
        self.running = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), String> {
        self.running = false;
        self.active_flows = 0;
        Ok(())
    }

    fn status(&self) -> BackendStatus {
        BackendStatus { name: self.name(), running: self.running, active_flows: self.active_flows }
    }

    fn close_managed_flows(&mut self) -> Result<usize, String> {
        let closed = self.active_flows;
        self.active_flows = 0;
        Ok(closed)
    }

    fn apply_decision(
        &mut self,
        _flow: &FlowDescriptor,
        decision: &RouteDecision,
        _links: LinkState,
    ) {
        if self.running && !matches!(decision.action, RouteAction::UseSystemDefault) {
            self.active_flows = self.active_flows.saturating_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::WindivertProbe;

    #[test]
    fn probe_reports_missing_files() {
        let dir =
            std::env::temp_dir().join(format!("linkroute_probe_missing_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let probe = WindivertProbe::probe_dir(dir.clone());

        assert!(!probe.ready());
        assert_eq!(probe.missing_files(), vec!["WinDivert.dll", "WinDivert64.sys"]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn probe_accepts_required_files() {
        let dir =
            std::env::temp_dir().join(format!("linkroute_probe_ready_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("WinDivert.dll"), b"").unwrap();
        fs::write(dir.join("WinDivert64.sys"), b"").unwrap();

        let probe = WindivertProbe::probe_dir(dir.clone());

        assert!(probe.ready());
        assert!(probe.missing_files().is_empty());
        let _ = fs::remove_dir_all(dir);
    }
}
