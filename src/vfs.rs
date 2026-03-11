//! Virtual Filesystem (VFS) abstraction
//!
//! Merges multiple physical directories into a unified virtual directory structure.
//! This allows sharing multiple folders as if they were one seamless filesystem.

use crate::config::ShareConfig;
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt::Debug;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs;
use tokio::io::AsyncRead;

/// Metadata for a virtual file or directory
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct VfsMetadata {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub read_only: bool,
}

/// A virtual directory entry
#[derive(Debug, Clone)]
pub struct VfsDirEntry {
    pub name: String,
    pub metadata: VfsMetadata,
    pub virtual_path: String,
}

/// Error types for VFS operations
#[derive(Debug, thiserror::Error)]
pub enum VfsError {
    #[error("Path not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Is a directory: {0}")]
    IsDirectory(String),

    #[error("Is not a directory: {0}")]
    NotADirectory(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Invalid path: {0}")]
    #[allow(dead_code)]
    InvalidPath(String),
}

pub type VfsResult<T> = Result<T, VfsError>;

/// Trait for protocol-agnostic filesystem operations
#[async_trait]
pub trait VirtualFilesystem: Send + Sync {
    /// List directory contents
    async fn list_dir(&self, path: &str) -> VfsResult<Vec<VfsDirEntry>>;

    /// Get metadata for a path
    async fn metadata(&self, path: &str) -> VfsResult<VfsMetadata>;

    /// Check if path exists
    #[allow(dead_code)]
    async fn exists(&self, path: &str) -> bool;

    /// Check if path is a directory
    async fn is_dir(&self, path: &str) -> bool;

    /// Open a file for reading
    #[allow(dead_code)]
    async fn open_read(&self, path: &str) -> VfsResult<Box<dyn AsyncRead + Send + Unpin>>;

    /// Get file size
    #[allow(dead_code)]
    async fn file_size(&self, path: &str) -> VfsResult<u64>;

    /// Resolve virtual path to physical path (for direct file serving)
    fn resolve_path(&self, virtual_path: &str) -> VfsResult<PathBuf>;
}

/// Mount point mapping virtual paths to physical directories
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct MountPoint {
    virtual_path: String,
    physical_path: PathBuf,
    name: String,
    read_only: bool,
    description: Option<String>,
}

/// The main VFS implementation that merges multiple shares
#[allow(dead_code)]
pub struct MergedVfs {
    mounts: Vec<MountPoint>,
    /// Quick lookup for root-level virtual directories
    root_entries: HashMap<String, MountPoint>,
}

impl MergedVfs {
    /// Create a new merged VFS from share configurations
    pub fn new(shares: &HashMap<String, ShareConfig>) -> Self {
        let mut mounts = Vec::new();
        let mut root_entries = HashMap::new();

        for (name, share) in shares.iter().filter(|(_, s)| s.enabled) {
            let virtual_path = normalize_virtual_path(&share.virtual_path);

            let mount = MountPoint {
                virtual_path: virtual_path.clone(),
                physical_path: share.path.clone(),
                name: name.clone(),
                read_only: share.read_only,
                description: share.description.clone(),
            };

            // Extract root directory name for quick lookup
            if let Some(root_name) = get_root_segment(&virtual_path) {
                root_entries.insert(root_name, mount.clone());
            }

            mounts.push(mount);
        }

        Self {
            mounts,
            root_entries,
        }
    }

