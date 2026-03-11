//! SMB1/CIFS server implementation
//!
//! Minimal read-only SMB1 server for Windows 98/XP compatibility.
//! Supports guest authentication only - no NTLM.
//!
//! # References
//! - MS-SMB: https://docs.microsoft.com/en-us/openspecs/windows_protocols/ms-smb/
//! - MS-CIFS: https://docs.microsoft.com/en-us/openspecs/windows_protocols/ms-cifs/
//!
//! # Testing
//! - Wireshark with `smb` filter
//! - smbclient -L //localhost -p 4450 -N
//! - Windows XP: net use M: \\192.168.1.x\share

use crate::config::SmbConfig;
use crate::vfs::{SharedVfs, VfsDirEntry};
use async_trait::async_trait;
use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;

use super::ProtocolServer;

// =============================================================================
// Constants
// =============================================================================

/// SMB1 protocol magic: 0xFF 'S' 'M' 'B'
const SMB_MAGIC: [u8; 4] = [0xFF, b'S', b'M', b'B'];

/// NetBIOS session message type for session message
const NETBIOS_SESSION_MESSAGE: u8 = 0x00;

// SMB1 Command codes (MS-SMB 2.2.3)
mod commands {
    pub const SMB_COM_NEGOTIATE: u8 = 0x72;
    pub const SMB_COM_SESSION_SETUP_ANDX: u8 = 0x73;
    pub const SMB_COM_LOGOFF_ANDX: u8 = 0x74;
    pub const SMB_COM_TREE_CONNECT_ANDX: u8 = 0x75;
    pub const SMB_COM_TREE_DISCONNECT: u8 = 0x71;
    pub const SMB_COM_NT_CREATE_ANDX: u8 = 0xA2;
    pub const SMB_COM_READ_ANDX: u8 = 0x2E;
    pub const SMB_COM_CLOSE: u8 = 0x04;
    pub const SMB_COM_TRANSACTION2: u8 = 0x32;
    pub const SMB_COM_NT_TRANSACT: u8 = 0xA0;
    pub const SMB_COM_FIND_CLOSE2: u8 = 0x34;
    pub const SMB_COM_ECHO: u8 = 0x2B;
    // Reserved for potential future use
    #[allow(dead_code)]
    pub const SMB_COM_QUERY_INFORMATION: u8 = 0x08;
    #[allow(dead_code)]
    pub const SMB_COM_QUERY_INFORMATION2: u8 = 0x23;
}

// NT_TRANSACT subcommand codes (MS-SMB 2.2.7)
mod nt_transact {
    pub const NT_TRANSACT_NOTIFY_CHANGE: u16 = 0x0004;
    pub const NT_TRANSACT_QUERY_SECURITY_DESC: u16 = 0x0006;
}

// TRANS2 subcommand codes (MS-SMB 2.2.6)
mod trans2 {
    pub const TRANS2_FIND_FIRST2: u16 = 0x0001;
    pub const TRANS2_FIND_NEXT2: u16 = 0x0002;
    pub const TRANS2_QUERY_FS_INFO: u16 = 0x0003;
    pub const TRANS2_QUERY_PATH_INFO: u16 = 0x0005;
    pub const TRANS2_QUERY_FILE_INFO: u16 = 0x0007;
}

// SMB Status codes (MS-SMB 2.2.2.4)
#[allow(dead_code)]
mod status {
    pub const STATUS_SUCCESS: u32 = 0x00000000;
    pub const STATUS_MORE_PROCESSING_REQUIRED: u32 = 0xC0000016;
    pub const STATUS_NO_SUCH_FILE: u32 = 0xC000000F;
    pub const STATUS_ACCESS_DENIED: u32 = 0xC0000022;
    pub const STATUS_OBJECT_NAME_NOT_FOUND: u32 = 0xC0000034;
    pub const STATUS_OBJECT_PATH_NOT_FOUND: u32 = 0xC000003A;
    pub const STATUS_INVALID_HANDLE: u32 = 0xC0000008;
    pub const STATUS_NOT_IMPLEMENTED: u32 = 0xC0000002;
    pub const STATUS_NO_MORE_FILES: u32 = 0x80000006;
    pub const STATUS_INVALID_SMB: u32 = 0x00010002; // DOS error format
}

// SMB Header flags (MS-SMB 2.2.3.1)
#[allow(dead_code)]
mod flags {
    pub const SMB_FLAGS_REPLY: u8 = 0x80;
    pub const SMB_FLAGS_CASE_INSENSITIVE: u8 = 0x08;
    pub const SMB_FLAGS_CANONICALIZED_PATHS: u8 = 0x10;
}

#[allow(dead_code)]
mod flags2 {
    pub const SMB_FLAGS2_UNICODE: u16 = 0x8000;
    pub const SMB_FLAGS2_NT_STATUS: u16 = 0x4000;
    pub const SMB_FLAGS2_EXTENDED_SECURITY: u16 = 0x0800;
    pub const SMB_FLAGS2_LONG_NAMES: u16 = 0x0001;
}

// =============================================================================
// Packet Structures
// =============================================================================

/// SMB1 Header (32 bytes) - MS-SMB 2.2.3.1
#[derive(Debug, Clone)]
pub struct SmbHeader {
    pub command: u8,
    pub status: u32, // NT_STATUS or DOS error
    pub flags: u8,
    pub flags2: u16,
    pub pid_high: u16,
    pub signature: [u8; 8], // Security signature (usually zeros)
    pub reserved: u16,
    pub tid: u16,     // Tree ID (share connection)
    pub pid_low: u16, // Process ID
    pub uid: u16,     // User ID (session)
    pub mid: u16,     // Multiplex ID (request/response matching)
}

impl SmbHeader {
    pub fn new_response(request: &SmbHeader, status: u32) -> Self {
        Self {
            command: request.command,
            status,
            flags: request.flags | flags::SMB_FLAGS_REPLY,
            flags2: request.flags2,
            pid_high: request.pid_high,
            signature: [0u8; 8],
            reserved: 0,
            tid: request.tid,
            pid_low: request.pid_low,
            uid: request.uid,
            mid: request.mid,
        }
    }

    pub fn parse(data: &[u8]) -> io::Result<Self> {
        if data.len() < 32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Header too short",
            ));
        }
        if &data[0..4] != &SMB_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid SMB magic",
            ));
        }

        let mut signature = [0u8; 8];
        signature.copy_from_slice(&data[14..22]);

        Ok(Self {
            command: data[4],
            status: u32::from_le_bytes([data[5], data[6], data[7], data[8]]),
            flags: data[9],
            flags2: u16::from_le_bytes([data[10], data[11]]),
            pid_high: u16::from_le_bytes([data[12], data[13]]),
            signature,
            reserved: u16::from_le_bytes([data[22], data[23]]),
            tid: u16::from_le_bytes([data[24], data[25]]),
            pid_low: u16::from_le_bytes([data[26], data[27]]),
            uid: u16::from_le_bytes([data[28], data[29]]),
            mid: u16::from_le_bytes([data[30], data[31]]),
        })
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32);
        buf.extend_from_slice(&SMB_MAGIC);
        buf.push(self.command);
        buf.extend_from_slice(&self.status.to_le_bytes());
        buf.push(self.flags);
        buf.extend_from_slice(&self.flags2.to_le_bytes());
        buf.extend_from_slice(&self.pid_high.to_le_bytes());
        buf.extend_from_slice(&self.signature);
        buf.extend_from_slice(&self.reserved.to_le_bytes());
        buf.extend_from_slice(&self.tid.to_le_bytes());
        buf.extend_from_slice(&self.pid_low.to_le_bytes());
        buf.extend_from_slice(&self.uid.to_le_bytes());
        buf.extend_from_slice(&self.mid.to_le_bytes());
        buf
    }

    pub fn is_unicode(&self) -> bool {
        self.flags2 & flags2::SMB_FLAGS2_UNICODE != 0
    }
}

/// Raw SMB message (header + parameter words + data)
#[derive(Debug)]
pub struct SmbMessage {
    pub header: SmbHeader,
    pub params: Vec<u8>, // Parameter words (word_count * 2 bytes)
    pub data: Vec<u8>,   // Data bytes
}

impl SmbMessage {
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        if data.len() < 35 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Message too short",
            ));
        }

        let header = SmbHeader::parse(data)?;
        let word_count = data[32] as usize;
        let params_end = 33 + word_count * 2;

        if data.len() < params_end + 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Params truncated",
            ));
        }

        let params = data[33..params_end].to_vec();
        let byte_count = u16::from_le_bytes([data[params_end], data[params_end + 1]]) as usize;
        let data_start = params_end + 2;
        let data_end = data_start + byte_count;

        if data.len() < data_end {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Data truncated"));
        }

        Ok(Self {
            header,
            params,
            data: data[data_start..data_end].to_vec(),
        })
    }

    pub fn serialize(&self) -> Vec<u8> {
        let word_count = (self.params.len() / 2) as u8;
        let byte_count = self.data.len() as u16;

        let mut buf = self.header.serialize();
        buf.push(word_count);
        buf.extend_from_slice(&self.params);
        buf.extend_from_slice(&byte_count.to_le_bytes());
        buf.extend_from_slice(&self.data);
        buf
    }
}

// =============================================================================
// Session State
// =============================================================================

/// File handle tracking for open files
#[derive(Debug)]
struct OpenFile {
    virtual_path: String,
    physical_path: std::path::PathBuf,
    #[allow(dead_code)]
    tree_id: u16,
}

/// Tree connection (mounted share)
#[derive(Debug)]
struct TreeConnection {
    share_name: String,
    #[allow(dead_code)]
    virtual_root: String,
}

/// Per-connection session state
struct SessionState {
    /// User ID assigned after SESSION_SETUP_ANDX
    uid: u16,
    /// Active tree connections: TID -> TreeConnection
    trees: HashMap<u16, TreeConnection>,
    /// Open file handles: FID -> OpenFile
    files: HashMap<u16, OpenFile>,
    /// Next tree ID to assign
    next_tid: AtomicU16,
    /// Next file ID to assign
    next_fid: AtomicU16,
    /// Directory search handles for FIND_FIRST/FIND_NEXT
    searches: HashMap<u16, SearchHandle>,
    /// Next search handle ID
    next_sid: AtomicU16,
}

/// Search handle for directory enumeration
#[derive(Debug)]
#[allow(dead_code)] // Fields stored for potential FIND_NEXT2 resume
struct SearchHandle {
    /// The directory being searched
    directory_path: String,
    /// Remaining entries to return
    entries: Vec<VfsDirEntry>,
    /// Info level requested by client
    info_level: u16,
    /// Flags from original request
    flags: u16,
}

