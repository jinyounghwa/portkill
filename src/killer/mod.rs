use std::process::Command;

use crate::scanner::PortEntry;

pub struct Killer;

impl Killer {
    pub fn kill(pid: u32, signal: nix::sys::signal::Signal) -> Result<String, String> {
        let process_name =
            if let Ok(contents) = std::fs::read_to_string(format!("/proc/{}/cmdline", pid)) {
                contents.replace('\0', " ").trim().to_string()
            } else {
                format!("PID {}", pid)
            };

        match Command::new("kill")
            .arg("-")
            .arg(pid.to_string())
            .arg(signal.as_str())
            .status()
        {
            Ok(status) if status.success() => {
                Ok(format!("Process {} killed successfully", process_name))
            }
            Ok(_) => Err(format!(
                "Failed to kill process {} (non-zero exit code)",
                process_name
            )),
            Err(e) => {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    Err(format!(
                        "Permission denied: Use `sudo portkill` to kill {} (PID {})",
                        process_name, pid
                    ))
                } else {
                    Err(format!("Failed to kill process {}: {}", process_name, e))
                }
            }
        }
    }

    pub fn kill_sigterm(pid: u32) -> Result<String, String> {
        Self::kill(pid, nix::sys::signal::Signal::SIGTERM)
    }

    pub fn kill_sigkill(pid: u32) -> Result<String, String> {
        Self::kill(pid, nix::sys::signal::Signal::SIGKILL)
    }

    pub fn can_kill(pid: u32) -> bool {
        if pid == 1 {
            return false;
        }

        if let Ok(contents) = std::fs::read_to_string(format!("/proc/{}/status", pid)) {
            if contents.contains("Name:\tsystemd") || contents.contains("Name:\tinit") {
                return false;
            }
        }

        true
    }
}
