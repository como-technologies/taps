//! Storage backend selection.
//!
//! `StorageService` is implemented on top of the `object_store` crate's
//! `ObjectStore` trait. This module picks the concrete backend based on
//! config and returns an `Arc<dyn ObjectStore>` the rest of the storage
//! layer talks to. Everything cloud-provider-specific lives here.
//!
//! Backends we expose:
//!
//! - [`StorageBackend::Filesystem`] — `object_store::local::LocalFileSystem`
//!   rooted at a directory. The dev / single-VM default.
//! - [`StorageBackend::S3Compatible`] — `object_store::aws::AmazonS3` with a
//!   configurable endpoint. Speaks the S3 wire protocol; works against
//!   rustfs (our local-dev S3-compatible target), Cloudflare R2, Backblaze
//!   B2, Hetzner Object Storage, GCS-via-S3-interop, etc. Despite the
//!   `AmazonS3` type name in `object_store`, this is the *protocol* impl —
//!   we're not coupling to AWS-the-service.
//! - [`StorageBackend::Gcs`] — `object_store::gcp::GoogleCloudStorage`
//!   (GCP-native API; provided as an alternative to the S3-interop path
//!   for users who want native GCS).
//! - [`StorageBackend::InMemory`] — `object_store::memory::InMemory`. Useful
//!   for tests and quick experiments; process-local only.

use std::path::Path;
use std::sync::Arc;

use object_store::ObjectStore;
use object_store::aws::AmazonS3Builder;
use object_store::gcp::GoogleCloudStorageBuilder;
use object_store::local::LocalFileSystem;
use object_store::memory::InMemory;

use crate::error::AppError;

/// Which `ObjectStore` implementation to instantiate.
#[derive(Debug, Clone)]
pub enum StorageBackend {
    /// Local filesystem rooted at the given directory.
    Filesystem { root: std::path::PathBuf },

    /// S3-protocol compatible provider (rustfs / R2 / B2 / Hetzner / etc.).
    /// `endpoint` is the full URL to the API (e.g. `http://localhost:9000`).
    S3Compatible {
        endpoint: String,
        bucket: String,
        region: String,
        access_key_id: String,
        secret_access_key: String,
    },

    /// Google Cloud Storage via the native API. `service_account_path` is
    /// optional — when `None`, Application Default Credentials are used.
    Gcs {
        bucket: String,
        service_account_path: Option<std::path::PathBuf>,
    },

    /// Process-local in-memory store. Tests and tire-kicking only — nothing
    /// persists across restarts.
    InMemory,
}

/// Construct the configured `ObjectStore`. The returned handle is cheap to
/// clone via `Arc` and can be shared across all of `StorageService`'s
/// operations and across threads.
pub fn build_store(backend: &StorageBackend) -> Result<Arc<dyn ObjectStore>, AppError> {
    match backend {
        StorageBackend::Filesystem { root } => {
            std::fs::create_dir_all(root).map_err(AppError::Storage)?;
            let store = LocalFileSystem::new_with_prefix(root).map_err(|e| {
                AppError::Internal(format!("LocalFileSystem at {}: {e}", root.display()))
            })?;
            Ok(Arc::new(store))
        }
        StorageBackend::S3Compatible {
            endpoint,
            bucket,
            region,
            access_key_id,
            secret_access_key,
        } => {
            let store = AmazonS3Builder::new()
                .with_endpoint(endpoint)
                .with_bucket_name(bucket)
                .with_region(region)
                .with_access_key_id(access_key_id)
                .with_secret_access_key(secret_access_key)
                // rustfs and self-hosted S3-compats commonly serve HTTP
                // rather than HTTPS; the SDK's default is HTTPS-only.
                .with_allow_http(endpoint.starts_with("http://"))
                .build()
                .map_err(|e| AppError::Internal(format!("AmazonS3 builder: {e}")))?;
            Ok(Arc::new(store))
        }
        StorageBackend::Gcs {
            bucket,
            service_account_path,
        } => {
            let mut builder = GoogleCloudStorageBuilder::new().with_bucket_name(bucket);
            if let Some(p) = service_account_path {
                builder = builder.with_service_account_path(p.display().to_string());
            }
            let store = builder
                .build()
                .map_err(|e| AppError::Internal(format!("GoogleCloudStorage builder: {e}")))?;
            Ok(Arc::new(store))
        }
        StorageBackend::InMemory => Ok(Arc::new(InMemory::new())),
    }
}

/// Convenience for `Filesystem` callers that already have a `Path`.
pub fn filesystem(root: impl AsRef<Path>) -> StorageBackend {
    StorageBackend::Filesystem {
        root: root.as_ref().to_path_buf(),
    }
}