    /// Find the mount point for a given virtual path
    /// Returns (mount_point, remainder_path) where remainder is the path within the mount
    fn find_mount(&self, virtual_path: &str) -> Option<(&MountPoint, String)> {
        let normalized = normalize_virtual_path(virtual_path);

        // Sort by virtual_path length descending to match most specific first
        let mut sorted_mounts: Vec<_> = self.mounts.iter().collect();
        sorted_mounts.sort_by(|a, b| b.virtual_path.len().cmp(&a.virtual_path.len()));

        for mount in sorted_mounts {
            if normalized == mount.virtual_path {
                return Some((mount, String::new()));
            }

            let prefix = if mount.virtual_path == "/" {
                "/".to_string()
            } else {
                format!("{}/", mount.virtual_path)
            };

            if normalized.starts_with(&prefix) || normalized == mount.virtual_path {
                let remainder = if mount.virtual_path == "/" {
                    normalized[1..].to_string()
                } else {
                    normalized.strip_prefix(&prefix).unwrap_or("").to_string()
                };
                return Some((mount, remainder));
            }
        }

        None
    }

    /// Convert physical metadata to VFS metadata
    async fn to_vfs_metadata(
        &self,
        name: &str,
        physical_path: &Path,
        read_only: bool,
    ) -> VfsResult<VfsMetadata> {
        let meta = fs::metadata(physical_path).await?;

        Ok(VfsMetadata {
            name: name.to_string(),
            is_dir: meta.is_dir(),
            size: meta.len(),
            modified: meta.modified().ok(),
            created: meta.created().ok(),
            read_only,
        })
    }
}

#[async_trait]
impl VirtualFilesystem for MergedVfs {
    async fn list_dir(&self, path: &str) -> VfsResult<Vec<VfsDirEntry>> {
        let normalized = normalize_virtual_path(path);

        // Root directory: list all mount points
        if normalized == "/" {
            let mut entries = Vec::new();
            let mut seen = std::collections::HashSet::new();

            for mount in &self.mounts {
                if let Some(root_name) = get_root_segment(&mount.virtual_path) {
                    if seen.insert(root_name.clone()) {
                        entries.push(VfsDirEntry {
                            name: root_name.clone(),
                            metadata: VfsMetadata {
                                name: root_name.clone(),
                                is_dir: true,
                                size: 0,
                                modified: None,
                                created: None,
                                read_only: mount.read_only,
                            },
                            virtual_path: format!("/{}", root_name),
                        });
                    }
                } else if mount.virtual_path == "/" {
                    // Mount at root - list its contents
                    let mut dir = fs::read_dir(&mount.physical_path).await?;
                    while let Some(entry) = dir.next_entry().await? {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if !seen.insert(name.clone()) {
                            continue;
                        }
                        let meta = self
                            .to_vfs_metadata(&name, &entry.path(), mount.read_only)
                            .await?;
                        entries.push(VfsDirEntry {
                            name: name.clone(),
                            metadata: meta,
                            virtual_path: format!("/{}", name),
                        });
                    }
                }
            }

            return Ok(entries);
        }

        // Find matching mount point
        match self.find_mount(&normalized) {
            Some((mount, remainder)) => {
                let physical_path = if remainder.is_empty() {
                    mount.physical_path.clone()
                } else {
                    mount.physical_path.join(&remainder)
                };

                if !physical_path.is_dir() {
                    return Err(VfsError::NotADirectory(normalized));
                }

                let mut entries = Vec::new();
                let mut dir = fs::read_dir(&physical_path).await?;

                while let Some(entry) = dir.next_entry().await? {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let meta = self
                        .to_vfs_metadata(&name, &entry.path(), mount.read_only)
                        .await?;

                    let entry_virtual_path =
                        format!("{}/{}", normalized.trim_end_matches('/'), name);

                    entries.push(VfsDirEntry {
                        name,
                        metadata: meta,
                        virtual_path: entry_virtual_path,
                    });
                }

                Ok(entries)
            }
            None => Err(VfsError::NotFound(normalized)),
        }
    }

    async fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        let normalized = normalize_virtual_path(path);

        if normalized == "/" {
            return Ok(VfsMetadata {
                name: "/".to_string(),
                is_dir: true,
                size: 0,
                modified: None,
                created: None,
                read_only: true,
            });
        }

