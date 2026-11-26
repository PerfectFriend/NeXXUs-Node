//! Sync Folder Engine (Phase 2): Managed local sovereign drive folder (~/.nexxus/drive or ~/NeXXus).
//! Scans directory, digests files into 16 KB chunks, encodes into RS(4+2), seals with per-file key,
//! stores in CAS, and maintains local index mapping relative paths to manifest roots.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use nexxus_core::error::NexxusError;
use crate::net::network::P2pNetwork;
use crate::storage::ChunkStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedFile {
    pub relative_path: String,
    pub root_hex: String,
    pub size_bytes: u64,
    pub mtime_unix: u64,
    #[serde(default)]
    pub content_hash_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncReport {
    pub drive_path: String,
    pub scanned_files: usize,
    pub newly_synced: usize,
    pub total_bytes: u64,
    pub errors: Vec<String>,
}

pub struct SyncEngine {
    drive_dir: PathBuf,
    store: Arc<ChunkStore>,
    network: Arc<P2pNetwork>,
}

impl SyncEngine {
    pub fn new(drive_dir: PathBuf, store: Arc<ChunkStore>, network: Arc<P2pNetwork>) -> Self {
        Self { drive_dir, store, network }
    }

    pub fn drive_path(&self) -> &Path {
        &self.drive_dir
    }

    /// Scans the drive directory, identifies new/modified files, and syncs them to the vault.
    pub async fn scan_and_sync(&self) -> Result<SyncReport, NexxusError> {
        if !tokio::fs::try_exists(self.drive_path()).await
            .map_err(|e| NexxusError::Io(format!("stat drive dir: {e}")))?
        {
            tokio::fs::create_dir_all(self.drive_path()).await
                .map_err(|e| NexxusError::Io(format!("create drive dir: {e}")))?;
        }

        let canonical_base = tokio::fs::canonicalize(self.drive_path()).await
            .map_err(|e| NexxusError::Io(format!("canonicalize drive dir: {e}")))?;

        let mut scanned = 0usize;
        let mut newly_synced = 0usize;
        let mut total_bytes = 0u64;
        let mut errors = Vec::new();

        let entries = walk_dir_safe(&canonical_base, 0, 16).await;
        for path in entries {
            let meta = match tokio::fs::symlink_metadata(&path).await {
                Ok(m) => m,
                Err(e) => {
                    errors.push(format!("{}: metadata error {e}", path.display()));
                    continue;
                }
            };

            // H-24: Never sync symlinks
            if meta.file_type().is_symlink() || !meta.is_file() {
                continue;
            }

            scanned += 1;
            let size = meta.len();
            total_bytes += size;
            if size == 0 {
                continue;
            }

            let mtime_unix = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            let rel_path = path
                .strip_prefix(&canonical_base)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| path.file_name().unwrap_or_default().to_string_lossy().into_owned());

            // H-24: Detect changes by size, mtime, AND content hash
            let sync_key = format!("sync_idx_{rel_path}");
            let mut current_hash_hex = None;

            let needs_sync = match self.store.get_meta(&sync_key)? {
                Some(raw) => {
                    let prev: Result<SyncedFile, _> = serde_json::from_slice(&raw);
                    match prev {
                        Ok(p) => {
                            if p.size_bytes != size || p.mtime_unix != mtime_unix {
                                // Potential change: verify via content hash
                                let hash = compute_file_sha256(&path).await?;
                                current_hash_hex = Some(hash.clone());
                                p.content_hash_hex.is_empty() || p.content_hash_hex != hash
                            } else {
                                false
                            }
                        }
                        Err(_) => true,
                    }
                }
                None => true,
            };

            if needs_sync {
                let content_hash = match current_hash_hex {
                    Some(h) => h,
                    None => compute_file_sha256(&path).await?,
                };

                match crate::vault::upload_file(&self.network, &self.store, &path).await {
                    Ok(up) => {
                        let root_hex = up["root"].as_str().unwrap_or_default().to_string();
                        let synced = SyncedFile {
                            relative_path: rel_path.clone(),
                            root_hex,
                            size_bytes: size,
                            mtime_unix,
                            content_hash_hex: content_hash,
                        };
                        let raw = serde_json::to_vec(&synced).map_err(|e| NexxusError::Io(e.to_string()))?;
                        self.store.put_meta(&sync_key, &raw)?;
                        newly_synced += 1;
                    }
                    Err(e) => {
                        errors.push(format!("{}: sync upload error: {e}", path.display()));
                    }
                }
            }
        }

        Ok(SyncReport {
            drive_path: self.drive_dir.display().to_string(),
            scanned_files: scanned,
            newly_synced,
            total_bytes,
            errors,
        })
    }

    /// List all currently indexed files in this drive from the store metadata.
    pub fn list_synced(&self) -> Result<Vec<SyncedFile>, NexxusError> {
        let mut list = Vec::new();
        for (_k, v) in self.store.list_meta_prefix("sync_idx_")? {
            if let Ok(file) = serde_json::from_slice::<SyncedFile>(&v) {
                list.push(file);
            }
        }
        list.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        Ok(list)
    }

    /// Query the status of the sovereign drive folder and its indexed files.
    pub fn status(&self) -> Result<Value, NexxusError> {
        let files = self.list_synced()?;
        let total_size: u64 = files.iter().map(|f| f.size_bytes).sum();
        Ok(serde_json::json!({
            "drive_path": self.drive_dir.display().to_string(),
            "exists": self.drive_dir.exists(),
            "synced_files_count": files.len(),
            "total_bytes": total_size,
            "files": files,
        }))
    }
}

