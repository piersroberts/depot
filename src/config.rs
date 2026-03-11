//! Configuration management for Depot file server
//!
//! Loads and manages server configuration from JSON files.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;

/// Root configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Server identification
    #[serde(default = "default_server_name")]
    pub server_name: String,

    /// Shared directories configuration (key is share name)
    pub shares: HashMap<String, ShareConfig>,

    /// User accounts with share access grants (key is username)
    #[serde(default)]
    pub users: HashMap<String, User>,

    /// Protocol-specific settings
    #[serde(default)]
    pub protocols: ProtocolsConfig,

    /// Admin panel settings
    #[serde(default)]
    pub admin: AdminConfig,

    /// Logging level
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_server_name() -> String {
    "Depot".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

/// Configuration for a shared directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareConfig {
    /// Local filesystem path
    pub path: PathBuf,

    /// Virtual path (how it appears to clients)
    /// e.g., "/games" would make files accessible at /games/...
    #[serde(default = "default_virtual_path")]
    pub virtual_path: String,

    /// Whether this share is read-only
    #[serde(default = "default_true")]
    pub read_only: bool,

    /// Optional description
    pub description: Option<String>,

    /// Whether this share is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_virtual_path() -> String {
    "/".to_string()
}

fn default_true() -> bool {
    true
}

/// User account with share access grants
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// Argon2 hashed password
    pub password_hash: String,

    /// List of share names this user can access
    /// Empty list means no access, use "*" for access to all shares
    #[serde(default)]
    pub shares: Vec<String>,

    /// Optional description or display name
    #[serde(default)]
    pub description: Option<String>,

    /// Whether this user account is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl User {
    /// Create a new user with a plaintext password (will be hashed)
    pub fn new(password: &str, shares: Vec<String>) -> anyhow::Result<Self> {
        let password_hash = hash_password(password)?;
        Ok(Self {
            password_hash,
            shares,
            description: None,
            enabled: true,
        })
    }

    /// Verify a password against this user's stored hash
    pub fn verify_password(&self, password: &str) -> bool {
        verify_password(password, &self.password_hash)
    }

    /// Check if user has access to a specific share
    #[allow(dead_code)]
    pub fn has_access_to(&self, share_name: &str) -> bool {
        if !self.enabled {
            return false;
        }
        self.shares.iter().any(|s| s == "*" || s == share_name)
    }
}

/// Hash a password using Argon2
pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Failed to hash password: {}", e))?;
    Ok(hash.to_string())
}

/// Verify a password against an Argon2 hash
pub fn verify_password(password: &str, hash: &str) -> bool {
    let parsed_hash = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

/// Protocol-specific configurations
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProtocolsConfig {
    #[serde(default)]
    pub ftp: FtpConfig,

    #[serde(default)]
    pub http: HttpConfig,

    #[serde(default)]
    pub smb: SmbConfig,

    // Future protocol support
    // pub appleshare: Option<AppleShareConfig>,
}

/// FTP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FtpConfig {
    /// Whether FTP is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Listen address
    #[serde(default = "default_bind_address")]
    pub bind_address: IpAddr,

    /// Listen port
    #[serde(default = "default_ftp_port")]
    pub port: u16,

    /// Passive mode port range start
    #[serde(default = "default_passive_start")]
    pub passive_port_start: u16,

    /// Passive mode port range end
    #[serde(default = "default_passive_end")]
    pub passive_port_end: u16,

    /// Allow anonymous access (disabled by default for security)
    #[serde(default)]
    pub anonymous: bool,

    /// Welcome message
    #[serde(default = "default_ftp_banner")]
    pub banner: String,
}

impl Default for FtpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind_address: default_bind_address(),
            port: default_ftp_port(),
            passive_port_start: default_passive_start(),
            passive_port_end: default_passive_end(),
            anonymous: false, // Disabled by default for security
            banner: default_ftp_banner(),
        }
    }
}