impl SessionState {
    fn new() -> Self {
        Self {
            uid: 0,
            trees: HashMap::new(),
            files: HashMap::new(),
            next_tid: AtomicU16::new(1),
            next_fid: AtomicU16::new(1),
            searches: HashMap::new(),
            next_sid: AtomicU16::new(1),
        }
    }

    fn allocate_tid(&self) -> u16 {
        self.next_tid.fetch_add(1, Ordering::SeqCst)
    }

    fn allocate_fid(&self) -> u16 {
        self.next_fid.fetch_add(1, Ordering::SeqCst)
    }

    fn allocate_sid(&self) -> u16 {
        self.next_sid.fetch_add(1, Ordering::SeqCst)
    }
}

/// Build full VFS path from a client path and share root.
/// Handles cases where client may have included the share root in the path.
fn build_vfs_path(share_root: &str, client_path: &str) -> String {
    // Normalize: convert backslashes, remove leading slash for comparison
    let normalized = client_path.replace('\\', "/");
    let client_clean = normalized.trim_start_matches('/');
    let share_clean = share_root.trim_start_matches('/');

    // If client path starts with share root, strip it
    let relative_path = if !share_clean.is_empty() && client_clean.starts_with(share_clean) {
        let remainder = &client_clean[share_clean.len()..];
        remainder.trim_start_matches('/')
    } else {
        client_clean
    };

    // Build full path
    if relative_path.is_empty() {
        share_root.to_string()
    } else {
        format!("{}/{}", share_root, relative_path)
    }
}

// =============================================================================
// SMB Server
// =============================================================================

pub struct SmbServer {
    config: SmbConfig,
    vfs: SharedVfs,
    server_name: String,
    running: AtomicBool,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
}

impl SmbServer {
    pub fn new(config: SmbConfig, vfs: SharedVfs, server_name: String) -> Arc<Self> {
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        Arc::new(Self {
            config,
            vfs,
            server_name,
            running: AtomicBool::new(false),
            shutdown_tx,
            shutdown_rx,
        })
    }

    /// Handle a single client connection
    async fn handle_connection(
        self: Arc<Self>,
        mut stream: TcpStream,
        peer_addr: std::net::SocketAddr,
    ) {
        tracing::info!("SMB connection from {}", peer_addr);

        // Enable TCP_NODELAY for low latency (disable Nagle's algorithm)
        if let Err(e) = stream.set_nodelay(true) {
            tracing::warn!("Failed to set TCP_NODELAY: {}", e);
        }

        let state = Arc::new(RwLock::new(SessionState::new()));
        let mut negotiated = false;

        loop {
            // Read NetBIOS session header (4 bytes)
            let mut netbios_header = [0u8; 4];
            if stream.read_exact(&mut netbios_header).await.is_err() {
                break;
            }

            // NetBIOS session message format:
            // - Byte 0: Message type (0x00 = session message)
            // - Bytes 1-3: Length (24-bit big-endian)
            if netbios_header[0] != NETBIOS_SESSION_MESSAGE {
                tracing::warn!(
                    "Unexpected NetBIOS message type: 0x{:02X}",
                    netbios_header[0]
                );
                continue;
            }

            let length = ((netbios_header[1] as usize) << 16)
                | ((netbios_header[2] as usize) << 8)
                | (netbios_header[3] as usize);

            if length > 65536 {
                tracing::warn!("SMB message too large: {} bytes", length);
                break;
            }

            // Read SMB message
            let mut smb_data = vec![0u8; length];
            if stream.read_exact(&mut smb_data).await.is_err() {
                break;
            }

            // Parse and handle message
            let msg = match SmbMessage::parse(&smb_data) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("Failed to parse SMB message: {}", e);
                    continue;
                }
            };

            tracing::debug!(
                "SMB command: 0x{:02X}, TID: {}, UID: {}, MID: {}",
                msg.header.command,
                msg.header.tid,
                msg.header.uid,
                msg.header.mid
            );

            // Dispatch command
            let response = match msg.header.command {
                commands::SMB_COM_NEGOTIATE if !negotiated => {
                    negotiated = true;
                    self.handle_negotiate(&msg).await
                }
                commands::SMB_COM_SESSION_SETUP_ANDX => {
                    self.handle_session_setup(&msg, &state).await
                }
                commands::SMB_COM_TREE_CONNECT_ANDX => self.handle_tree_connect(&msg, &state).await,
                commands::SMB_COM_TREE_DISCONNECT => {
                    self.handle_tree_disconnect(&msg, &state).await
                }
                commands::SMB_COM_NT_CREATE_ANDX => self.handle_nt_create(&msg, &state).await,
                commands::SMB_COM_READ_ANDX => self.handle_read(&msg, &state).await,
                commands::SMB_COM_CLOSE => self.handle_close(&msg, &state).await,
                commands::SMB_COM_TRANSACTION2 => self.handle_trans2(&msg, &state).await,
                commands::SMB_COM_FIND_CLOSE2 => self.handle_find_close2(&msg, &state).await,
                commands::SMB_COM_ECHO => self.handle_echo(&msg).await,
                commands::SMB_COM_LOGOFF_ANDX => self.handle_logoff(&msg, &state).await,
                commands::SMB_COM_NT_TRANSACT => self.handle_nt_transact(&msg, &state).await,
                cmd => {
                    tracing::warn!("Unhandled SMB command: 0x{:02X}", cmd);
                    self.error_response(&msg, status::STATUS_NOT_IMPLEMENTED)
                }
            };

            // Send response with NetBIOS header
            let response_data = response.serialize();
            let response_len = response_data.len();
            let netbios_response = [
                NETBIOS_SESSION_MESSAGE,
                ((response_len >> 16) & 0xFF) as u8,
                ((response_len >> 8) & 0xFF) as u8,
                (response_len & 0xFF) as u8,
            ];

