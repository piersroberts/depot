//! FTP server implementation using libunftp
//!
//! Provides FTP access to the virtual filesystem with support for:
//! - Anonymous access
//! - User authentication
//! - Passive mode (important for retro clients behind NAT)
//! - Classic LIST format for maximum compatibility

use crate::config::{FtpConfig, User};
use crate::vfs::{SharedVfs, VfsError};
use async_trait::async_trait;
use libunftp::auth::{AuthenticationError, Authenticator, Credentials, DefaultUser};
use libunftp::storage::{Error, ErrorKind, Fileinfo, Metadata, Result, StorageBackend};
use std::collections::HashMap;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::io::AsyncRead;

use super::ProtocolServer;

/// FTP server wrapper
pub struct FtpServer {
    config: FtpConfig,
    users: HashMap<String, User>,
    vfs: SharedVfs,
    running: AtomicBool,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
}

impl FtpServer {
    pub fn new(config: FtpConfig, users: HashMap<String, User>, vfs: SharedVfs) -> Arc<Self> {
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        Arc::new(Self {
            config,
            users,
            vfs,
            running: AtomicBool::new(false),
            shutdown_tx,
            shutdown_rx,
        })
    }

    /// Create the libunftp server instance
    fn create_server(&self) -> anyhow::Result<libunftp::Server<VfsStorageBackend, DefaultUser>> {
        let vfs = self.vfs.clone();
        // Leak the banner string to get a 'static lifetime - it lives for the server's lifetime anyway
        let banner: &'static str = Box::leak(self.config.banner.clone().into_boxed_str());
        let allow_anonymous = self.config.anonymous;
        let users = self.users.clone();
        let passive_start = self.config.passive_port_start;
        let passive_end = self.config.passive_port_end;
        
        let server = libunftp::ServerBuilder::with_authenticator(
            Box::new(move || VfsStorageBackend::new(vfs.clone())),
            Arc::new(VfsAuthenticator::new(allow_anonymous, users)),
        )
        .greeting(banner)
        .passive_ports(passive_start..passive_end)
        .build()?;
        
        Ok(server)
    }
}

#[async_trait]
impl ProtocolServer for FtpServer {
    fn name(&self) -> &'static str {
        "FTP"
    }

    async fn start(&self) -> anyhow::Result<()> {
        if !self.config.enabled {
            tracing::info!("FTP server is disabled in configuration");
            return Ok(());
        }

        let addr = format!("{}:{}", self.config.bind_address, self.config.port);
        tracing::info!("Starting FTP server on {}", addr);

        let server = self.create_server()?;
        self.running.store(true, Ordering::SeqCst);

        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            tokio::select! {
                result = server.listen(addr) => {
                    if let Err(e) = result {
                        tracing::error!("FTP server error: {}", e);
                    }
                }
                _ = shutdown_rx.changed() => {
                    tracing::info!("FTP server shutting down");
                }
            }
        });

        tracing::info!(
            "FTP server listening on port {} (passive ports {}-{})",
            self.config.port,
            self.config.passive_port_start,
            self.config.passive_port_end
        );

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

/// Storage backend that bridges libunftp to our VFS
pub struct VfsStorageBackend {
    vfs: SharedVfs,
}

impl std::fmt::Debug for VfsStorageBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VfsStorageBackend").finish()
    }
}

impl VfsStorageBackend {
    pub fn new(vfs: SharedVfs) -> Self {
        Self { vfs }
    }
}

/// Metadata wrapper for VFS metadata
#[derive(Debug)]
pub struct VfsFileMetadata {
    size: u64,
    is_dir: bool,
    modified: Option<SystemTime>,
    // uid/gid not applicable for our VFS
}

impl Metadata for VfsFileMetadata {
    fn len(&self) -> u64 {
        self.size
    }

    fn is_dir(&self) -> bool {
        self.is_dir
    }

    fn is_file(&self) -> bool {
        !self.is_dir
    }

    fn is_symlink(&self) -> bool {
        false
    }

    fn modified(&self) -> Result<SystemTime> {
        self.modified
            .ok_or_else(|| Error::new(ErrorKind::LocalError, "No modification time"))
    }

    fn gid(&self) -> u32 {
        0
    }

    fn uid(&self) -> u32 {
        0
    }
}

fn vfs_error_to_storage_error(e: VfsError) -> Error {
    match e {
        VfsError::NotFound(_) => Error::new(ErrorKind::PermanentFileNotAvailable, e),
        VfsError::PermissionDenied(_) => Error::new(ErrorKind::PermissionDenied, e),
        VfsError::IsDirectory(_) => Error::new(ErrorKind::PermanentFileNotAvailable, e),
        VfsError::NotADirectory(_) => Error::new(ErrorKind::PermanentFileNotAvailable, e),
        VfsError::Io(io_err) => Error::new(ErrorKind::LocalError, io_err),
        VfsError::InvalidPath(_) => Error::new(ErrorKind::PermanentFileNotAvailable, e),
    }
}

#[async_trait]
impl StorageBackend<DefaultUser> for VfsStorageBackend {
    type Metadata = VfsFileMetadata;

    fn supported_features(&self) -> u32 {
        // Basic features only - no SITEMD5, etc.
        0
    }