fn default_bind_address() -> IpAddr {
    "0.0.0.0".parse().unwrap()
}

fn default_ftp_port() -> u16 {
    2121 // Non-privileged port by default
}

fn default_passive_start() -> u16 {
    60000
}

fn default_passive_end() -> u16 {
    60100
}

fn default_ftp_banner() -> String {
    "Welcome to Depot FTP Server".to_string()
}

/// SMB/CIFS server configuration (Windows file sharing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmbConfig {
    /// Whether SMB is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Listen address
    #[serde(default = "default_bind_address")]
    pub bind_address: IpAddr,

    /// Listen port (445 for direct SMB, 139 for NetBIOS)
    #[serde(default = "default_smb_port")]
    pub port: u16,

    /// Server name broadcast on the network
    #[serde(default = "default_smb_netbios_name")]
    pub netbios_name: String,

    /// Workgroup/domain name
    #[serde(default = "default_smb_workgroup")]
    pub workgroup: String,

    /// Allow guest access (no authentication required)
    #[serde(default = "default_true")]
    pub guest_access: bool,
}

impl Default for SmbConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default - requires privilege for port 445
            bind_address: default_bind_address(),
            port: default_smb_port(),
            netbios_name: default_smb_netbios_name(),
            workgroup: default_smb_workgroup(),
            guest_access: true,
        }
    }
}

fn default_smb_port() -> u16 {
    4450 // Non-privileged by default; use 445 for standard SMB
}

fn default_smb_netbios_name() -> String {
    "DEPOT".to_string()
}

fn default_smb_workgroup() -> String {
    "WORKGROUP".to_string()
}

/// HTTP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    /// Whether HTTP is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Listen address
    #[serde(default = "default_bind_address")]
    pub bind_address: IpAddr,

    /// Listen port
    #[serde(default = "default_http_port")]
    pub port: u16,

    /// Require authentication (uses configured users)
    #[serde(default)]
    pub require_auth: bool,

    /// Use simple HTML for retro browser compatibility
    #[serde(default = "default_true")]
    pub retro_compatible: bool,

    /// Show file sizes in listings
    #[serde(default = "default_true")]
    pub show_file_sizes: bool,

    /// Show modification dates in listings
    #[serde(default = "default_true")]
    pub show_dates: bool,

    /// Built-in theme: "retro", "modern", "terminal", "paperwhite", "ocean", "midnight"
    #[serde(default = "default_theme")]
    pub theme: String,

    /// Custom footer message
    #[serde(default)]
    pub footer_message: Option<String>,

    /// Custom CSS (optional, overrides theme)
    pub custom_css: Option<String>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind_address: default_bind_address(),
            port: default_http_port(),
            require_auth: false,
            retro_compatible: true,
            show_file_sizes: true,
            show_dates: true,
            theme: default_theme(),
            footer_message: None,
            custom_css: None,
        }
    }
}

fn default_http_port() -> u16 {
    8080
}

fn default_theme() -> String {
    "modern".to_string()
}

/// Admin panel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminConfig {
    /// Whether admin panel is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Admin panel port (separate from file serving)
    #[serde(default = "default_admin_port")]
    pub port: u16,

    /// Admin username
    #[serde(default = "default_admin_user")]
    pub username: String,

    /// Admin password (should be changed!)
    #[serde(default = "default_admin_pass")]
    pub password: String,

    /// Bind address for admin panel
    #[serde(default = "default_admin_bind")]
    pub bind_address: IpAddr,
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_admin_port(),
            username: default_admin_user(),
            password: default_admin_pass(),
            bind_address: default_admin_bind(),
        }
    }
}

fn default_admin_port() -> u16 {
    8888
}

fn default_admin_user() -> String {
    "admin".to_string()
}

fn default_admin_pass() -> String {
    "depot".to_string()
}

fn default_admin_bind() -> IpAddr {
    "127.0.0.1".parse().unwrap() // Admin only on localhost by default
}

