use crate::error::{AppError, AppResult};
use crate::model::{RemoteFileMeta, S3Config};
use crate::storage::StorageBackend;
use async_trait::async_trait;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::{BehaviorVersion, Region};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;

pub struct S3Storage {
    client: Client,
    bucket: String,
    cfg: S3Config,
}

impl S3Storage {
    pub async fn new(cfg: S3Config) -> AppResult<Self> {
        if cfg.endpoint.is_empty() || cfg.bucket.is_empty() {
            return Err(AppError::Config("endpoint/bucket required".into()));
        }
        let creds = Credentials::new(
            cfg.access_key_id.clone(),
            cfg.secret_access_key.clone(),
            None,
            None,
            "gsyncing",
        );
        let sdk_config = aws_sdk_s3::config::Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(cfg.region.clone()))
            .endpoint_url(cfg.endpoint.clone())
            .credentials_provider(creds)
            .force_path_style(cfg.path_style)
            .build();
        let client = Client::from_conf(sdk_config);
        Ok(Self {
            client,
            bucket: cfg.bucket.clone(),
            cfg,
        })
    }
}

#[async_trait]
impl StorageBackend for S3Storage {
    fn label(&self) -> String {
        format!("s3:{}", self.bucket)
    }

    async fn ping(&self) -> AppResult<String> {
        // list with max-keys=1 to verify credentials + endpoint + bucket
        let resp = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .max_keys(1)
            .prefix(self.cfg.prefix.clone())
            .send()
            .await
            .map_err(|e| AppError::storage(format!("list_objects: {e}")))?;
        let n = resp.key_count().unwrap_or(0);
        Ok(format!(
            "bucket={} prefix={} keys_visible={}",
            self.bucket, self.cfg.prefix, n
        ))
    }

    async fn list(&self, prefix: &str) -> AppResult<Vec<RemoteFileMeta>> {
        let mut out: Vec<RemoteFileMeta> = Vec::new();
        let mut continuation: Option<String> = None;
        loop {
            let mut req = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(prefix);
            if let Some(token) = continuation.as_ref() {
                req = req.continuation_token(token);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| AppError::storage(format!("list_objects: {e}")))?;
            if let Some(objs) = resp.contents {
                for o in objs {
                    let key = o.key.unwrap_or_default();
                    let size = o.size.unwrap_or(0) as u64;
                    let etag = o.e_tag.map(|s| s.trim_matches('"').to_string());
                    let modified = o
                        .last_modified
                        .map(|t| t.secs() * 1000 + t.subsec_nanos() as i64 / 1_000_000);
                    out.push(RemoteFileMeta {
                        key,
                        size,
                        etag,
                        modified,
                    });
                }
            }
            if let Some(token) = resp.next_continuation_token {
                continuation = Some(token);
            } else {
                break;
            }
        }
        Ok(out)
    }

    async fn put(&self, key: &str, body: Vec<u8>) -> AppResult<()> {
        let stream = ByteStream::from(body);
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(stream)
            .send()
            .await
            .map_err(|e| AppError::storage(format!("put_object {key}: {e}")))?;
        Ok(())
    }

    async fn get(&self, key: &str) -> AppResult<Vec<u8>> {
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| AppError::storage(format!("get_object {key}: {e}")))?;
        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| AppError::storage(format!("read body {key}: {e}")))?
            .into_bytes();
        Ok(bytes.to_vec())
    }

    async fn delete(&self, key: &str) -> AppResult<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| AppError::storage(format!("delete_object {key}: {e}")))?;
        Ok(())
    }

    async fn copy(&self, src_key: &str, dst_key: &str) -> AppResult<()> {
        // Server-side copy — avoids the round trip + memory of get+put.
        let copy_source = format!("{}/{}", self.bucket, src_key);
        self.client
            .copy_object()
            .copy_source(copy_source)
            .bucket(&self.bucket)
            .key(dst_key)
            .send()
            .await
            .map_err(|e| AppError::storage(format!("copy_object {src_key} -> {dst_key}: {e}")))?;
        Ok(())
    }

    async fn put_path(&self, key: &str, path: &std::path::Path) -> AppResult<()> {
        // Native streaming upload via the AWS SDK — file is read in 8 KiB
        // chunks under the hood, peak memory is ~SDK buffer size not file size.
        let body = ByteStream::from_path(path)
            .await
            .map_err(|e| AppError::storage(format!("open {}: {e}", path.display())))?;
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(body)
            .send()
            .await
            .map_err(|e| AppError::storage(format!("put_object (stream) {key}: {e}")))?;
        Ok(())
    }

    async fn get_to_path(&self, key: &str, path: &std::path::Path) -> AppResult<()> {
        use tokio::io::AsyncWriteExt;
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| AppError::storage(format!("get_object {key}: {e}")))?;

        let path = path.to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(AppError::Io)?;
            }
        }
        let tmp = path.with_extension("gsyncing.tmp");
        let mut file = tokio::fs::File::create(&tmp).await.map_err(AppError::Io)?;
        let mut body = resp.body;
        while let Some(chunk_res) = body.next().await {
            let chunk = chunk_res.map_err(|e| AppError::storage(format!("stream {key}: {e}")))?;
            file.write_all(&chunk).await.map_err(AppError::Io)?;
        }
        file.flush().await.map_err(AppError::Io)?;
        drop(file);
        tokio::fs::rename(&tmp, &path).await.map_err(AppError::Io)?;
        Ok(())
    }
}
