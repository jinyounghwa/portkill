pub mod models;
pub mod process_info;

pub use models::{PortEntry, Protocol, Scanner, SocketState};
pub use process_info::ProcessInfo;

pub struct Killer;

impl Killer {
    pub fn kill(pid: u32, signal: nix::sys::signal::Signal) -> Result<String, String> {
        use nix::sys::signal;
        use nix::unistd::Pid;

        // Get process name for better error messages
        let process_name = Self::get_process_name(pid);

        // Use nix::sys::signal::kill directly
        match signal::kill(Pid::from_raw(pid as i32), signal) {
            Ok(_) => Ok(format!(
                "Successfully sent {} to {} (PID {})",
                signal, process_name, pid
            )),
            Err(e) => {
                if e == nix::errno::Errno::EPERM {
                    Err(format!(
                        "Permission denied: Use `sudo portkill` to kill {} (PID {})",
                        process_name, pid
                    ))
                } else if e == nix::errno::Errno::ESRCH {
                    Err(format!("Process {} (PID {}) not found", process_name, pid))
                } else {
                    Err(format!("Failed to kill {} (PID {}): {}", process_name, pid, e))
                }
            }
        }
    }

    fn get_process_name(pid: u32) -> String {
        // Try to get process name from different sources
        #[cfg(target_os = "linux")]
        {
            if let Ok(contents) = std::fs::read_to_string(format!("/proc/{}/cmdline", pid)) {
                let name = contents.replace('\0', " ").trim().to_string();
                if !name.is_empty() {
                    return name;
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            if let Ok(output) = std::process::Command::new("ps")
                .args(&["-p", &pid.to_string(), "-o", "comm="])
                .output()
            {
                let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !name.is_empty() {
                    return name;
                }
            }
        }

        format!("PID {}", pid)
    }

    pub fn kill_sigterm(pid: u32) -> Result<String, String> {
        Self::kill(pid, nix::sys::signal::Signal::SIGTERM)
    }

    pub fn kill_sigkill(pid: u32) -> Result<String, String> {
        Self::kill(pid, nix::sys::signal::Signal::SIGKILL)
    }

    pub fn can_kill(pid: u32) -> bool {
        // Never kill PID 1
        if pid == 1 {
            return false;
        }

        // Check if it's a system process
        #[cfg(target_os = "linux")]
        {
            if let Ok(contents) = std::fs::read_to_string(format!("/proc/{}/status", pid)) {
                if contents.contains("Name:\tsystemd") || contents.contains("Name:\tinit") {
                    return false;
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            // On macOS, check for critical system processes
            if let Ok(output) = std::process::Command::new("ps")
                .args(&["-p", &pid.to_string(), "-o", "comm="])
                .output()
            {
                let name = String::from_utf8_lossy(&output.stdout).to_lowercase();
                if name.contains("kernel") || name.contains("launchd") {
                    return false;
                }
            }
        }

        true
    }
}