            if stream.write_all(&netbios_response).await.is_err() {
                break;
            }
            if stream.write_all(&response_data).await.is_err() {
                break;
            }
        }

        tracing::info!("SMB connection closed from {}", peer_addr);
    }

    // =========================================================================
    // Command Handlers
    // =========================================================================

    /// NEGOTIATE - Dialect negotiation (MS-SMB 2.2.4.5)
    async fn handle_negotiate(&self, request: &SmbMessage) -> SmbMessage {
        // Client sends list of dialect strings, we pick NT LM 0.12 (SMB1)
        // Parse dialect strings from request.data
        let dialects = parse_dialect_strings(&request.data);
        tracing::debug!("Client offered dialects: {:?}", dialects);

        // Find "NT LM 0.12" or "NT LANMAN 1.0" - required for XP compatibility
        let dialect_index = dialects
            .iter()
            .position(|d| d == "NT LM 0.12" || d == "NT LANMAN 1.0")
            .unwrap_or(0) as u16;

        // Build NEGOTIATE response (MS-SMB 2.2.4.5.2.1)
        // We're using the non-extended security response for simplicity
        let mut params = Vec::with_capacity(34);

        // DialectIndex (2 bytes)
        params.extend_from_slice(&dialect_index.to_le_bytes());
        // SecurityMode (1 byte) - User level security, no signatures
        params.push(0x03);
        // MaxMpxCount (2 bytes) - Max pending requests
        params.extend_from_slice(&50u16.to_le_bytes());
        // MaxNumberVcs (2 bytes) - Max virtual circuits
        params.extend_from_slice(&1u16.to_le_bytes());
        // MaxBufferSize (4 bytes)
        params.extend_from_slice(&16644u32.to_le_bytes());
        // MaxRawSize (4 bytes)
        params.extend_from_slice(&65536u32.to_le_bytes());
        // SessionKey (4 bytes)
        params.extend_from_slice(&0u32.to_le_bytes());
        // Capabilities (4 bytes) - UNICODE, LARGE_FILES, NT_SMBS, STATUS32
        let caps: u32 = 0x000000F3; // CAP_UNICODE | CAP_LARGE_FILES | etc
        params.extend_from_slice(&caps.to_le_bytes());
        // SystemTime (8 bytes) - Windows FILETIME
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let filetime = (now.as_secs() + 11644473600) * 10_000_000;
        params.extend_from_slice(&filetime.to_le_bytes());
        // ServerTimeZone (2 bytes) - Minutes from UTC
        params.extend_from_slice(&0i16.to_le_bytes());
        // EncryptionKeyLength (1 byte) - 0 for no challenge
        params.push(0);

        // Data: Server GUID (optional) + Domain name + Server name
        let mut data = Vec::new();
        // For non-extended security, include domain and server name
        // OEM strings (not Unicode) for simplicity
        data.extend_from_slice(b"WORKGROUP\0");
        data.extend_from_slice(self.server_name.as_bytes());
        data.push(0);

        let mut header = SmbHeader::new_response(&request.header, status::STATUS_SUCCESS);
        header.flags2 |= flags2::SMB_FLAGS2_UNICODE | flags2::SMB_FLAGS2_NT_STATUS;

        SmbMessage {
            header,
            params,
            data,
        }
    }

    /// SESSION_SETUP_ANDX - Authenticate session (MS-SMB 2.2.4.6)
    async fn handle_session_setup(
        &self,
        request: &SmbMessage,
        state: &Arc<RwLock<SessionState>>,
    ) -> SmbMessage {
        // For guest access, we accept any credentials
        // In a real implementation, check password against config

        let uid = {
            let mut state = state.write().await;
            if state.uid == 0 {
                state.uid = 1; // Assign UID
            }
            state.uid
        };

        // Response: AndXCommand, Reserved, AndXOffset, Action (MS-SMB 2.2.4.6.2)
        let params = vec![
            0xFF, // AndXCommand: none
            0x00, // Reserved
            0x00, 0x00, // AndXOffset
            0x01, 0x00, // Action: logged in as guest
        ];

        // Data: Native OS + Native LAN Manager
        let mut data = Vec::new();
        // Pad for Unicode alignment
        data.push(0);
        // NativeOS (Unicode)
        for c in "Unix\0".encode_utf16() {
            data.extend_from_slice(&c.to_le_bytes());
        }
        // NativeLanMan
        for c in "Depot SMB\0".encode_utf16() {
            data.extend_from_slice(&c.to_le_bytes());
        }
        // PrimaryDomain
        for c in "WORKGROUP\0".encode_utf16() {
            data.extend_from_slice(&c.to_le_bytes());
        }

        let mut header = SmbHeader::new_response(&request.header, status::STATUS_SUCCESS);
        header.uid = uid;

        SmbMessage {
            header,
            params,
            data,
        }
    }

    /// TREE_CONNECT_ANDX - Connect to a share (MS-SMB 2.2.4.7)
    async fn handle_tree_connect(
        &self,
        request: &SmbMessage,
        state: &Arc<RwLock<SessionState>>,
    ) -> SmbMessage {
        // Parse TREE_CONNECT_ANDX request
        // Params: AndXCommand(1), Reserved(1), AndXOffset(2), Flags(2), PasswordLength(2)
        // Data: Password(PasswordLength), Path(null-term), Service(null-term)

        let password_length = if request.params.len() >= 8 {
            u16::from_le_bytes([request.params[6], request.params[7]]) as usize
        } else {
            1 // Default assumption
        };

        // Skip password bytes to get to path
        // Note: SMB1 Unicode strings start at odd offsets when password length is odd
        let path_start = password_length;

        // Parse share path - format: \\server\share
        let share_path = if request.header.is_unicode() && path_start < request.data.len() {
            parse_unicode_string(&request.data[path_start..])
        } else if path_start < request.data.len() {
            parse_ascii_string(&request.data[path_start..])
        } else {
            String::new()
        };

        tracing::debug!(
            "TREE_CONNECT to: {} (pw_len={}, data_len={})",
            share_path,
            password_length,
            request.data.len()
        );

        // Debug: hex dump first 50 bytes
        let hex: String = request
            .data
            .iter()
            .take(50)
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ");
        tracing::trace!("TREE_CONNECT data hex: {}", hex);

        // Extract share name from path (after last \)
        let share_name = share_path
            .split('\\')
            .last()
            .unwrap_or(&share_path)
            .to_uppercase();

        // TODO: Verify share exists in VFS
        // For now, accept any share name

        let tid = {
            let mut state = state.write().await;
            let tid = state.allocate_tid();
            state.trees.insert(
                tid,
                TreeConnection {
                    share_name: share_name.clone(),
                    virtual_root: format!("/{}", share_name.to_lowercase()),
                },
            );
            tid
        };

        // Response parameters
        let params = vec![
            0xFF, // AndXCommand: none
            0x00, // Reserved
            0x00, 0x00, // AndXOffset
            0x01, 0x00, // OptionalSupport: SMB_SUPPORT_SEARCH_BITS
            0x00, 0x00, 0x00, 0x00, // MaximalShareAccessRights (read-only)
            0x00, 0x00, 0x00, 0x00, // GuestMaximalShareAccessRights
        ];

        // Data: Service type + Native filesystem
        let mut data = Vec::new();
        data.extend_from_slice(b"A:\0"); // Service: disk share
        data.extend_from_slice(b"NTFS\0"); // Filesystem

        let mut header = SmbHeader::new_response(&request.header, status::STATUS_SUCCESS);
        header.tid = tid;

        SmbMessage {
            header,
            params,
            data,
        }
    }

    /// TREE_DISCONNECT - Disconnect from share
    async fn handle_tree_disconnect(
        &self,
        request: &SmbMessage,
        state: &Arc<RwLock<SessionState>>,
    ) -> SmbMessage {
        let tid = request.header.tid;

        {
            let mut state = state.write().await;
            state.trees.remove(&tid);
        }

        SmbMessage {
            header: SmbHeader::new_response(&request.header, status::STATUS_SUCCESS),
            params: vec![],
            data: vec![],
        }
    }

    /// NT_CREATE_ANDX - Open file or directory (MS-SMB 2.2.4.9)
    async fn handle_nt_create(
        &self,
        request: &SmbMessage,
        state: &Arc<RwLock<SessionState>>,
    ) -> SmbMessage {
        // Parse filename from request
        // Parameters contain various flags, data contains filename
        let filename = if request.header.is_unicode() {
            // Skip padding byte for Unicode alignment
            let data = if request.data.first() == Some(&0) {
                &request.data[1..]
            } else {
                &request.data
            };
            parse_unicode_string(data)
        } else {
            parse_ascii_string(&request.data)
        };

        // Convert Windows path separators
        let virtual_path = filename.replace('\\', "/");
        tracing::debug!("NT_CREATE file: {}", virtual_path);

        // Get tree connection to determine share root
        let share_root = {
            let state = state.read().await;
            state
                .trees
                .get(&request.header.tid)
                .map(|t| t.virtual_root.clone())
                .unwrap_or_default()
        };

        let full_path = build_vfs_path(&share_root, &virtual_path);

        // Check if path exists and get metadata via VFS
        let metadata = match self.vfs.metadata(&full_path).await {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!("NT_CREATE path not found: {} ({:?})", full_path, e);
                return self.error_response(request, status::STATUS_OBJECT_NAME_NOT_FOUND);
            }
        };

        // Resolve physical path once and cache it for fast reads
        let physical_path = match self.vfs.resolve_path(&full_path) {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!("NT_CREATE resolve failed: {} ({:?})", full_path, e);
                return self.error_response(request, status::STATUS_OBJECT_NAME_NOT_FOUND);
            }
        };

        // Allocate FID
        let fid = {
            let mut state = state.write().await;
            let fid = state.allocate_fid();
            state.files.insert(
                fid,
                OpenFile {
                    virtual_path: full_path.clone(),
                    physical_path,
                    tree_id: request.header.tid,
                },
            );
            fid
        };

        // Build response (MS-SMB 2.2.4.9.2)
        // This is complex - 69 bytes of parameters
        let mut params = Vec::with_capacity(69);
        params.push(0xFF); // AndXCommand: none
        params.push(0x00); // Reserved
        params.extend_from_slice(&0u16.to_le_bytes()); // AndXOffset
        params.push(0x00); // OplockLevel: none
        params.extend_from_slice(&fid.to_le_bytes()); // FID
        params.extend_from_slice(&1u32.to_le_bytes()); // CreateAction: file opened

        // CreationTime, LastAccessTime, LastWriteTime, ChangeTime (8 bytes each)
        let filetime = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| (d.as_secs() + 11644473600) * 10_000_000)
            .unwrap_or(0);
        for _ in 0..4 {
            params.extend_from_slice(&filetime.to_le_bytes());
        }

        // ExtFileAttributes (4 bytes)
        let attrs: u32 = if metadata.is_dir { 0x10 } else { 0x20 }; // Directory or Archive
        params.extend_from_slice(&attrs.to_le_bytes());

        // AllocationSize (8 bytes)
        params.extend_from_slice(&metadata.size.to_le_bytes());
        // EndOfFile (8 bytes)
        params.extend_from_slice(&metadata.size.to_le_bytes());

        // FileType (2 bytes) - disk file
        params.extend_from_slice(&0u16.to_le_bytes());
        // DeviceState (2 bytes)
        params.extend_from_slice(&0u16.to_le_bytes());
        // Directory (1 byte)
        params.push(if metadata.is_dir { 1 } else { 0 });

        SmbMessage {
            header: SmbHeader::new_response(&request.header, status::STATUS_SUCCESS),
            params,
            data: vec![],
        }
    }

    /// READ_ANDX - Read file data (MS-SMB 2.2.4.2)
    async fn handle_read(
        &self,
        request: &SmbMessage,
        state: &Arc<RwLock<SessionState>>,
    ) -> SmbMessage {
        // Parse parameters (MS-SMB 2.2.4.2.1)
        // WordCount = 10 or 12
        // AndXCommand(1), Reserved(1), AndXOffset(2), FID(2), Offset(4), MaxCountOfBytesToReturn(2)...
        if request.params.len() < 20 {
            return self.error_response(request, status::STATUS_INVALID_SMB);
        }

        let fid = u16::from_le_bytes([request.params[4], request.params[5]]); // FID at offset 4
        let offset = u32::from_le_bytes([
            request.params[6],
            request.params[7],
            request.params[8],
            request.params[9],
        ]) as u64;
        let max_count = u16::from_le_bytes([request.params[10], request.params[11]]) as usize;

        tracing::debug!(
            "READ_ANDX: fid={}, offset={}, max={}",
            fid,
            offset,
            max_count
        );

        // Get cached physical path from state (resolved once at NT_CREATE)
        let physical_path = {
            let state = state.read().await;
            match state.files.get(&fid) {
                Some(f) => f.physical_path.clone(),
                None => return self.error_response(request, status::STATUS_INVALID_HANDLE),
            }
        };

        // Read file directly using cached physical path (fast path)
        let data = match Self::read_file_direct(&physical_path, offset, max_count).await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("READ_ANDX failed: {}", e);
                return self.error_response(request, status::STATUS_NO_SUCH_FILE);
            }
        };

        let data_length = data.len() as u16;

        // Build response
        let mut params = Vec::with_capacity(24);
        params.push(0xFF); // AndXCommand: none
        params.push(0x00); // Reserved
        params.extend_from_slice(&0u16.to_le_bytes()); // AndXOffset
        params.extend_from_slice(&0u16.to_le_bytes()); // Available (reserved)
        params.extend_from_slice(&0u16.to_le_bytes()); // DataCompactionMode
        params.extend_from_slice(&0u16.to_le_bytes()); // Reserved
        params.extend_from_slice(&data_length.to_le_bytes()); // DataLength
        params.extend_from_slice(&59u16.to_le_bytes()); // DataOffset (32 header + 1 wordcount + 24 params + 2 bytecount = 59)
        params.extend_from_slice(&0u16.to_le_bytes()); // DataLengthHigh
        params.extend_from_slice(&[0u8; 8]); // Reserved

        SmbMessage {
            header: SmbHeader::new_response(&request.header, status::STATUS_SUCCESS),
            params,
            data,
        }
    }

    /// CLOSE - Close file handle
    async fn handle_close(
        &self,
        request: &SmbMessage,
        state: &Arc<RwLock<SessionState>>,
    ) -> SmbMessage {
        if request.params.len() < 2 {
            return self.error_response(request, status::STATUS_INVALID_SMB);
        }

        let fid = u16::from_le_bytes([request.params[0], request.params[1]]);

        {
            let mut state = state.write().await;
            state.files.remove(&fid);
        }

        SmbMessage {
            header: SmbHeader::new_response(&request.header, status::STATUS_SUCCESS),
            params: vec![],
            data: vec![],
        }
    }

    /// TRANSACTION2 - Various operations including directory listing
    async fn handle_trans2(
        &self,
        request: &SmbMessage,
        state: &Arc<RwLock<SessionState>>,
    ) -> SmbMessage {
        // Parse TRANS2 setup words to determine subcommand
        if request.params.len() < 28 {
            return self.error_response(request, status::STATUS_INVALID_SMB);
        }

        let param_offset = u16::from_le_bytes([request.params[20], request.params[21]]) as usize;
        let _data_offset = u16::from_le_bytes([request.params[24], request.params[25]]) as usize;

        // Subcommand is in setup words (after 28 bytes of params)
        if request.params.len() < 30 {
            return self.error_response(request, status::STATUS_INVALID_SMB);
        }
        let subcommand = u16::from_le_bytes([request.params[28], request.params[29]]);

        tracing::debug!("TRANS2 subcommand: 0x{:04X}", subcommand);

        match subcommand {
            trans2::TRANS2_FIND_FIRST2 => {
                self.handle_find_first2(request, state, param_offset).await
            }
            trans2::TRANS2_FIND_NEXT2 => self.handle_find_next2(request, state).await,
            trans2::TRANS2_QUERY_FS_INFO => {
                self.handle_query_fs_info(request, state, param_offset)
                    .await
            }
            trans2::TRANS2_QUERY_PATH_INFO => {
                self.handle_query_path_info(request, state, param_offset)
                    .await
            }
            trans2::TRANS2_QUERY_FILE_INFO => {
                self.handle_query_file_info(request, state, param_offset)
                    .await
            }
            _ => {
                tracing::warn!("Unhandled TRANS2 subcommand: 0x{:04X}", subcommand);
                self.error_response(request, status::STATUS_NOT_IMPLEMENTED)
            }
        }
    }

    /// FIND_FIRST2 - Start directory enumeration (MS-SMB 2.2.6.2)
    async fn handle_find_first2(
        &self,
        request: &SmbMessage,
        state: &Arc<RwLock<SessionState>>,
        param_offset: usize,
    ) -> SmbMessage {
        // Calculate relative offset within request.data
        // param_offset is from start of SMB header
        // request.data starts after: header (32) + word_count (1) + params + byte_count (2)
        let word_count = (request.params.len() / 2) as usize;
        let data_start_offset = 32 + 1 + (word_count * 2) + 2;
        let relative_offset = param_offset.saturating_sub(data_start_offset);

        // TRANS2_FIND_FIRST2 parameters are in request.data at relative_offset
        // Format:
        // - SearchAttributes (2 bytes)
        // - SearchCount (2 bytes) - max entries to return
        // - Flags (2 bytes)
        // - InformationLevel (2 bytes)
        // - SearchStorageType (4 bytes)
        // - FileName (variable, null-terminated)

        let params_data = &request.data[relative_offset..];

        if params_data.len() < 12 {
            return self.error_response(request, status::STATUS_INVALID_SMB);
        }

        let _search_attrs = u16::from_le_bytes([params_data[0], params_data[1]]);
        let max_count = u16::from_le_bytes([params_data[2], params_data[3]]) as usize;
        let flags = u16::from_le_bytes([params_data[4], params_data[5]]);
        let info_level = u16::from_le_bytes([params_data[6], params_data[7]]);
        // SearchStorageType at bytes 8-11

        // Parse search pattern (starts at byte 12)
        let pattern = if request.header.is_unicode() {
            // May need padding for alignment - check if byte 12 is a null padding byte
            let start = if params_data.len() > 12 && params_data[12] == 0 {
                13
            } else {
                12
            };
            parse_unicode_string(&params_data[start..])
        } else {
            parse_ascii_string(&params_data[12..])
        };

        // Default empty pattern to '*'
        let pattern = if pattern.is_empty() {
            "*".to_string()
        } else {
            pattern
        };

        tracing::debug!(
            "FIND_FIRST2: pattern='{}', info_level=0x{:04X}, max={}",
            pattern,
            info_level,
            max_count
        );

        // Convert pattern to directory path
        // Pattern is like "\\*" or "\\subdir\\*" or "\\subdir\\*.mp3"
        let pattern_path = pattern.replace('\\', "/");
        let (dir_path, file_pattern) = if let Some(pos) = pattern_path.rfind('/') {
            let dir = &pattern_path[..pos];
            let pat = &pattern_path[pos + 1..];
            (if dir.is_empty() { "" } else { dir }, pat)
        } else {
            ("", pattern_path.as_str())
        };

        // Get share root from tree connection
        let share_root = {
            let state = state.read().await;
            state
                .trees
                .get(&request.header.tid)
                .map(|t| t.virtual_root.clone())
                .unwrap_or_else(|| "/".to_string())
        };

        let full_dir = build_vfs_path(&share_root, dir_path);

        tracing::debug!(
            "FIND_FIRST2: listing dir='{}', pattern='{}'",
            full_dir,
            file_pattern
        );

        // List directory via VFS
        let entries = match self.vfs.list_dir(&full_dir).await {
            Ok(entries) => entries,
            Err(e) => {
                tracing::debug!("FIND_FIRST2 list_dir failed: {:?}", e);
                return self.error_response(request, status::STATUS_OBJECT_PATH_NOT_FOUND);
            }
        };

        // Filter entries by pattern (simple wildcard matching)
        let filtered: Vec<VfsDirEntry> = entries
            .into_iter()
            .filter(|e| match_wildcard(file_pattern, &e.name))
            .collect();

        if filtered.is_empty() {
            // No matches
            let params = vec![
                0x00, 0x00, // SID
                0x00, 0x00, // SearchCount
                0x01, 0x00, // EndOfSearch: yes
                0x00, 0x00, // EaErrorOffset
                0x00, 0x00, // LastNameOffset
            ];
            return SmbMessage {
                header: SmbHeader::new_response(&request.header, status::STATUS_NO_MORE_FILES),
                params,
                data: vec![],
            };
        }

        // Format entries and split into first batch + remaining
        let (entry_data, returned_count, last_offset) = format_find_entries(
            &filtered[..filtered.len().min(max_count)],
            info_level,
            request.header.is_unicode(),
        );

        let end_of_search = returned_count >= filtered.len();

        // If there are more entries, store search handle
        let sid = if !end_of_search {
            let remaining: Vec<VfsDirEntry> = filtered.into_iter().skip(returned_count).collect();
            let mut state = state.write().await;
            let sid = state.allocate_sid();
            state.searches.insert(
                sid,
                SearchHandle {
                    directory_path: full_dir,
                    entries: remaining,
                    info_level,
                    flags,
                },
            );
            sid
        } else {
            0 // No search handle needed
        };

        // Build TRANS2 response
        // Parameters: SID(2) + SearchCount(2) + EndOfSearch(2) + EaErrorOffset(2) + LastNameOffset(2)
        let mut params = Vec::with_capacity(10);
        params.extend_from_slice(&sid.to_le_bytes());
        params.extend_from_slice(&(returned_count as u16).to_le_bytes());
        params.extend_from_slice(&(if end_of_search { 1u16 } else { 0u16 }).to_le_bytes());
        params.extend_from_slice(&0u16.to_le_bytes()); // EaErrorOffset
        params.extend_from_slice(&(last_offset as u16).to_le_bytes());

        // TRANS2 response format
        self.build_trans2_response(request, params, entry_data)
    }

    /// FIND_NEXT2 - Continue directory enumeration (MS-SMB 2.2.6.3)
    async fn handle_find_next2(
        &self,
        request: &SmbMessage,
        state: &Arc<RwLock<SessionState>>,
    ) -> SmbMessage {
        // Parameters in request.data:
        // - SID (2 bytes)
        // - SearchCount (2 bytes)
        // - InformationLevel (2 bytes)
        // - ResumeKey (4 bytes)
        // - Flags (2 bytes)
        // - FileName (variable)

        if request.data.len() < 12 {
            return self.error_response(request, status::STATUS_INVALID_SMB);
        }

        let sid = u16::from_le_bytes([request.data[0], request.data[1]]);
        let max_count = u16::from_le_bytes([request.data[2], request.data[3]]) as usize;
        let info_level = u16::from_le_bytes([request.data[4], request.data[5]]);

        tracing::debug!("FIND_NEXT2: sid={}, max={}", sid, max_count);

        // Get remaining entries from search handle
        let (entries, end_of_search) = {
            let mut state = state.write().await;
            if let Some(handle) = state.searches.get_mut(&sid) {
                let take_count = max_count.min(handle.entries.len());
                let batch: Vec<VfsDirEntry> = handle.entries.drain(..take_count).collect();
                let done = handle.entries.is_empty();
                if done {
                    state.searches.remove(&sid);
                }
                (batch, done)
            } else {
                return self.error_response(request, status::STATUS_INVALID_HANDLE);
            }
        };

        if entries.is_empty() {
            let params = vec![
                0x00, 0x00, // SearchCount
                0x01, 0x00, // EndOfSearch: yes
                0x00, 0x00, // EaErrorOffset
                0x00, 0x00, // LastNameOffset
            ];
            return SmbMessage {
                header: SmbHeader::new_response(&request.header, status::STATUS_NO_MORE_FILES),
                params,
                data: vec![],
            };
        }

        let (entry_data, returned_count, last_offset) =
            format_find_entries(&entries, info_level, request.header.is_unicode());

        // Parameters: SearchCount(2) + EndOfSearch(2) + EaErrorOffset(2) + LastNameOffset(2)
        let mut params = Vec::with_capacity(8);
        params.extend_from_slice(&(returned_count as u16).to_le_bytes());
        params.extend_from_slice(&(if end_of_search { 1u16 } else { 0u16 }).to_le_bytes());
        params.extend_from_slice(&0u16.to_le_bytes()); // EaErrorOffset
        params.extend_from_slice(&(last_offset as u16).to_le_bytes());

        self.build_trans2_response(request, params, entry_data)
    }

    /// QUERY_FS_INFO - Get filesystem information (MS-SMB 2.2.6.4)
    async fn handle_query_fs_info(
        &self,
        request: &SmbMessage,
        state: &Arc<RwLock<SessionState>>,
        param_offset: usize,
    ) -> SmbMessage {
        // Get share name from tree connection for volume label
        let share_label = {
            let state = state.read().await;
            state
                .trees
                .get(&request.header.tid)
                .map(|t| t.share_name.clone())
                .unwrap_or_else(|| "SHARE".to_string())
        };

        // Calculate relative offset within request.data
        let word_count = (request.params.len() / 2) as usize;
        let data_start_offset = 32 + 1 + (word_count * 2) + 2;
        let relative_offset = param_offset.saturating_sub(data_start_offset);

        let params_data = &request.data[relative_offset..];

        // Parameters:
        // - InformationLevel (2 bytes)
        if params_data.len() < 2 {
            return self.error_response(request, status::STATUS_INVALID_SMB);
        }

        let info_level = u16::from_le_bytes([params_data[0], params_data[1]]);
        tracing::debug!("QUERY_FS_INFO: level=0x{:04X}", info_level);

        let data = match info_level {
            0x0001 => {
                // SMB_INFO_ALLOCATION - Filesystem allocation info
                let mut d = Vec::with_capacity(18);
                d.extend_from_slice(&0u32.to_le_bytes()); // idFileSystem (ignored)
                d.extend_from_slice(&4096u32.to_le_bytes()); // cSectorUnit (sectors per unit)
                d.extend_from_slice(&0x7FFFFFFFu32.to_le_bytes()); // cUnit (total units - ~8TB)
                d.extend_from_slice(&0x7FFFFFFFu32.to_le_bytes()); // cUnitAvail (free units)
                d.extend_from_slice(&512u16.to_le_bytes()); // cbSector (bytes per sector)
                d
            }
            0x0002 => {
                // SMB_INFO_VOLUME - Volume label
                let label = share_label.as_bytes();
                let mut d = Vec::with_capacity(5 + label.len());
                d.extend_from_slice(&0u32.to_le_bytes()); // ulVolSerialNbr
                d.push(label.len() as u8); // cCharCount
                d.extend_from_slice(label);
                d
            }
            0x0102 => {
                // SMB_QUERY_FS_SIZE_INFO - Size info (NT style)
                let mut d = Vec::with_capacity(24);
                d.extend_from_slice(&0x7FFFFFFFFFFFFFFFu64.to_le_bytes()); // TotalAllocationUnits
                d.extend_from_slice(&0x7FFFFFFFFFFFFFFFu64.to_le_bytes()); // TotalFreeAllocationUnits
                d.extend_from_slice(&1u32.to_le_bytes()); // SectorsPerAllocationUnit
                d.extend_from_slice(&4096u32.to_le_bytes()); // BytesPerSector
                d
            }
            0x0103 => {
                // SMB_QUERY_FS_DEVICE_INFO - Device info
                let mut d = Vec::with_capacity(8);
                d.extend_from_slice(&0x00000007u32.to_le_bytes()); // DeviceType = FILE_DEVICE_DISK
                d.extend_from_slice(&0x00000020u32.to_le_bytes()); // Characteristics = FILE_DEVICE_IS_MOUNTED
                d
            }
            0x0104 => {
                // SMB_QUERY_FS_ATTRIBUTE_INFO - Filesystem attributes
                let name = "NTFS"; // Pretend to be NTFS for compatibility
                let name_bytes: Vec<u8> =
                    name.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
                let mut d = Vec::with_capacity(12 + name_bytes.len());
                d.extend_from_slice(&0x0000001Fu32.to_le_bytes()); // FileSystemAttributes
                d.extend_from_slice(&255u32.to_le_bytes()); // MaxFileNameLengthInBytes
                d.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes()); // LengthOfFileSystemName
                d.extend_from_slice(&name_bytes);
                d
            }
            0x0105 => {
                // SMB_QUERY_FS_VOLUME_INFO - Volume info (NT style)
                let label_bytes: Vec<u8> = share_label
                    .encode_utf16()
                    .flat_map(|c| c.to_le_bytes())
                    .collect();
                let mut d = Vec::with_capacity(18 + label_bytes.len());
                d.extend_from_slice(&0u64.to_le_bytes()); // VolumeCreationTime
                d.extend_from_slice(&0u32.to_le_bytes()); // VolumeSerialNumber
                d.extend_from_slice(&(label_bytes.len() as u32).to_le_bytes()); // VolumeLabelLength
                d.extend_from_slice(&0u16.to_le_bytes()); // Reserved
                d.extend_from_slice(&label_bytes);
                d
            }
            _ => {
                // Default: return SMB_INFO_ALLOCATION for unknown levels
                tracing::debug!(
                    "Unknown FS info level 0x{:04X}, using ALLOCATION",
                    info_level
                );
                let mut d = Vec::with_capacity(18);
                d.extend_from_slice(&0u32.to_le_bytes());
                d.extend_from_slice(&4096u32.to_le_bytes());
                d.extend_from_slice(&0x7FFFFFFFu32.to_le_bytes());
                d.extend_from_slice(&0x7FFFFFFFu32.to_le_bytes());
                d.extend_from_slice(&512u16.to_le_bytes());
                d
            }
        };

        self.build_trans2_response(request, vec![], data)
    }

    /// QUERY_PATH_INFO - Get file/directory metadata (MS-SMB 2.2.6.6)
    async fn handle_query_path_info(
        &self,
        request: &SmbMessage,
        state: &Arc<RwLock<SessionState>>,
        param_offset: usize,
    ) -> SmbMessage {
        // Calculate relative offset within request.data
        let word_count = (request.params.len() / 2) as usize;
        let data_start_offset = 32 + 1 + (word_count * 2) + 2;
        let relative_offset = param_offset.saturating_sub(data_start_offset);

        let params_data = &request.data[relative_offset..];

        // Parameters:
        // - InformationLevel (2 bytes)
        // - Reserved (4 bytes)
        // - FileName (variable)

        if params_data.len() < 6 {
            return self.error_response(request, status::STATUS_INVALID_SMB);
        }

        let info_level = u16::from_le_bytes([params_data[0], params_data[1]]);
        // Reserved bytes 2-5

        let filename = if request.header.is_unicode() {
            // Pad byte for unicode alignment
            let start = if params_data.len() > 6 && params_data[6] == 0 {
                7
            } else {
                6
            };
            parse_unicode_string(&params_data[start..])
        } else {
            parse_ascii_string(&params_data[6..])
        };

        let virtual_path = filename.replace('\\', "/");

        tracing::debug!(
            "QUERY_PATH_INFO: path='{}', level=0x{:04X}",
            virtual_path,
            info_level
        );

        // Get share root
        let share_root = {
            let state = state.read().await;
            state
                .trees
                .get(&request.header.tid)
                .map(|t| t.virtual_root.clone())
                .unwrap_or_else(|| "/".to_string())
        };

        let full_path = build_vfs_path(&share_root, &virtual_path);

        // Get metadata via VFS
        let metadata = match self.vfs.metadata(&full_path).await {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!("QUERY_PATH_INFO failed: {} ({:?})", full_path, e);
                return self.error_response(request, status::STATUS_OBJECT_NAME_NOT_FOUND);
            }
        };

        // Format response based on info level
        let data = format_path_info(&metadata, info_level);

        self.build_trans2_response(request, vec![], data)
    }

    /// QUERY_FILE_INFO - Get open file metadata (MS-SMB 2.2.6.8)
    async fn handle_query_file_info(
        &self,
        request: &SmbMessage,
        state: &Arc<RwLock<SessionState>>,
        param_offset: usize,
    ) -> SmbMessage {
        // Calculate offset within request.data
        // param_offset is from start of SMB header (32 bytes)
        // Then: WordCount (1), Params (variable), ByteCount (2), then Data
        let word_count = (request.params.len() / 2) as usize;
        let data_start_offset = 32 + 1 + (word_count * 2) + 2;
        let relative_offset = param_offset.saturating_sub(data_start_offset);

        // Parameters in Trans2_Parameters:
        // - FID (2 bytes)
        // - InformationLevel (2 bytes)

        if request.data.len() < relative_offset + 4 {
            return self.error_response(request, status::STATUS_INVALID_SMB);
        }

        let fid = u16::from_le_bytes([
            request.data[relative_offset],
            request.data[relative_offset + 1],
        ]);
        let info_level = u16::from_le_bytes([
            request.data[relative_offset + 2],
            request.data[relative_offset + 3],
        ]);

        tracing::debug!(
            "QUERY_FILE_INFO: fid={}, level=0x{:04X}, param_offset={}, relative={}",
            fid,
            info_level,
            param_offset,
            relative_offset
        );

        // Get cached physical path from state (fast path)
        let (virtual_path, physical_path) = {
            let state = state.read().await;
            match state.files.get(&fid) {
                Some(f) => (f.virtual_path.clone(), f.physical_path.clone()),
                None => return self.error_response(request, status::STATUS_INVALID_HANDLE),
            }
        };

        tracing::debug!(
            "QUERY_FILE_INFO: fid={}, path='{}', level=0x{:04X}",
            fid,
            virtual_path,
            info_level
        );

        // Get metadata directly from filesystem (bypassing VFS for speed)
        let fs_meta = match tokio::fs::metadata(&physical_path).await {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!("QUERY_FILE_INFO failed: {:?}", e);
                return self.error_response(request, status::STATUS_OBJECT_NAME_NOT_FOUND);
            }
        };

        // Convert to VfsMetadata format
        let metadata = crate::vfs::VfsMetadata {
            name: physical_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            size: fs_meta.len(),
            is_dir: fs_meta.is_dir(),
            read_only: true, // Shares are read-only
            modified: fs_meta.modified().ok(),
            created: fs_meta.created().ok(),
        };

        let data = format_path_info(&metadata, info_level);
        self.build_trans2_response(request, vec![], data)
    }

    /// Build a TRANS2 response message
    fn build_trans2_response(
        &self,
        request: &SmbMessage,
        params: Vec<u8>,
        data: Vec<u8>,
    ) -> SmbMessage {
        // TRANS2 response format (MS-SMB 2.2.6.1.2):
        // Word Count = 10 + SetupCount
        // TotalParameterCount (2)
        // TotalDataCount (2)
        // Reserved1 (2)
        // ParameterCount (2)
        // ParameterOffset (2)
        // ParameterDisplacement (2)
        // DataCount (2)
        // DataOffset (2)
        // DataDisplacement (2)
        // SetupCount (1)
        // Reserved2 (1)

        let param_len = params.len() as u16;
        let data_len = data.len() as u16;

        // Calculate offsets (header=32, word_count=1, words=20, byte_count=2)
        let param_offset: u16 = 32 + 1 + 20 + 2; // 55
        let data_offset: u16 = param_offset + param_len;

        let mut response_params = Vec::with_capacity(20);
        response_params.extend_from_slice(&param_len.to_le_bytes()); // TotalParameterCount
        response_params.extend_from_slice(&data_len.to_le_bytes()); // TotalDataCount
        response_params.extend_from_slice(&0u16.to_le_bytes()); // Reserved1
        response_params.extend_from_slice(&param_len.to_le_bytes()); // ParameterCount
        response_params.extend_from_slice(&param_offset.to_le_bytes()); // ParameterOffset
        response_params.extend_from_slice(&0u16.to_le_bytes()); // ParameterDisplacement
        response_params.extend_from_slice(&data_len.to_le_bytes()); // DataCount
        response_params.extend_from_slice(&data_offset.to_le_bytes()); // DataOffset
        response_params.extend_from_slice(&0u16.to_le_bytes()); // DataDisplacement
        response_params.push(0); // SetupCount
        response_params.push(0); // Reserved2

        // Combine params and data
        let mut response_data = params;
        response_data.extend_from_slice(&data);

        SmbMessage {
            header: SmbHeader::new_response(&request.header, status::STATUS_SUCCESS),
            params: response_params,
            data: response_data,
        }
    }

    /// FIND_CLOSE2 - Close search handle
    async fn handle_find_close2(
        &self,
        request: &SmbMessage,
        state: &Arc<RwLock<SessionState>>,
    ) -> SmbMessage {
        if request.params.len() >= 2 {
            let sid = u16::from_le_bytes([request.params[0], request.params[1]]);
            let mut state = state.write().await;
            state.searches.remove(&sid);
        }

        SmbMessage {
            header: SmbHeader::new_response(&request.header, status::STATUS_SUCCESS),
            params: vec![],
            data: vec![],
        }
    }

    /// ECHO - Keep-alive / ping
    async fn handle_echo(&self, request: &SmbMessage) -> SmbMessage {
        // Echo back the same data
        SmbMessage {
            header: SmbHeader::new_response(&request.header, status::STATUS_SUCCESS),
            params: vec![0x01, 0x00], // SequenceNumber
            data: request.data.clone(),
        }
    }

    /// LOGOFF_ANDX - End session
    /// NT_TRANSACT - NT Transaction commands (MS-SMB 2.2.7)
    async fn handle_nt_transact(
        &self,
        request: &SmbMessage,
        _state: &Arc<RwLock<SessionState>>,
    ) -> SmbMessage {
        // NT_TRANSACT request format has function code in setup words
        // WordCount = 19 + SetupCount
        // Setup words start at offset 38 in params
        if request.params.len() < 40 {
            return self.error_response(request, status::STATUS_INVALID_SMB);
        }

        // Get SetupCount and Function (subcommand)
        let setup_count = request.params[36];
        if setup_count < 1 || request.params.len() < 40 {
            return self.error_response(request, status::STATUS_INVALID_SMB);
        }

        let function = u16::from_le_bytes([request.params[38], request.params[39]]);
        tracing::debug!("NT_TRANSACT: function=0x{:04X}", function);

        match function {
            nt_transact::NT_TRANSACT_QUERY_SECURITY_DESC => {
                // Return a minimal security descriptor allowing everyone read access
                // This is a stub - real implementation would check actual permissions
                self.handle_query_security_desc(request)
            }
            nt_transact::NT_TRANSACT_NOTIFY_CHANGE => {
                // Directory change notifications - not supported for read-only server
                // Return STATUS_NOT_SUPPORTED so client doesn't keep waiting
                self.error_response(request, status::STATUS_NOT_IMPLEMENTED)
            }
            _ => {
                tracing::debug!("NT_TRANSACT: unhandled function 0x{:04X}", function);
                self.error_response(request, status::STATUS_NOT_IMPLEMENTED)
            }
        }
    }

    /// Handle NT_TRANSACT_QUERY_SECURITY_DESC - Return minimal security descriptor
    fn handle_query_security_desc(&self, request: &SmbMessage) -> SmbMessage {
        // Build a minimal self-relative security descriptor
        // This grants Everyone read access (good enough for read-only share)
        //
        // SECURITY_DESCRIPTOR structure (self-relative):
        // - Revision (1 byte): 1
        // - Sbz1 (1 byte): 0
        // - Control (2 bytes): SE_SELF_RELATIVE (0x8000) | SE_DACL_PRESENT (0x0004)
        // - OffsetOwner (4 bytes): offset to owner SID
        // - OffsetGroup (4 bytes): offset to group SID
        // - OffsetSacl (4 bytes): 0 (no SACL)
        // - OffsetDacl (4 bytes): offset to DACL
        // Then: DACL, Owner SID, Group SID

        // Minimal security descriptor with Everyone (S-1-1-0) having read access
        let mut sd = Vec::new();

        // Header (20 bytes)
        sd.push(1); // Revision
        sd.push(0); // Sbz1
        sd.extend_from_slice(&0x8004u16.to_le_bytes()); // Control: SE_SELF_RELATIVE | SE_DACL_PRESENT
        sd.extend_from_slice(&48u32.to_le_bytes()); // OffsetOwner (after DACL)
        sd.extend_from_slice(&60u32.to_le_bytes()); // OffsetGroup (after Owner)
        sd.extend_from_slice(&0u32.to_le_bytes()); // OffsetSacl (none)
        sd.extend_from_slice(&20u32.to_le_bytes()); // OffsetDacl (right after header)

        // DACL (28 bytes at offset 20)
        // ACL header (8 bytes)
        sd.push(2); // AclRevision
        sd.push(0); // Sbz1
        sd.extend_from_slice(&28u16.to_le_bytes()); // AclSize (8 header + 20 ACE)
        sd.extend_from_slice(&1u16.to_le_bytes()); // AceCount
        sd.extend_from_slice(&0u16.to_le_bytes()); // Sbz2

        // ACCESS_ALLOWED_ACE for Everyone (S-1-1-0) with read access
        // ACE header (4 bytes) + Mask (4 bytes) + SID (8 bytes) = 16 bytes... wait
        // Actually ACE is: Type(1) + Flags(1) + Size(2) + Mask(4) + SID
        // Everyone SID (S-1-1-0) = 8 bytes
        sd.push(0); // AceType: ACCESS_ALLOWED_ACE_TYPE
        sd.push(0); // AceFlags
        sd.extend_from_slice(&20u16.to_le_bytes()); // AceSize (4 header + 4 mask + 12 SID)
        sd.extend_from_slice(&0x001200A9u32.to_le_bytes()); // AccessMask: READ_CONTROL | SYNCHRONIZE | FILE_READ_*

        // Everyone SID S-1-1-0 (12 bytes with padding for alignment)
        sd.push(1); // Revision
        sd.push(1); // SubAuthorityCount
        sd.extend_from_slice(&[0, 0, 0, 0, 0, 1]); // IdentifierAuthority (WORLD)
        sd.extend_from_slice(&0u32.to_le_bytes()); // SubAuthority[0] = 0

        // Owner SID: S-1-5-32-544 (Administrators) - 16 bytes at offset 48
        sd.push(1); // Revision
        sd.push(2); // SubAuthorityCount
        sd.extend_from_slice(&[0, 0, 0, 0, 0, 5]); // IdentifierAuthority (NT)
        sd.extend_from_slice(&32u32.to_le_bytes()); // SubAuthority[0] = BUILTIN
        sd.extend_from_slice(&544u32.to_le_bytes()); // SubAuthority[1] = Administrators

        // Group SID: S-1-5-32-544 (Administrators) - 16 bytes at offset 64
        sd.push(1); // Revision
        sd.push(2); // SubAuthorityCount
        sd.extend_from_slice(&[0, 0, 0, 0, 0, 5]); // IdentifierAuthority (NT)
        sd.extend_from_slice(&32u32.to_le_bytes()); // SubAuthority[0] = BUILTIN
        sd.extend_from_slice(&544u32.to_le_bytes()); // SubAuthority[1] = Administrators

        // Build NT_TRANSACT response
        // Response params: just the security descriptor length (4 bytes)
        let mut trans_params = Vec::new();
        trans_params.extend_from_slice(&(sd.len() as u32).to_le_bytes());

        // Build response parameters (38 bytes for NT_TRANSACT response)
        let param_offset = 32 + 1 + 36 + 2; // SMB header + wordcount + params + bytecount = 71
        let data_offset = param_offset + trans_params.len();

        let mut params = Vec::new();
        params.extend_from_slice(&[0u8; 3]); // Reserved
        params.extend_from_slice(&(trans_params.len() as u32).to_le_bytes()); // TotalParamCount
        params.extend_from_slice(&(sd.len() as u32).to_le_bytes()); // TotalDataCount
        params.extend_from_slice(&(trans_params.len() as u32).to_le_bytes()); // ParamCount
        params.extend_from_slice(&(param_offset as u32).to_le_bytes()); // ParamOffset
        params.extend_from_slice(&0u32.to_le_bytes()); // ParamDisplacement
        params.extend_from_slice(&(sd.len() as u32).to_le_bytes()); // DataCount
        params.extend_from_slice(&(data_offset as u32).to_le_bytes()); // DataOffset
        params.extend_from_slice(&0u32.to_le_bytes()); // DataDisplacement
        params.push(0); // SetupCount

        // Data section: trans_params + security descriptor
        let mut data = trans_params;
        data.extend_from_slice(&sd);

        SmbMessage {
            header: SmbHeader::new_response(&request.header, status::STATUS_SUCCESS),
            params,
            data,
        }
    }

    async fn handle_logoff(
        &self,
        request: &SmbMessage,
        state: &Arc<RwLock<SessionState>>,
    ) -> SmbMessage {
        {
            let mut state = state.write().await;
            state.uid = 0;
            state.trees.clear();
            state.files.clear();
            state.searches.clear();
        }

        let params = vec![
            0xFF, // AndXCommand: none
            0x00, // Reserved
            0x00, 0x00, // AndXOffset
        ];

        SmbMessage {
            header: SmbHeader::new_response(&request.header, status::STATUS_SUCCESS),
            params,
            data: vec![],
        }
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    fn error_response(&self, request: &SmbMessage, status: u32) -> SmbMessage {
        SmbMessage {
            header: SmbHeader::new_response(&request.header, status),
            params: vec![],
            data: vec![],
        }
    }

    /// Fast path: read directly from physical path (no VFS lookup)
    async fn read_file_direct(
        physical_path: &std::path::Path,
        offset: u64,
        max_len: usize,
    ) -> io::Result<Vec<u8>> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        let mut file = tokio::fs::File::open(physical_path).await?;
        file.seek(std::io::SeekFrom::Start(offset)).await?;

        // Cap at 64KB per read for SMB1 compatibility
        let read_size = max_len.min(65536);
        let mut buffer = vec![0u8; read_size];
        let bytes_read = file.read(&mut buffer).await?;
        buffer.truncate(bytes_read);

        Ok(buffer)
    }
}

