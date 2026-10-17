//! A safety net for graphics drivers that can't run the GPU renderer.
//!
//! Some Windows and Linux machines (old GPUs, broken or missing drivers) fail while the window
//! is being set up, or crash outright inside the driver. Either way the app would vanish before
//! showing anything. So each launch is marked as "starting" until a few frames have painted;
//! if a launch dies before that, the next one uses the software renderer (WARP on Windows,
//! llvmpipe on Linux) and keeps using it. Setup errors relaunch in software mode at once.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set on a relaunch to force the software renderer.
const SOFTWARE_VAR: &str = "DRROCKET_SOFTWARE_RENDERING";

fn starting_marker() -> PathBuf {
    crate::config::config_dir().join("starting")
}

/// Present once the GPU renderer has failed on this machine; delete it to try the GPU again.
fn software_marker() -> PathBuf {
    crate::config::config_dir().join("software-rendering")
}

/// Whether this launch should render in software, and records the launch as starting.
pub fn begin() -> bool {
    if cfg!(target_os = "macos") {
        return false;
    }
    let dir = crate::config::config_dir();
    let _ = std::fs::create_dir_all(&dir);
    // The previous launch never painted: the GPU renderer is what killed it.
    if starting_marker().exists() && !software_marker().exists() {
        let _ = std::fs::write(software_marker(), "The GPU renderer failed to start on this machine. Delete this file to try it again.\n");
    }
    let _ = std::fs::write(starting_marker(), b"");
    std::env::var_os(SOFTWARE_VAR).is_some() || software_marker().exists()
}

/// Called every frame; after a few painted frames the launch counts as a success.
pub fn frame_painted(frame_nr: u64) {
    static DONE: AtomicBool = AtomicBool::new(false);
    if frame_nr >= 3 && !DONE.swap(true, Ordering::Relaxed) {
        let _ = std::fs::remove_file(starting_marker());
    }
}

/// The window couldn't be created with the GPU renderer: start again in software mode.
/// Returns false when this already was the software attempt (or the relaunch failed).
pub fn relaunch_in_software() -> bool {
    if cfg!(target_os = "macos") || std::env::var_os(SOFTWARE_VAR).is_some() {
        return false;
    }
    let _ = std::fs::write(software_marker(), "The GPU renderer failed to start on this machine. Delete this file to try it again.\n");
    let _ = std::fs::remove_file(starting_marker());
    let Ok(exe) = std::env::current_exe() else { return false };
    std::process::Command::new(exe).args(std::env::args_os().skip(1)).env(SOFTWARE_VAR, "1").spawn().is_ok()
}

/// Adapter choice for the software renderer: a CPU adapter if there is one, else whatever
/// the GL backend offers, else the first adapter.
pub fn software_adapter(adapters: &[eframe::wgpu::Adapter]) -> Result<eframe::wgpu::Adapter, String> {
    use eframe::wgpu::{Backend, DeviceType};
    let pick = |f: &dyn Fn(&eframe::wgpu::AdapterInfo) -> bool| adapters.iter().find(|a| f(&a.get_info())).cloned();
    pick(&|i| i.device_type == DeviceType::Cpu)
        .or_else(|| pick(&|i| i.backend == Backend::Gl))
        .or_else(|| adapters.first().cloned())
        .ok_or_else(|| "no graphics adapter found".to_string())
}
