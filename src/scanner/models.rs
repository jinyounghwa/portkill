use std::io::{BufRead, BufReader, Error};

#[derive(Clone, Debug, PartialEq)]
pub enum Protocol {
    Tcp,
    Tcp6,
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::Tcp => write!(f, "TCP"),
            Protocol::Tcp6 => write!(f, "TCP6"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SocketState {
    Established,
    Listen,
    TimeWait,
    CloseWait,
    Other(u8),
}

impl std::fmt::Display for SocketState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SocketState::Established => write!(f, "ESTABLISHED"),
            SocketState::Listen => write!(f, "LISTEN"),
            SocketState::TimeWait => write!(f, "TIME_WAIT"),
            SocketState::CloseWait => write!(f, "CLOSE_WAIT"),
            SocketState::Other(code) => write!(f, "Other({})", code),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PortEntry {
    pub port: u16,
    pub protocol: Protocol,
    pub state: SocketState,
    pub local_addr: String,
    pub remote_addr: String,
    pub inode: Option<u32>,
    pub pid: Option<u32>,
    pub process_name: String,
    pub cmdline: String,
    pub user: String,
}

pub struct Scanner;

impl Scanner {
    pub fn scan_tcp() -> Result<Vec<PortEntry>, Error> {
        // macOS fallback using lsof
        if cfg!(target_os = "macos") {
            return Self::scan_with_lsof(false);
        }

        let file = std::fs::File::open("/proc/net/tcp")?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines().skip(1) {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            if let Some(entry) = Self::parse_line(&line, Protocol::Tcp) {
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    pub fn scan_tcp6() -> Result<Vec<PortEntry>, Error> {
        // macOS fallback using lsof
        if cfg!(target_os = "macos") {
            return Self::scan_with_lsof(true);
        }

        let file = std::fs::File::open("/proc/net/tcp6")?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines().skip(1) {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            if let Some(entry) = Self::parse_line(&line, Protocol::Tcp6) {
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    #[cfg(target_os = "macos")]
    fn scan_with_lsof(ipv6: bool) -> Result<Vec<PortEntry>, Error> {
        use std::process::Command;

        let output = Command::new("lsof")
            .args(&["-i", if ipv6 { "tcp6" } else { "tcp" }, "-P", "-n"])
            .output()
            .map_err(|e| Error::new(std::io::ErrorKind::Other, format!("lsof failed: {}", e)))?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut entries = Vec::new();

        for line in output_str.lines().skip(1) {
            if let Some(entry) = Self::parse_lsof_line(line, if ipv6 { Protocol::Tcp6 } else { Protocol::Tcp }) {
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    #[cfg(not(target_os = "macos"))]
    fn scan_with_lsof(_ipv6: bool) -> Result<Vec<PortEntry>, Error> {
        Err(Error::new(std::io::ErrorKind::Unsupported, "lsof not supported"))
    }

    #[cfg(target_os = "macos")]
    fn parse_lsof_line(line: &str, protocol: Protocol) -> Option<PortEntry> {
        // lsof format: COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            return None;
        }

        let process_name = parts[0].to_string();
        let pid = parts[1].parse::<u32>().ok()?;
        let user = parts[2].to_string();

        // NAME field contains address:port and possibly state
        // Examples:
        // *:57698 (LISTEN)
        // 192.168.0.109:57698->192.168.0.175:52113 (ESTABLISHED)
        let name_parts: Vec<&str> = parts[8..].iter().map(|s| *s).collect();
        let name = name_parts.join(" ");

        // Parse state
        let state = if name.contains("LISTEN") {
            SocketState::Listen
        } else if name.contains("ESTABLISHED") {
            SocketState::Established
        } else if name.contains("TIME_WAIT") {
            SocketState::TimeWait
        } else if name.contains("CLOSE_WAIT") {
            SocketState::CloseWait
        } else {
            return None; // Skip other states
        };

        // Extract local address and port
        let addr_part = name.split_whitespace().next()?;

        // For ESTABLISHED, split by ->
        let local_addr = if addr_part.contains("->") {
            addr_part.split("->").next()?.to_string()
        } else {
            addr_part.to_string()
        };

        // Extract port from *:PORT or IP:PORT
        let port = if let Some(pos) = local_addr.rfind(':') {
            local_addr[pos + 1..].parse::<u16>().ok()?
        } else {
            return None;
        };

        Some(PortEntry {
            port,
            protocol,
            state,
            local_addr,
            remote_addr: String::new(),
            inode: None,
            pid: Some(pid),
            process_name,
            cmdline: String::new(),
            user,
        })
    }

    #[cfg(not(target_os = "macos"))]
    fn parse_lsof_line(_line: &str, _protocol: Protocol) -> Option<PortEntry> {
        None
    }

    fn parse_line(line: &str, protocol: Protocol) -> Option<PortEntry> {
        let fields: Vec<&str> = line.split_whitespace().collect();

        if fields.len() < 10 {
            return None;
        }

        let local_addr_hex = fields[1];
        let rem_addr_hex = fields[2];
        let state_hex = fields[3];
        let inode_str = fields[9];

        let local_port = Self::parse_port(local_addr_hex)?;
        let _remote_port = Self::parse_port(rem_addr_hex);

        let state = Self::parse_state(state_hex)?;

        let local_addr = Self::parse_address(local_addr_hex);
        let remote_addr = Self::parse_address(rem_addr_hex);

        let inode = if inode_str != "0" {
            inode_str.parse::<u32>().ok()
        } else {
            None
        };

        Some(PortEntry {
            port: local_port,
            protocol,
            state,
            local_addr,
            remote_addr,
            inode,
            pid: None,
            process_name: String::new(),
            cmdline: String::new(),
            user: String::new(),
        })
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

    fn parse_state(state_hex: &str) -> Option<SocketState> {
        if state_hex.is_empty() {
            return Some(SocketState::Other(0));
        }

        let state_val = u8::from_str_radix(state_hex.trim_start_matches('0'), 16).ok()?;

        match state_val {
            0x01 => Some(SocketState::Established),
            0x0A => Some(SocketState::Listen),
            0x06 => Some(SocketState::TimeWait),
            0x08 => Some(SocketState::CloseWait),
            _ => Some(SocketState::Other(state_val)),
        }
    }

    fn parse_address(addr_hex: &str) -> String {
        if addr_hex.is_empty() {
            return String::new();
        }

        let parts: Vec<&str> = addr_hex.split(':').collect();

        if parts.len() < 2 {
            return String::new();
        }

        let ip_part = parts[0];
        let port_part = parts[1];

        let ip = Self::parse_ip_hex(ip_part);

        if let Some(port) = Self::parse_port(port_part) {
            format!("{}:{}", ip, port)
        } else {
            ip
        }
    }

    fn parse_ip_hex(hex: &str) -> String {
        if hex.is_empty() || hex.len() < 8 {
            return String::new();
        }

        let bytes: Vec<u8> = hex
            .as_bytes()
            .chunks(2)
            .rev()
            .take(4)
            .map(|chunk| u8::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16).unwrap_or(0))
            .collect();

        format!("{}.{}.{}.{}", bytes[0], bytes[1], bytes[2], bytes[3])
    }
}