#[async_trait]
impl ProtocolServer for SmbServer {
    fn name(&self) -> &'static str {
        "SMB"
    }

    async fn start(&self) -> anyhow::Result<()> {
        if !self.config.enabled {
            tracing::info!("SMB server is disabled in configuration");
            return Ok(());
        }

        let addr = format!("{}:{}", self.config.bind_address, self.config.port);
        tracing::info!("Starting SMB server on {}", addr);

        let listener = TcpListener::bind(&addr).await?;
        self.running.store(true, Ordering::SeqCst);

        let server = Arc::new(self.clone());
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok((stream, addr)) => {
                                let server = Arc::clone(&server);
                                tokio::spawn(async move {
                                    server.handle_connection(stream, addr).await;
                                });
                            }
                            Err(e) => {
                                tracing::error!("SMB accept error: {}", e);
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        tracing::info!("SMB server shutting down");
                        break;
                    }
                }
            }
        });

        tracing::info!("SMB server listening on port {}", self.config.port);
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        self.running.store(false, Ordering::SeqCst);
        self.shutdown_tx.send(true)?;
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

// Can't derive Clone for SmbServer due to watch channels, implement manually
impl Clone for SmbServer {
    fn clone(&self) -> Self {
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        Self {
            config: self.config.clone(),
            vfs: self.vfs.clone(),
            server_name: self.server_name.clone(),
            running: AtomicBool::new(false),
            shutdown_tx,
            shutdown_rx,
        }
    }
}