/// Returns the standard default Sovereign Drive directory (~/.nexxus/drive).
pub fn default_drive_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".nexxus").join("drive")
}

async fn compute_file_sha256(path: &Path) -> Result<String, NexxusError> {
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::File::open(path).await
        .map_err(|e| NexxusError::Io(format!("open for hash {}: {e}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf).await
            .map_err(|e| NexxusError::Io(format!("read for hash {}: {e}", path.display())))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// H-24: Safe directory traversal preventing symlink traversal, symlink escapes, and cycles.
/// Async: не блокирует tokio runtime (аудит 2026-11-23, sync без std::fs в async-путях).
fn walk_dir_safe<'a>(
    dir: &'a Path,
    depth: usize,
    max_depth: usize,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<PathBuf>> + Send + 'a>> {
    Box::pin(async move {
    if depth > max_depth {
        return Vec::new();
    }
    let mut files = Vec::new();
    let entries = match tokio::fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut entries = entries;
    while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let meta = match tokio::fs::symlink_metadata(&path).await {
                Ok(m) => m,
                Err(_) => continue,
            };
            // Skip symlinks entirely to avoid leaking host secrets or infinite recursion
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_file() {
                files.push(path);
            } else if meta.is_dir() {
                files.extend(walk_dir_safe(&path, depth + 1, max_depth).await);
            }
    }
    files
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use crate::net::network::P2pNetwork;
    use nexxus_core::types::NodeId;

    #[tokio::test]
    async fn sync_engine_scans_and_uploads_files() {
        let dir = tempdir().unwrap();
        let store_path = dir.path().join("store.redb");
        let store = Arc::new(ChunkStore::open(&store_path).unwrap());
        let net = Arc::new(P2pNetwork::new(
            NodeId([0x55; 32]),
            "127.0.0.1:0".parse().unwrap(),
            store.clone(),
        ));

        let drive_dir = dir.path().join("drive");
        std::fs::create_dir_all(&drive_dir).unwrap();

        let doc1 = drive_dir.join("hello.txt");
        std::fs::write(&doc1, b"Hello NeXXus Sovereign Drive!").unwrap();

        let engine = SyncEngine::new(drive_dir.clone(), store.clone(), net.clone());
        assert_eq!(engine.drive_path(), &drive_dir);
        let rep = engine.scan_and_sync().await.unwrap();

        assert_eq!(rep.scanned_files, 1);
        assert_eq!(rep.newly_synced, 1);
        assert!(rep.total_bytes > 0);
        assert!(rep.errors.is_empty());

        // Subsequent scan with unchanged file does not re-upload
        let rep2 = engine.scan_and_sync().await.unwrap();
        assert_eq!(rep2.scanned_files, 1);
        assert_eq!(rep2.newly_synced, 0);

        // H-24: Modify file keeping the EXACT same size - must detect and re-sync!
        // Sleep slightly to guarantee mtime tick on filesystems with 1s resolution
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(&doc1, b"Hello NeXXus Sovereign World!").unwrap(); // same 29 bytes
        let rep3 = engine.scan_and_sync().await.unwrap();
        assert_eq!(rep3.scanned_files, 1);
        assert_eq!(rep3.newly_synced, 1, "H-24: modification with identical file size must trigger sync");

        // Verify status
        let st = engine.status().unwrap();
        assert_eq!(st["synced_files_count"], 1);
        assert_eq!(st["files"][0]["relative_path"], "hello.txt");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn sync_engine_skips_symlinks() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let store_path = dir.path().join("store.redb");
        let store = Arc::new(ChunkStore::open(&store_path).unwrap());
        let net = Arc::new(P2pNetwork::new(
            NodeId([0x55; 32]),
            "127.0.0.1:0".parse().unwrap(),
            store.clone(),
        ));

        let drive_dir = dir.path().join("drive");
        std::fs::create_dir_all(&drive_dir).unwrap();

        let outside_secret = dir.path().join("secret.key");
        std::fs::write(&outside_secret, b"super_secret_ssh_key").unwrap();

        // Symlink pointing to outside secret
        let symlink_path = drive_dir.join("leaked_key.txt");
        symlink(&outside_secret, &symlink_path).unwrap();

        let engine = SyncEngine::new(drive_dir.clone(), store.clone(), net.clone());
        let rep = engine.scan_and_sync().await.unwrap();

        // Must skip symlink
        assert_eq!(rep.scanned_files, 0, "H-24: symlinks must NEVER be scanned or synced");
        assert_eq!(rep.newly_synced, 0);
    }
}
