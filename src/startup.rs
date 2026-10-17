//! A safety net for graphics drivers that can't run the default renderer.
//!
//! Some Windows and Linux machines (old GPUs, broken drivers) fail while the window is being
//! set up, or crash outright inside the driver, so the app would vanish before showing anything.
//! Each launch is marked as "starting" until a few frames have painted. If a launch dies before
//! that, the next one steps down to the next renderer and remembers it:
//!
//! 1. default: wgpu's choice (Vulkan or DirectX 12 on Windows, Vulkan on Linux)
//! 2. DirectX 12 on the GPU (Windows only; skips a broken Vulkan driver)
//! 3. OpenGL on the GPU (works with nearly any driver, including old ones)
//! 4. software: the CPU (WARP on Windows, llvmpipe on Linux), slow but always there
//!
//! A setup error (rather than a crash) relaunches at the next step straight away.

use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew};
use eframe::wgpu::{self, Backend, Backends, DeviceType};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Renderer {
    Default,
    Dx12,
    OpenGl,
    Software,
}

impl Renderer {
    const ALL: [Renderer; 4] = [Renderer::Default, Renderer::Dx12, Renderer::OpenGl, Renderer::Software];

    /// The choices offered in Settings on this platform.
    pub fn choices() -> Vec<Renderer> {
        Renderer::ALL.into_iter().filter(|r| *r != Renderer::Dx12 || cfg!(windows)).collect()
    }

    pub fn label(self) -> &'static str {
        match self {
            Renderer::Default => "Automatic",
            Renderer::Dx12 => "DirectX 12",
            Renderer::OpenGl => "OpenGL",
            Renderer::Software => "Software (CPU)",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Renderer::Default => "default",
            Renderer::Dx12 => "dx12",
            Renderer::OpenGl => "opengl",
            Renderer::Software => "software",
        }
    }

    fn parse(s: &str) -> Option<Renderer> {
        Renderer::ALL.into_iter().find(|r| r.name() == s.trim())
    }

    /// The next renderer to try after this one failed, if any.
    fn next(self) -> Option<Renderer> {
        let i = Renderer::ALL.iter().position(|r| *r == self)?;
        Renderer::ALL[i + 1..].iter().copied().find(|r| *r != Renderer::Dx12 || cfg!(windows))
    }

    /// The wgpu setup for this renderer (`None`: eframe's default).
    pub fn wgpu_setup(self) -> Option<WgpuSetup> {
        let (backends, want): (Backends, fn(&wgpu::AdapterInfo) -> bool) = match self {
            Renderer::Default => return None,
            Renderer::Dx12 => (Backends::DX12, |i| i.device_type != DeviceType::Cpu),
            Renderer::OpenGl => (Backends::GL, |i| i.device_type != DeviceType::Cpu),
            Renderer::Software => (Backends::all(), |i| i.device_type == DeviceType::Cpu),
        };
        let mut setup = WgpuSetupCreateNew::without_display_handle();
        setup.instance_descriptor.backends = backends;
        setup.native_adapter_selector = Some(Arc::new(move |adapters: &[wgpu::Adapter], _surface: Option<&wgpu::Surface<'_>>| {
            let info = |a: &wgpu::Adapter| a.get_info();
            adapters
                .iter()
                .find(|a| want(&info(a)))
                // Software mode with no CPU adapter listed: GL is the next most forgiving.
                .or_else(|| adapters.iter().find(|a| info(a).backend == Backend::Gl))
                .or_else(|| adapters.first())
                .cloned()
                .ok_or_else(|| "no graphics adapter found".to_string())
        }));
        Some(WgpuSetup::CreateNew(setup))
    }
}

/// Set (to a renderer name) on a relaunch, and for testing a renderer by hand.
const RENDERER_VAR: &str = "DRROCKET_RENDERER";

fn starting_marker() -> PathBuf {
    crate::config::config_dir().join("starting")
}

/// The renderer that last worked, once the default has failed; delete it to start over.
fn renderer_file() -> PathBuf {
    crate::config::config_dir().join("renderer")
}

fn remember(r: Renderer) {
    let _ = std::fs::write(renderer_file(), format!("{}\n# Dr. Rocket's renderer on this machine (the default failed). Delete this file to try the default again.\n", r.name()));
}

fn stored() -> Renderer {
    std::fs::read_to_string(renderer_file()).ok().and_then(|s| s.lines().next().and_then(Renderer::parse)).unwrap_or(Renderer::Default)
}