// =============================================================================
// Utility Functions
// =============================================================================

fn parse_dialect_strings(data: &[u8]) -> Vec<String> {
    let mut dialects = Vec::new();
    let mut start = 0;

    for (i, &byte) in data.iter().enumerate() {
        if byte == 0x02 {
            // Dialect marker
            start = i + 1;
        } else if byte == 0x00 && start > 0 {
            // End of string
            if let Ok(s) = std::str::from_utf8(&data[start..i]) {
                dialects.push(s.to_string());
            }
            start = 0;
        }
    }

    dialects
}

fn parse_unicode_string(data: &[u8]) -> String {
    let mut chars = Vec::new();
    let mut i = 0;
    while i + 1 < data.len() {
        let c = u16::from_le_bytes([data[i], data[i + 1]]);
        if c == 0 {
            break;
        }
        chars.push(c);
        i += 2;
    }
    String::from_utf16_lossy(&chars)
}

fn parse_ascii_string(data: &[u8]) -> String {
    data.iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect()
}

/// Simple wildcard matching for SMB search patterns
/// Supports * (any chars) and ? (single char)
fn match_wildcard(pattern: &str, name: &str) -> bool {
    // Handle common patterns quickly
    if pattern == "*" || pattern == "*.*" {
        return true;
    }

    let pattern = pattern.to_lowercase();
    let name = name.to_lowercase();

    let mut p_chars = pattern.chars().peekable();
    let mut n_chars = name.chars().peekable();

    match_wildcard_recursive(&mut p_chars, &mut n_chars)
}