/// Read a [`StorageBackend`] from environment variables, namespaced by
/// `prefix` (e.g. `"AUTHOR"`, `"ASSESS"`, `"ANALYZE"`).
///
/// Env vars (all optional; `filesystem` is the default backend):
/// - `<PREFIX>_STORAGE_BACKEND` — `filesystem` (default), `s3_compatible`, `gcs`, `in_memory`.
/// - When `filesystem`: `<PREFIX>_DATA_DIR` (default: `./data`).
/// - When `s3_compatible`: `<PREFIX>_S3_ENDPOINT`, `<PREFIX>_S3_BUCKET`,
///   `<PREFIX>_S3_REGION`, `<PREFIX>_S3_ACCESS_KEY_ID`, `<PREFIX>_S3_SECRET_ACCESS_KEY`.
/// - When `gcs`: `<PREFIX>_GCS_BUCKET`, `<PREFIX>_GCS_SERVICE_ACCOUNT_PATH`
///   (optional; falls back to Application Default Credentials).
pub fn load_from_env(prefix: &str) -> Result<StorageBackend, AppError> {
    let backend_var = format!("{prefix}_STORAGE_BACKEND");
    let raw = std::env::var(&backend_var).unwrap_or_else(|_| "filesystem".to_string());
    match raw.as_str() {
        "filesystem" | "" => {
            let dir = std::env::var(format!("{prefix}_DATA_DIR"))
                .unwrap_or_else(|_| "./data".to_string());
            Ok(StorageBackend::Filesystem {
                root: std::path::PathBuf::from(dir),
            })
        }
        "in_memory" => Ok(StorageBackend::InMemory),
        "s3_compatible" => Ok(StorageBackend::S3Compatible {
            endpoint: required(prefix, "S3_ENDPOINT")?,
            bucket: required(prefix, "S3_BUCKET")?,
            region: required(prefix, "S3_REGION")?,
            access_key_id: required(prefix, "S3_ACCESS_KEY_ID")?,
            secret_access_key: required(prefix, "S3_SECRET_ACCESS_KEY")?,
        }),
        "gcs" => Ok(StorageBackend::Gcs {
            bucket: required(prefix, "GCS_BUCKET")?,
            service_account_path: std::env::var(format!("{prefix}_GCS_SERVICE_ACCOUNT_PATH"))
                .ok()
                .filter(|s| !s.is_empty())
                .map(std::path::PathBuf::from),
        }),
        other => Err(AppError::Internal(format!(
            "{backend_var}: unknown backend '{other}'; \
             expected one of filesystem, s3_compatible, gcs, in_memory"
        ))),
    }
}

fn required(prefix: &str, suffix: &str) -> Result<String, AppError> {
    let name = format!("{prefix}_{suffix}");
    std::env::var(&name).map_err(|_| AppError::Internal(format!("missing required env var {name}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use object_store::path::Path as ObjectPath;
    use object_store::{ObjectStoreExt, PutPayload};
    use tempfile::TempDir;

    #[tokio::test]
    async fn in_memory_round_trips_a_blob() {
        let store = build_store(&StorageBackend::InMemory).unwrap();
        let key = ObjectPath::from("projects/p1/draft.yaml");
        store
            .put(&key, PutPayload::from(Bytes::from_static(b"hello: world")))
            .await
            .unwrap();
        let got = store.get(&key).await.unwrap().bytes().await.unwrap();
        assert_eq!(&got[..], b"hello: world");
    }

    #[tokio::test]
    async fn filesystem_round_trips_a_blob() {
        let tmp = TempDir::new().unwrap();
        let store = build_store(&filesystem(tmp.path())).unwrap();
        let key = ObjectPath::from("projects/p1/draft.yaml");
        store
            .put(&key, PutPayload::from(Bytes::from_static(b"hello: world")))
            .await
            .unwrap();
        let got = store.get(&key).await.unwrap().bytes().await.unwrap();
        assert_eq!(&got[..], b"hello: world");
    }

    #[tokio::test]
    async fn filesystem_writes_real_files_under_the_root() {
        let tmp = TempDir::new().unwrap();
        let store = build_store(&filesystem(tmp.path())).unwrap();
        let key = ObjectPath::from("projects/p1/project.json");
        store
            .put(&key, PutPayload::from(Bytes::from_static(b"{}")))
            .await
            .unwrap();
        let on_disk = tmp.path().join("projects/p1/project.json");
        assert!(on_disk.exists(), "{} should exist", on_disk.display());
        assert_eq!(std::fs::read(on_disk).unwrap(), b"{}");
    }
}