    async fn metadata<P: AsRef<Path> + Send + Debug>(
        &self,
        _user: &DefaultUser,
        path: P,
    ) -> Result<Self::Metadata> {
        let path_str = path.as_ref().to_string_lossy();
        let meta = self
            .vfs
            .metadata(&path_str)
            .await
            .map_err(vfs_error_to_storage_error)?;

        Ok(VfsFileMetadata {
            size: meta.size,
            is_dir: meta.is_dir,
            modified: meta.modified,
        })
    }

    async fn list<P: AsRef<Path> + Send + Debug>(
        &self,
        _user: &DefaultUser,
        path: P,
    ) -> Result<Vec<Fileinfo<PathBuf, Self::Metadata>>>
    where
        Self::Metadata: Metadata,
    {
        let path_str = path.as_ref().to_string_lossy();
        let entries = self
            .vfs
            .list_dir(&path_str)
            .await
            .map_err(vfs_error_to_storage_error)?;

        let fileinfos: Vec<_> = entries
            .into_iter()
            .map(|entry| Fileinfo {
                path: PathBuf::from(&entry.name),
                metadata: VfsFileMetadata {
                    size: entry.metadata.size,
                    is_dir: entry.metadata.is_dir,
                    modified: entry.metadata.modified,
                },
            })
            .collect();

        Ok(fileinfos)
    }

    async fn get<P: AsRef<Path> + Send + Debug>(
        &self,
        _user: &DefaultUser,
        path: P,
        start_pos: u64,
    ) -> Result<Box<dyn AsyncRead + Send + Sync + Unpin>> {
        let path_str = path.as_ref().to_string_lossy();
        let physical_path = self
            .vfs
            .resolve_path(&path_str)
            .map_err(vfs_error_to_storage_error)?;

        let mut file = tokio::fs::File::open(&physical_path)
            .await
            .map_err(|e| Error::new(ErrorKind::LocalError, e))?;

        if start_pos > 0 {
            use tokio::io::AsyncSeekExt;
            file.seek(std::io::SeekFrom::Start(start_pos))
                .await
                .map_err(|e| Error::new(ErrorKind::LocalError, e))?;
        }

        Ok(Box::new(file))
    }

    async fn put<
        P: AsRef<Path> + Send + Debug,
        R: AsyncRead + Send + Sync + Unpin + 'static,
    >(
        &self,
        _user: &DefaultUser,
        _input: R,
        _path: P,
        _start_pos: u64,
    ) -> Result<u64> {
        // Read-only for now
        Err(Error::new(
            ErrorKind::PermissionDenied,
            "Write access not permitted",
        ))
    }

    async fn del<P: AsRef<Path> + Send + Debug>(
        &self,
        _user: &DefaultUser,
        _path: P,
    ) -> Result<()> {
        Err(Error::new(
            ErrorKind::PermissionDenied,
            "Delete not permitted",
        ))
    }

    async fn mkd<P: AsRef<Path> + Send + Debug>(
        &self,
        _user: &DefaultUser,
        _path: P,
    ) -> Result<()> {
        Err(Error::new(
            ErrorKind::PermissionDenied,
            "Directory creation not permitted",
        ))
    }

    async fn rename<P: AsRef<Path> + Send + Debug>(
        &self,
        _user: &DefaultUser,
        _from: P,
        _to: P,
    ) -> Result<()> {
        Err(Error::new(
            ErrorKind::PermissionDenied,
            "Rename not permitted",
        ))
    }

    async fn rmd<P: AsRef<Path> + Send + Debug>(
        &self,
        _user: &DefaultUser,
        _path: P,
    ) -> Result<()> {
        Err(Error::new(
            ErrorKind::PermissionDenied,
            "Directory removal not permitted",
        ))
    }

    async fn cwd<P: AsRef<Path> + Send + Debug>(
        &self,
        _user: &DefaultUser,
        path: P,
    ) -> Result<()> {
        let path_str = path.as_ref().to_string_lossy();
        
        if self.vfs.is_dir(&path_str).await {
            Ok(())
        } else {
            Err(Error::new(
                ErrorKind::PermanentFileNotAvailable,
                "Not a directory",
            ))
        }
    }
}

/// Simple authenticator supporting anonymous and username/password
#[derive(Debug)]
struct VfsAuthenticator {
    allow_anonymous: bool,
    users: HashMap<String, User>,
}

impl VfsAuthenticator {
    fn new(allow_anonymous: bool, users: HashMap<String, User>) -> Self {
        Self {
            allow_anonymous,
            users,
        }
    }
}

#[async_trait]
impl Authenticator<DefaultUser> for VfsAuthenticator {
    async fn authenticate(
        &self,
        username: &str,
        creds: &Credentials,
    ) -> std::result::Result<DefaultUser, AuthenticationError> {
        // Check anonymous access
        if self.allow_anonymous
            && (username.eq_ignore_ascii_case("anonymous") || username.eq_ignore_ascii_case("ftp"))
        {
            return Ok(DefaultUser);
        }

        // Check user credentials - get password from credentials
        let password = match creds.password.as_ref() {
            Some(p) => p,
            None => return Err(AuthenticationError::BadPassword),
        };

        // Look up user and verify password using Argon2
        if let Some(user) = self.users.get(username) {
            if user.enabled && user.verify_password(password) {
                return Ok(DefaultUser);
            }
        }
        Err(AuthenticationError::BadPassword)
    }
}