fn match_wildcard_recursive(
    pattern: &mut std::iter::Peekable<std::str::Chars>,
    name: &mut std::iter::Peekable<std::str::Chars>,
) -> bool {
    loop {
        match (pattern.peek(), name.peek()) {
            (None, None) => return true,
            (None, Some(_)) => return false,
            (Some('*'), _) => {
                pattern.next();
                // Try matching * with 0, 1, 2, ... chars
                let mut name_clone = name.clone();
                loop {
                    let mut pattern_clone = pattern.clone();
                    if match_wildcard_recursive(&mut pattern_clone, &mut name_clone.clone()) {
                        return true;
                    }
                    if name_clone.next().is_none() {
                        break;
                    }
                }
                return false;
            }
            (Some('?'), Some(_)) => {
                pattern.next();
                name.next();
            }
            (Some(p), Some(n)) if *p == *n => {
                pattern.next();
                name.next();
            }
            _ => return false,
        }
    }
}

/// Convert SystemTime to Windows FILETIME (100ns intervals since 1601-01-01)
fn systemtime_to_filetime(time: Option<std::time::SystemTime>) -> u64 {
    time.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| (d.as_secs() + 11644473600) * 10_000_000 + d.subsec_nanos() as u64 / 100)
        .unwrap_or(0)
}