impl Config {
    /// Load configuration from a TOML file
    pub fn load(path: &PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Load from default location or create default config
    #[allow(dead_code)]
    pub fn load_or_default() -> anyhow::Result<Self> {
        let config_path = Self::default_config_path();
        
        if config_path.exists() {
            Self::load(&config_path)
        } else {
            Ok(Self::default())
        }
    }

    /// Get the default config file path
    pub fn default_config_path() -> PathBuf {
        PathBuf::from("depot.toml")
    }

    /// Validate the configuration
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.shares.is_empty() {
            anyhow::bail!("At least one share must be configured");
        }

        for (name, share) in &self.shares {
            if !share.path.exists() {
                tracing::warn!(
                    "Share '{}' path does not exist: {}",
                    name,
                    share.path.display()
                );
            }
        }

        // Security: refuse to start with default admin credentials
        if self.admin.enabled {
            if self.admin.password == "depot" {
                anyhow::bail!(
                    "Security error: Admin panel is enabled with default password.\n\
                     Please set a secure password in your config file:\n\
                     [admin]\n\
                     password = \"your-secure-password-here\""
                );
            }
            if self.admin.username == "admin" && self.admin.password.len() < 8 {
                tracing::warn!(
                    "Security warning: Admin username is 'admin' with a short password. \
                     Consider using stronger credentials."
                );
            }
        }

        Ok(())
    }

    /// Save configuration to a TOML file
    pub fn save(&self, path: &PathBuf) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Find a user by username
    #[allow(dead_code)]
    pub fn find_user(&self, username: &str) -> Option<&User> {
        self.users.get(username).filter(|u| u.enabled)
    }

    /// Authenticate a user by username and password
    #[allow(dead_code)]
    pub fn authenticate_user(&self, username: &str, password: &str) -> Option<&User> {
        self.find_user(username)
            .filter(|user| user.verify_password(password))
    }

    /// Add a new user to the configuration
    pub fn add_user(&mut self, username: String, user: User) -> anyhow::Result<()> {
        if self.users.contains_key(&username) {
            anyhow::bail!("User '{}' already exists", username);
        }
        self.users.insert(username, user);
        Ok(())
    }

    /// Remove a user from the configuration
    pub fn remove_user(&mut self, username: &str) -> anyhow::Result<()> {
        self.users
            .remove(username)
            .ok_or_else(|| anyhow::anyhow!("User '{}' not found", username))?;
        Ok(())
    }

    /// Grant a user access to a share
    pub fn grant_share(&mut self, username: &str, share_name: &str) -> anyhow::Result<()> {
        let user = self
            .users
            .get_mut(username)
            .ok_or_else(|| anyhow::anyhow!("User '{}' not found", username))?;

        if !user.shares.contains(&share_name.to_string()) {
            user.shares.push(share_name.to_string());
        }
        Ok(())
    }

    /// Revoke a user's access to a share
    pub fn revoke_share(&mut self, username: &str, share_name: &str) -> anyhow::Result<()> {
        let user = self
            .users
            .get_mut(username)
            .ok_or_else(|| anyhow::anyhow!("User '{}' not found", username))?;

        user.shares.retain(|s| s != share_name);
        Ok(())
    }

    /// Get all share names from the config
    #[allow(dead_code)]
    pub fn share_names(&self) -> Vec<String> {
        self.shares.keys().cloned().collect()
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut shares = HashMap::new();
        shares.insert(
            "Public".to_string(),
            ShareConfig {
                path: PathBuf::from("."),
                virtual_path: "/".to_string(),
                read_only: true,
                description: Some("Default public share".to_string()),
                enabled: true,
            },
        );
        Self {
            server_name: default_server_name(),
            shares,
            users: HashMap::new(),
            protocols: ProtocolsConfig::default(),
            admin: AdminConfig::default(),
            log_level: default_log_level(),
        }
    }
}