        match self.find_mount(&normalized) {
            Some((mount, remainder)) => {
                let physical_path = if remainder.is_empty() {
                    mount.physical_path.clone()
                } else {
                    mount.physical_path.join(&remainder)
                };

                let name = physical_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| mount.name.clone());

                self.to_vfs_metadata(&name, &physical_path, mount.read_only)
                    .await
            }
            None => Err(VfsError::NotFound(normalized)),
        }
    }

    async fn exists(&self, path: &str) -> bool {
        self.metadata(path).await.is_ok()
    }

    async fn is_dir(&self, path: &str) -> bool {
        self.metadata(path).await.map(|m| m.is_dir).unwrap_or(false)
    }

    async fn open_read(&self, path: &str) -> VfsResult<Box<dyn AsyncRead + Send + Unpin>> {
        let physical_path = self.resolve_path(path)?;

        if physical_path.is_dir() {
            return Err(VfsError::IsDirectory(path.to_string()));
        }

        let file = fs::File::open(&physical_path).await?;
        Ok(Box::new(file))
    }

    async fn file_size(&self, path: &str) -> VfsResult<u64> {
        let meta = self.metadata(path).await?;
        Ok(meta.size)
    }

    fn resolve_path(&self, virtual_path: &str) -> VfsResult<PathBuf> {
        let normalized = normalize_virtual_path(virtual_path);

        match self.find_mount(&normalized) {
            Some((mount, remainder)) => {
                let physical_path = if remainder.is_empty() {
                    mount.physical_path.clone()
                } else {
                    mount.physical_path.join(&remainder)
                };

                // Security: ensure resolved path is within mount
                let canonical = physical_path
                    .canonicalize()
                    .map_err(|_| VfsError::NotFound(normalized.clone()))?;

                let mount_canonical = mount
                    .physical_path
                    .canonicalize()
                    .map_err(|_| VfsError::NotFound(normalized.clone()))?;

                if !canonical.starts_with(&mount_canonical) {
                    return Err(VfsError::PermissionDenied(
                        "Path traversal detected".to_string(),
                    ));
                }

                Ok(canonical)
            }
            None => Err(VfsError::NotFound(normalized)),
        }
    }
}

/// Normalize a virtual path (ensure leading slash, no trailing slash except for root)
fn normalize_virtual_path(path: &str) -> String {
    let mut normalized = path.trim().to_string();

    // Ensure leading slash
    if !normalized.starts_with('/') {
        normalized = format!("/{}", normalized);
    }

    // Remove trailing slash (except for root)
    while normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }

    // Collapse multiple slashes
    while normalized.contains("//") {
        normalized = normalized.replace("//", "/");
    }

    normalized
}

/// Get the first path segment after root
fn get_root_segment(path: &str) -> Option<String> {
    let normalized = normalize_virtual_path(path);
    if normalized == "/" {
        return None;
    }

    normalized
        .trim_start_matches('/')
        .split('/')
        .next()
        .map(|s| s.to_string())
}

/// A thread-safe wrapper around the VFS
pub type SharedVfs = Arc<dyn VirtualFilesystem>;

/// Create a shared VFS instance from configuration
pub fn create_vfs(shares: &HashMap<String, ShareConfig>) -> SharedVfs {
    Arc::new(MergedVfs::new(shares))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_virtual_path() {
        assert_eq!(normalize_virtual_path("/"), "/");
        assert_eq!(normalize_virtual_path(""), "/");
        assert_eq!(normalize_virtual_path("/foo"), "/foo");
        assert_eq!(normalize_virtual_path("/foo/"), "/foo");
        assert_eq!(normalize_virtual_path("foo"), "/foo");
        assert_eq!(normalize_virtual_path("/foo//bar"), "/foo/bar");
    }

    #[test]
    fn test_get_root_segment() {
        assert_eq!(get_root_segment("/"), None);
        assert_eq!(get_root_segment("/games"), Some("games".to_string()));
        assert_eq!(get_root_segment("/games/dos"), Some("games".to_string()));
    }
}