/// Format directory entries for FIND_FIRST2/FIND_NEXT2 response
/// Returns (data, entry_count, last_name_offset)
fn format_find_entries(
    entries: &[VfsDirEntry],
    info_level: u16,
    unicode: bool,
) -> (Vec<u8>, usize, usize) {
    let mut data = Vec::new();
    let mut count = 0;
    let mut last_offset = 0;

    // Common info levels:
    // 0x0001 = SMB_FIND_INFO_STANDARD
    // 0x0101 = SMB_FIND_FILE_DIRECTORY_INFO
    // 0x0102 = SMB_FIND_FILE_FULL_DIRECTORY_INFO
    // 0x0104 = SMB_FIND_FILE_BOTH_DIRECTORY_INFO (most common for XP)
    // 0x0605 = SMB_FIND_FILE_ID_BOTH_DIRECTORY_INFO (smbclient default)

    for (i, entry) in entries.iter().enumerate() {
        let entry_start = data.len();
        last_offset = entry_start;

        match info_level {
            0x0104 | 0x0605 => {
                // SMB_FIND_FILE_BOTH_DIRECTORY_INFO (MS-SMB 2.2.8.1.4)
                // 0x0605 = SMB_FIND_FILE_ID_BOTH_DIRECTORY_INFO (same format, we ignore FileId)
                format_both_directory_info(&mut data, entry, unicode, i == entries.len() - 1);
            }
            0x0102 => {
                // SMB_FIND_FILE_FULL_DIRECTORY_INFO
                format_full_directory_info(&mut data, entry, unicode, i == entries.len() - 1);
            }
            0x0101 => {
                // SMB_FIND_FILE_DIRECTORY_INFO
                format_directory_info(&mut data, entry, unicode, i == entries.len() - 1);
            }
            0x0001 => {
                // SMB_FIND_INFO_STANDARD (oldest format)
                format_info_standard(&mut data, entry, unicode, i == entries.len() - 1);
            }
            _ => {
                // Default to BOTH_DIRECTORY_INFO
                tracing::warn!(
                    "Unknown info level 0x{:04X}, using BOTH_DIRECTORY_INFO",
                    info_level
                );
                format_both_directory_info(&mut data, entry, unicode, i == entries.len() - 1);
            }
        }

        count += 1;
    }

    (data, count, last_offset)
}

/// SMB_FIND_FILE_BOTH_DIRECTORY_INFO format (94 bytes + filename)
fn format_both_directory_info(
    data: &mut Vec<u8>,
    entry: &VfsDirEntry,
    unicode: bool,
    is_last: bool,
) {
    let entry_start = data.len();

    // NextEntryOffset (4 bytes) - will be filled in after
    data.extend_from_slice(&[0u8; 4]);

    // FileIndex (4 bytes)
    data.extend_from_slice(&0u32.to_le_bytes());

    // CreationTime (8 bytes)
    let created = systemtime_to_filetime(entry.metadata.created);
    data.extend_from_slice(&created.to_le_bytes());

    // LastAccessTime (8 bytes)
    let modified = systemtime_to_filetime(entry.metadata.modified);
    data.extend_from_slice(&modified.to_le_bytes());

    // LastWriteTime (8 bytes)
    data.extend_from_slice(&modified.to_le_bytes());

    // ChangeTime (8 bytes)
    data.extend_from_slice(&modified.to_le_bytes());

    // EndOfFile (8 bytes)
    data.extend_from_slice(&entry.metadata.size.to_le_bytes());

    // AllocationSize (8 bytes) - round up to 4K
    let alloc_size = (entry.metadata.size + 4095) & !4095;
    data.extend_from_slice(&alloc_size.to_le_bytes());

    // ExtFileAttributes (4 bytes)
    let attrs: u32 = if entry.metadata.is_dir { 0x10 } else { 0x20 };
    data.extend_from_slice(&attrs.to_le_bytes());

    // FileNameLength (4 bytes)
    let name_bytes = encode_filename(&entry.name, unicode);
    data.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());

    // EaSize (4 bytes)
    data.extend_from_slice(&0u32.to_le_bytes());

    // ShortNameLength (1 byte)
    data.push(0);

    // Reserved (1 byte)
    data.push(0);

    // ShortName (24 bytes) - 8.3 format, usually empty
    data.extend_from_slice(&[0u8; 24]);

    // FileName (variable)
    data.extend_from_slice(&name_bytes);

    // Pad to 8-byte alignment
    while data.len() % 8 != 0 {
        data.push(0);
    }

    // Fill in NextEntryOffset
    if !is_last {
        let next_offset = (data.len() - entry_start) as u32;
        data[entry_start..entry_start + 4].copy_from_slice(&next_offset.to_le_bytes());
    }
}

/// SMB_FIND_FILE_FULL_DIRECTORY_INFO format
fn format_full_directory_info(
    data: &mut Vec<u8>,
    entry: &VfsDirEntry,
    unicode: bool,
    is_last: bool,
) {
    let entry_start = data.len();

    data.extend_from_slice(&[0u8; 4]); // NextEntryOffset
    data.extend_from_slice(&0u32.to_le_bytes()); // FileIndex

    let created = systemtime_to_filetime(entry.metadata.created);
    let modified = systemtime_to_filetime(entry.metadata.modified);
    data.extend_from_slice(&created.to_le_bytes()); // CreationTime
    data.extend_from_slice(&modified.to_le_bytes()); // LastAccessTime
    data.extend_from_slice(&modified.to_le_bytes()); // LastWriteTime
    data.extend_from_slice(&modified.to_le_bytes()); // ChangeTime
    data.extend_from_slice(&entry.metadata.size.to_le_bytes()); // EndOfFile

    let alloc_size = (entry.metadata.size + 4095) & !4095;
    data.extend_from_slice(&alloc_size.to_le_bytes()); // AllocationSize

    let attrs: u32 = if entry.metadata.is_dir { 0x10 } else { 0x20 };
    data.extend_from_slice(&attrs.to_le_bytes()); // ExtFileAttributes

    let name_bytes = encode_filename(&entry.name, unicode);
    data.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes()); // FileNameLength
    data.extend_from_slice(&0u32.to_le_bytes()); // EaSize
    data.extend_from_slice(&name_bytes); // FileName

    while data.len() % 8 != 0 {
        data.push(0);
    }

    if !is_last {
        let next_offset = (data.len() - entry_start) as u32;
        data[entry_start..entry_start + 4].copy_from_slice(&next_offset.to_le_bytes());
    }
}

