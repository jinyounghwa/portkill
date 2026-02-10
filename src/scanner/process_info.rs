use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::PortEntry;

pub struct ProcessInfo;

impl ProcessInfo {
    pub fn map_pid_to_info(entry: &mut PortEntry) {
        if let Some(inode) = entry.inode {
            entry.pid = Self::find_pid_by_inode(inode);
            if let Some(pid) = entry.pid {
                Self::read_process_info(pid, entry);
            }
        }
    }

    fn find_pid_by_inode(inode: u32) -> Option<u32> {
        let proc_path = Path::new("/proc");

        for entry in fs::read_dir(proc_path).ok()? {
            let entry = entry.ok()?;
            let pid_str = entry.file_name().to_string_lossy().to_string();

            if let Ok(pid) = pid_str.parse::<u32>() {
                let pid_path = proc_path.join(&pid_str);

                if pid_path.exists() {
                    let net_path = pid_path.join("net/tcp");
                    if let Ok(file) = fs::File::open(&net_path) {
                        let reader = BufReader::new(file);

                        for line in reader.lines().skip(1) {
                            if let Ok(line) = line {
                                let fields: Vec<&str> = line.split_whitespace().collect();
                                if fields.len() < 2 {
                                    continue;
                                }

                                let inode_str = fields[9];

                                if let Some(entry_inode) = inode_str.parse::<u32>().ok() {
                                    if entry_inode == inode {
                                        return Some(pid);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        None
    }

    fn read_process_info(pid: u32, entry: &mut PortEntry) {
        Self::read_cmdline(pid, entry);
        Self::read_status(pid, entry);
        Self::read_user(pid, entry);
    }

    fn read_cmdline(pid: u32, entry: &mut PortEntry) {
        let cmdline_path = format!("/proc/{}/cmdline", pid);

        if let Ok(contents) = fs::read_to_string(&cmdline_path) {
            entry.cmdline = contents.replace('\0', " ");
            entry.process_name = Self::extract_process_name(&entry.cmdline);
        }
    }

    fn read_status(pid: u32, entry: &mut PortEntry) {
        let status_path = format!("/proc/{}/status", pid);

        if let Ok(contents) = fs::read_to_string(&status_path) {
            for line in contents.lines() {
                if let Some((key, value)) = line.split_once(':') {
                    let key = key.trim();
                    let value = value.trim();

                    if key == "Name" {
                        entry.process_name = value.to_string();
                    } else if key == "Uid" {
                        if let Some(uid_str) = value.split_whitespace().next() {
                            if let Ok(uid) = uid_str.parse::<u32>() {
                                entry.user = uid.to_string();
                            }
                        }
                    }
                }
            }
        }
    }

    fn read_user(pid: u32, entry: &mut PortEntry) {
        if entry.user.is_empty() {
            let uid = users::uid_t::from(pid);
            if let Some(name) = users::get_user_by_uid(uid) {
                entry.user = name.name().to_string_lossy().to_string();
            }
        }
    }

    fn extract_process_name(cmdline: &str) -> String {
        let parts: Vec<&str> = cmdline.split_whitespace().collect();
        if parts.is_empty() {
            return String::new();
        }

        let name = parts[0].to_string();
        if name.ends_with('/') {
            name.trim_end_matches('/').to_string()
        } else {
            name
        }
    }

    fn parse_port(addr_hex: &str) -> Option<u16> {
        if addr_hex.is_empty() {
            return None;
        }

        let parts: Vec<&str> = addr_hex.split(':').collect();
        if parts.len() < 2 {
            return None;
        }

        let port_hex = parts[1];
        u16::from_str_radix(port_hex.trim_start_matches('0'), 16).ok()
    }
}