/// The renderer for this launch, and records the launch as starting.
pub fn begin() -> Renderer {
    let forced = std::env::var(RENDERER_VAR).ok().as_deref().and_then(Renderer::parse);
    if cfg!(target_os = "macos") && forced.is_none() {
        return Renderer::Default;
    }
    let _ = std::fs::create_dir_all(crate::config::config_dir());
    // 1.0.0-beta.2 went straight to software after a failed launch: start that machine one
    // step down from the default instead, so it gets its GPU back.
    let old = crate::config::config_dir().join("software-rendering");
    if old.exists() {
        let _ = std::fs::remove_file(old);
        if !renderer_file().exists() {
            remember(Renderer::Default.next().unwrap_or(Renderer::Software));
        }
    }
    let r = forced.unwrap_or_else(|| {
        let r = stored();
        // The previous launch never painted: its renderer is what killed it.
        match r.next() {
            Some(next) if starting_marker().exists() => {
                remember(next);
                next
            }
            _ => r,
        }
    });
    // Also on a relaunch (`forced`): if this one dies too, the next launch steps down from it.
    let _ = std::fs::write(starting_marker(), b"");
    let _ = ACTIVE.set(r);
    r
}

static ACTIVE: OnceLock<Renderer> = OnceLock::new();
static ADAPTER: OnceLock<String> = OnceLock::new();

/// The renderer this launch uses.
pub fn active() -> Renderer {
    ACTIVE.get().copied().unwrap_or(Renderer::Default)
}

/// The renderer the next launch will use (what Settings shows as chosen).
pub fn chosen() -> Renderer {
    std::env::var(RENDERER_VAR).ok().as_deref().and_then(Renderer::parse).unwrap_or_else(stored)
}

/// Pick the renderer for the next launch (`Default` goes back to automatic).
pub fn choose(r: Renderer) {
    if r == Renderer::Default {
        let _ = std::fs::remove_file(renderer_file());
    } else {
        remember(r);
    }
}

/// Record the graphics adapter in use, for Settings and problem reports.
pub fn set_adapter(info: &wgpu::AdapterInfo) {
    let kind = match info.device_type {
        DeviceType::Cpu => "CPU",
        DeviceType::IntegratedGpu => "integrated GPU",
        DeviceType::DiscreteGpu => "GPU",
        _ => "GPU",
    };
    let api = match info.backend {
        Backend::Dx12 => "DirectX 12",
        Backend::Vulkan => "Vulkan",
        Backend::Gl => "OpenGL",
        Backend::Metal => "Metal",
        _ => "an unknown API",
    };
    let _ = ADAPTER.set(format!("{} through {api} ({kind})", info.name));
}

/// "Intel(R) UHD Graphics 620 through DirectX 12 (integrated GPU)", once known.
pub fn adapter() -> Option<&'static str> {
    ADAPTER.get().map(String::as_str)
}

/// Start a fresh copy of the app (used after the app closes to apply a new renderer).
pub fn spawn_again() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).spawn();
    }
}

/// Called every frame; after a few painted frames the launch counts as a success.
pub fn frame_painted(frame_nr: u64) {
    static DONE: AtomicBool = AtomicBool::new(false);
    if frame_nr >= 3 && !DONE.swap(true, Ordering::Relaxed) {
        let _ = std::fs::remove_file(starting_marker());
    }
}

/// The window couldn't be created with `failed`: start again with the next renderer.
/// Returns false when there is none left (or the relaunch failed).
pub fn relaunch_after(failed: Renderer) -> bool {
    if cfg!(target_os = "macos") {
        return false;
    }
    let Some(next) = failed.next() else { return false };
    remember(next);
    let _ = std::fs::remove_file(starting_marker());
    let Ok(exe) = std::env::current_exe() else { return false };
    std::process::Command::new(exe).args(std::env::args_os().skip(1)).env(RENDERER_VAR, next.name()).spawn().is_ok()
}

#[cfg(test)]
mod tests {
    use super::Renderer;

    #[test]
    fn renderers_step_down_to_software_and_stop() {
        let mut r = Renderer::Default;
        let mut seen = vec![r];
        while let Some(n) = r.next() {
            r = n;
            seen.push(r);
        }
        assert_eq!(seen.last(), Some(&Renderer::Software));
        assert_eq!(seen.contains(&Renderer::Dx12), cfg!(windows));
        assert!(seen.contains(&Renderer::OpenGl));
        assert_eq!(Renderer::parse("opengl\n"), Some(Renderer::OpenGl));
    }
}