/// SMB_FIND_FILE_DIRECTORY_INFO format
fn format_directory_info(data: &mut Vec<u8>, entry: &VfsDirEntry, unicode: bool, is_last: bool) {
    let entry_start = data.len();

    data.extend_from_slice(&[0u8; 4]); // NextEntryOffset
    data.extend_from_slice(&0u32.to_le_bytes()); // FileIndex

    let created = systemtime_to_filetime(entry.metadata.created);
    let modified = systemtime_to_filetime(entry.metadata.modified);
    data.extend_from_slice(&created.to_le_bytes());
    data.extend_from_slice(&modified.to_le_bytes());
    data.extend_from_slice(&modified.to_le_bytes());
    data.extend_from_slice(&modified.to_le_bytes());
    data.extend_from_slice(&entry.metadata.size.to_le_bytes());

    let alloc_size = (entry.metadata.size + 4095) & !4095;
    data.extend_from_slice(&alloc_size.to_le_bytes());

    let attrs: u32 = if entry.metadata.is_dir { 0x10 } else { 0x20 };
    data.extend_from_slice(&attrs.to_le_bytes());

    let name_bytes = encode_filename(&entry.name, unicode);
    data.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
    data.extend_from_slice(&name_bytes);

    while data.len() % 8 != 0 {
        data.push(0);
    }

    if !is_last {
        let next_offset = (data.len() - entry_start) as u32;
        data[entry_start..entry_start + 4].copy_from_slice(&next_offset.to_le_bytes());
    }
}

/// SMB_FIND_INFO_STANDARD format (oldest, for Win9x/XP)
fn format_info_standard(data: &mut Vec<u8>, entry: &VfsDirEntry, _unicode: bool, _is_last: bool) {
    // ResumeKey (4 bytes)
    data.extend_from_slice(&0u32.to_le_bytes());

    // CreationDate (2) + CreationTime (2)
    let (created_date, created_time) = systemtime_to_dos_datetime(entry.metadata.created);
    data.extend_from_slice(&created_date.to_le_bytes());
    data.extend_from_slice(&created_time.to_le_bytes());

    // LastAccessDate (2) + LastAccessTime (2)
    let (modified_date, modified_time) = systemtime_to_dos_datetime(entry.metadata.modified);
    data.extend_from_slice(&modified_date.to_le_bytes());
    data.extend_from_slice(&modified_time.to_le_bytes());

    // LastWriteDate (2) + LastWriteTime (2)
    data.extend_from_slice(&modified_date.to_le_bytes());
    data.extend_from_slice(&modified_time.to_le_bytes());

    // DataSize (4 bytes)
    data.extend_from_slice(&(entry.metadata.size as u32).to_le_bytes());

    // AllocationSize (4 bytes)
    let alloc_size = ((entry.metadata.size + 4095) & !4095) as u32;
    data.extend_from_slice(&alloc_size.to_le_bytes());

    // Attributes (2 bytes)
    let attrs: u16 = if entry.metadata.is_dir { 0x10 } else { 0x20 };
    data.extend_from_slice(&attrs.to_le_bytes());

    // FileNameLength (1 byte)
    let name_bytes = entry.name.as_bytes();
    data.push(name_bytes.len() as u8);

    // FileName (variable, ASCII)
    data.extend_from_slice(name_bytes);
}

/// Encode filename as Unicode or ASCII
fn encode_filename(name: &str, unicode: bool) -> Vec<u8> {
    if unicode {
        let mut bytes = Vec::with_capacity(name.len() * 2);
        for c in name.encode_utf16() {
            bytes.extend_from_slice(&c.to_le_bytes());
        }
        bytes
    } else {
        name.as_bytes().to_vec()
    }
}

use crate::vfs::VfsMetadata;

/// Convert SystemTime to DOS date/time format (for SMB_INFO_STANDARD)
fn systemtime_to_dos_datetime(time: Option<std::time::SystemTime>) -> (u16, u16) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let time = time.unwrap_or(SystemTime::UNIX_EPOCH);
    let secs = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Convert to DOS date/time
    // DOS date: bits 0-4 = day (1-31), bits 5-8 = month (1-12), bits 9-15 = year-1980
    // DOS time: bits 0-4 = seconds/2 (0-29), bits 5-10 = minutes (0-59), bits 11-15 = hours (0-23)

    // Simple conversion from Unix timestamp
    let days_since_1970 = secs / 86400;
    let time_of_day = secs % 86400;

    // Approximate year/month/day (not accounting for leap years perfectly, but good enough)
    let days_since_1980 = days_since_1970.saturating_sub(3652); // ~10 years
    let year = (days_since_1980 / 365).min(127) as u16;
    let day_of_year = days_since_1980 % 365;
    let month = ((day_of_year / 30) + 1).min(12) as u16;
    let day = ((day_of_year % 30) + 1).min(31) as u16;

    let hours = (time_of_day / 3600) as u16;
    let minutes = ((time_of_day % 3600) / 60) as u16;
    let seconds = ((time_of_day % 60) / 2) as u16; // DOS stores seconds/2

    let dos_date = day | (month << 5) | (year << 9);
    let dos_time = seconds | (minutes << 5) | (hours << 11);

    (dos_date, dos_time)
}

/// Format file/directory info for QUERY_PATH_INFO response
fn format_path_info(metadata: &VfsMetadata, info_level: u16) -> Vec<u8> {
    let mut data = Vec::new();

    // Common info levels:
    // 0x0000 = SMB_INFO_STANDARD (DOS date/time format)
    // 0x0001 = SMB_INFO_QUERY_EA_SIZE
    // 0x0101 = SMB_QUERY_FILE_BASIC_INFO
    // 0x0102 = SMB_QUERY_FILE_STANDARD_INFO
    // 0x0107 = SMB_QUERY_FILE_ALL_INFO

    match info_level {
        0x0000 | 0x0001 => {
            // SMB_INFO_STANDARD / SMB_INFO_QUERY_EA_SIZE (22 bytes, or 26 with EA)
            // Uses DOS date/time format (Windows 9x/XP compatibility)
            let (created_date, created_time) = systemtime_to_dos_datetime(metadata.created);
            let (modified_date, modified_time) = systemtime_to_dos_datetime(metadata.modified);

            data.extend_from_slice(&created_date.to_le_bytes());
            data.extend_from_slice(&created_time.to_le_bytes());
            data.extend_from_slice(&modified_date.to_le_bytes()); // LastAccessDate
            data.extend_from_slice(&modified_time.to_le_bytes()); // LastAccessTime
            data.extend_from_slice(&modified_date.to_le_bytes()); // LastWriteDate
            data.extend_from_slice(&modified_time.to_le_bytes()); // LastWriteTime
            data.extend_from_slice(&(metadata.size as u32).to_le_bytes()); // DataSize (32-bit)
            let alloc_size = ((metadata.size as u32) + 4095) & !4095;
            data.extend_from_slice(&alloc_size.to_le_bytes()); // AllocationSize (32-bit)
            let attrs: u16 = if metadata.is_dir { 0x10 } else { 0x20 };
            data.extend_from_slice(&attrs.to_le_bytes()); // Attributes (16-bit)

            if info_level == 0x0001 {
                // SMB_INFO_QUERY_EA_SIZE adds EaSize (4 bytes)
                data.extend_from_slice(&0u32.to_le_bytes());
            }
        }
        0x0101 => {
            // SMB_QUERY_FILE_BASIC_INFO (40 bytes)
            let created = systemtime_to_filetime(metadata.created);
            let modified = systemtime_to_filetime(metadata.modified);
            data.extend_from_slice(&created.to_le_bytes()); // CreationTime
            data.extend_from_slice(&modified.to_le_bytes()); // LastAccessTime
            data.extend_from_slice(&modified.to_le_bytes()); // LastWriteTime
            data.extend_from_slice(&modified.to_le_bytes()); // ChangeTime
            let attrs: u32 = if metadata.is_dir { 0x10 } else { 0x20 };
            data.extend_from_slice(&attrs.to_le_bytes()); // ExtFileAttributes
            data.extend_from_slice(&0u32.to_le_bytes()); // Reserved
        }
        0x0102 => {
            // SMB_QUERY_FILE_STANDARD_INFO (24 bytes)
            let alloc_size = (metadata.size + 4095) & !4095;
            data.extend_from_slice(&alloc_size.to_le_bytes()); // AllocationSize
            data.extend_from_slice(&metadata.size.to_le_bytes()); // EndOfFile
            data.extend_from_slice(&1u32.to_le_bytes()); // NumberOfLinks
            data.push(0); // DeletePending
            data.push(if metadata.is_dir { 1 } else { 0 }); // Directory
            data.extend_from_slice(&0u16.to_le_bytes()); // Reserved
        }
        0x0103 => {
            // SMB_QUERY_FILE_EA_INFO (4 bytes)
            data.extend_from_slice(&0u32.to_le_bytes()); // EaSize (no extended attributes)
        }
        0x0107 | _ => {
            // SMB_QUERY_FILE_ALL_INFO (default for unknown levels)
            let created = systemtime_to_filetime(metadata.created);
            let modified = systemtime_to_filetime(metadata.modified);

            // Basic info portion
            data.extend_from_slice(&created.to_le_bytes());
            data.extend_from_slice(&modified.to_le_bytes());
            data.extend_from_slice(&modified.to_le_bytes());
            data.extend_from_slice(&modified.to_le_bytes());
            let attrs: u32 = if metadata.is_dir { 0x10 } else { 0x20 };
            data.extend_from_slice(&attrs.to_le_bytes());
            data.extend_from_slice(&0u32.to_le_bytes()); // Reserved

            // Standard info portion
            let alloc_size = (metadata.size + 4095) & !4095;
            data.extend_from_slice(&alloc_size.to_le_bytes());
            data.extend_from_slice(&metadata.size.to_le_bytes());
            data.extend_from_slice(&1u32.to_le_bytes()); // NumberOfLinks
            data.push(0); // DeletePending
            data.push(if metadata.is_dir { 1 } else { 0 }); // Directory
            data.extend_from_slice(&0u16.to_le_bytes()); // Reserved

            // EA info
            data.extend_from_slice(&0u32.to_le_bytes()); // EaSize

            // Name info
            let name_bytes = encode_filename(&metadata.name, true);
            data.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(&name_bytes);
        }
    }

    data
}
