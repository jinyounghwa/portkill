use std::io::{BufRead, BufReader, Error};

#[derive(Clone, Debug, PartialEq)]
pub enum Protocol {
    Tcp,
    Tcp6,
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
        let remote_port = Self::parse_port(rem_addr_hex);

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
