use anyhow::{Context, Result, anyhow, bail};
use rustls_pemfile::{pkcs8_private_keys, rsa_private_keys};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs1KeyDer, PrivatePkcs8KeyDer};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tokio_rustls::TlsAcceptor;

#[derive(Clone)]
enum KeyDer {
    Pkcs8(Vec<u8>),
    Pkcs1(Vec<u8>),
}

impl KeyDer {
    fn to_private_der(&self) -> PrivateKeyDer<'static> {
        match self {
            KeyDer::Pkcs8(bytes) => PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(bytes.clone())),
            KeyDer::Pkcs1(bytes) => PrivateKeyDer::Pkcs1(PrivatePkcs1KeyDer::from(bytes.clone())),
        }
    }
}

#[derive(Clone)]
pub struct CertificateStore {
    cert_chain: Vec<Vec<u8>>,
    key_der: KeyDer,
    client_root: Vec<u8>,
}

impl CertificateStore {
    pub fn builder() -> CertificateStoreBuilder {
        CertificateStoreBuilder::default()
    }

    pub fn rustls_server_config(&self, alpn: &[&[u8]]) -> Result<rustls::ServerConfig> {
        let chain: Vec<CertificateDer<'static>> = self
            .cert_chain
            .iter()
            .cloned()
            .map(CertificateDer::from)
            .collect();
        let key = self.key_der.to_private_der();

        let mut rustls_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(chain, key)?;
        rustls_config.alpn_protocols = alpn.iter().map(|proto| proto.to_vec()).collect();
        Ok(rustls_config)
    }

    pub fn arc_rustls_server_config(&self, alpn: &[&[u8]]) -> Result<Arc<rustls::ServerConfig>> {
        Ok(Arc::new(self.rustls_server_config(alpn)?))
    }

    pub fn tls_acceptor(&self, alpn: &[&[u8]]) -> Result<TlsAcceptor> {
        Ok(TlsAcceptor::from(self.arc_rustls_server_config(alpn)?))
    }

    pub fn https_config(&self) -> Result<Arc<rustls::ServerConfig>> {
        self.arc_rustls_server_config(&[b"h2", b"http/1.1"])
    }

    pub fn client_root_certificate(&self) -> Vec<u8> {
        self.client_root.clone()
    }
}

#[derive(Default)]
pub struct CertificateStoreBuilder {
    cert_path: Option<PathBuf>,
    key_path: Option<PathBuf>,
    root_ca_path: Option<PathBuf>,
}

impl CertificateStoreBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cert_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.cert_path = Some(path.into());
        self
    }

    pub fn key_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.key_path = Some(path.into());
        self
    }

    pub fn root_ca_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.root_ca_path = Some(path.into());
        self
    }

    pub fn build(self) -> Result<CertificateStore> {
        let cert_path = self
            .cert_path
            .ok_or_else(|| anyhow!("未设置证书路径，请调用 cert_path"))?;
        let key_path = self
            .key_path
            .ok_or_else(|| anyhow!("未设置私钥路径，请调用 key_path"))?;

        if !cert_path.exists() {
            bail!("证书文件不存在: {}", cert_path.display());
        }
        if !key_path.exists() {
            bail!("私钥文件不存在: {}", key_path.display());
        }

        let cert_chain = load_cert_chain(&cert_path)?;
        let key_der = load_private_key(&key_path)?;
        let (cert_chain, client_root) =
            load_cert_chain_with_root(cert_chain, self.root_ca_path.as_deref())?;

        tracing::info!(
            cert_path = %cert_path.display(),
            key_path = %key_path.display(),
            root_ca = self.root_ca_path.as_ref().map(|p| p.display().to_string()),
            chain_len = cert_chain.len(),
            "initialized certificate store"
        );

        Ok(CertificateStore {
            cert_chain,
            key_der,
            client_root,
        })
    }
}

/// 支持从文件路径热加载的证书存储。
#[derive(Clone)]
pub struct ReloadableCertificateStore {
    inner: Arc<RwLock<CertificateStore>>,
    cert_path: PathBuf,
    key_path: PathBuf,
    root_ca_path: Option<PathBuf>,
}

impl ReloadableCertificateStore {
    pub fn from_paths<P: Into<PathBuf>>(
        cert_path: P,
        key_path: P,
        root_ca_path: Option<PathBuf>,
    ) -> Result<Self> {
        let cert_path = cert_path.into();
        let key_path = key_path.into();
        let mut builder = CertificateStoreBuilder::new()
            .cert_path(cert_path.clone())
            .key_path(key_path.clone());
        if let Some(root) = root_ca_path.clone() {
            builder = builder.root_ca_path(root);
        }
        let store = builder.build()?;
        Ok(Self {
            inner: Arc::new(RwLock::new(store)),
            cert_path,
            key_path,
            root_ca_path,
        })
    }

    /// 重新从磁盘加载证书与私钥。
    pub fn reload(&self) -> Result<()> {
        let mut builder = CertificateStoreBuilder::new()
            .cert_path(self.cert_path.clone())
            .key_path(self.key_path.clone());
        if let Some(root) = self.root_ca_path.clone() {
            builder = builder.root_ca_path(root);
        }
        let store = builder.build()?;
        if let Ok(mut guard) = self.inner.write() {
            *guard = store;
        }
        Ok(())
    }

    pub fn tls_acceptor(&self, alpn: &[&[u8]]) -> Result<TlsAcceptor> {
        let guard = self.inner.read().expect("certificate store poisoned");
        guard.tls_acceptor(alpn)
    }

    pub fn https_acceptor(&self) -> Result<TlsAcceptor> {
        self.tls_acceptor(&[b"h2", b"http/1.1"])
    }
}

fn load_cert_chain(cert_path: &Path) -> Result<Vec<Vec<u8>>> {
    let data = fs::read(cert_path)
        .with_context(|| format!("读取证书文件失败: {}", cert_path.display()))?;
    if looks_like_pem(&data) || is_pem_path(cert_path) {
        let mut reader = Cursor::new(&data);
        let certs = rustls_pemfile::certs(&mut reader)
            .collect::<Result<Vec<_>, _>>()
            .context("解析 PEM 证书失败")?;
        if certs.is_empty() {
            bail!("PEM 证书文件为空: {}", cert_path.display());
        }
        Ok(certs.into_iter().map(|c| c.to_vec()).collect())
    } else {
        Ok(vec![data])
    }
}

fn load_private_key(key_path: &Path) -> Result<KeyDer> {
    let data =
        fs::read(key_path).with_context(|| format!("读取私钥文件失败: {}", key_path.display()))?;
    if looks_like_pem(&data) || is_pem_path(key_path) {
        let mut cursor = Cursor::new(&data);
        let mut keys = pkcs8_private_keys(&mut cursor)
            .collect::<Result<Vec<_>, _>>()
            .context("解析 PKCS8 私钥失败")?;
        if let Some(key) = keys.pop() {
            return Ok(KeyDer::Pkcs8(key.secret_pkcs8_der().to_vec()));
        }

        let mut cursor = Cursor::new(&data);
        let mut rsa_keys = rsa_private_keys(&mut cursor)
            .collect::<Result<Vec<_>, _>>()
            .context("解析 RSA 私钥失败")?;
        if let Some(rsa_key) = rsa_keys.pop() {
            return Ok(KeyDer::Pkcs1(rsa_key.secret_pkcs1_der().to_vec()));
        }

        bail!(
            "PEM 私钥文件不包含 PKCS8 或 RSA 私钥: {}",
            key_path.display()
        );
    }

    Ok(KeyDer::Pkcs8(data))
}

fn load_cert_chain_with_root(
    mut chain: Vec<Vec<u8>>,
    root_ca_path: Option<&Path>,
) -> Result<(Vec<Vec<u8>>, Vec<u8>)> {
    if chain.is_empty() {
        bail!("证书链为空");
    }

    let mut client_root = chain[0].clone();

    if let Some(path) = root_ca_path {
        if path.exists() {
            let root_chain = load_cert_chain(path)?;
            if let Some(first) = root_chain.first() {
                client_root = first.clone();
            }
            for cert in root_chain {
                if !chain.iter().any(|existing| existing == &cert) {
                    chain.push(cert);
                }
            }
        } else {
            tracing::warn!(
                path = %path.display(),
                "根证书文件不存在，将使用服务器证书作为客户端根证书"
            );
        }
    }

    Ok((chain, client_root))
}

fn looks_like_pem(data: &[u8]) -> bool {
    data.starts_with(b"-----BEGIN")
}

fn is_pem_path(path: &Path) -> bool {
    matches!(path.extension().and_then(|ext| ext.to_str()), Some("pem"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looks_like_pem_and_ext() {
        assert!(looks_like_pem(b"-----BEGIN CERTIFICATE-----\n..."));
        assert!(!looks_like_pem(b"random bytes"));
        assert!(is_pem_path(Path::new("/tmp/test.pem")));
        assert!(!is_pem_path(Path::new("/tmp/test.der")));
    }

    #[test]
    fn test_builder_missing_paths_errors() {
        // 仅设置 key_path，缺少 cert_path
        let err = CertificateStoreBuilder::new()
            .key_path("/tmp/missing.key")
            .build()
            .err()
            .expect("should error when cert_path is missing");
        let msg = format!("{err:#}");
        assert!(msg.contains("未设置证书路径"));

        // 仅设置 cert_path，缺少 key_path
        let err = CertificateStoreBuilder::new()
            .cert_path("/tmp/missing.crt")
            .build()
            .err()
            .expect("should error when key_path is missing");
        let msg = format!("{err:#}");
        assert!(msg.contains("未设置私钥路径"));
    }

    #[test]
    fn test_builder_nonexistent_files_errors() {
        // 同时设置证书与私钥，但文件不存在
        let err = CertificateStoreBuilder::new()
            .cert_path("/tmp/not-exist.crt")
            .key_path("/tmp/not-exist.key")
            .build()
            .err()
            .expect("should error on non-existent files");
        let msg = format!("{err:#}");
        assert!(msg.contains("证书文件不存在") || msg.contains("私钥文件不存在"));
    }

    #[test]
    fn test_builder_success_with_raw_der_bytes() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};
        let base = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let cert_path = base.join(format!("silent_tls_test_{}.crt", unique));
        let key_path = base.join(format!("silent_tls_test_{}.key", unique));

        // 写入原始字节（非 PEM），builder 会将其视为 DER 字节并成功构建
        fs::write(&cert_path, b"CERTBYTES").unwrap();
        fs::write(&key_path, b"KEYBYTES").unwrap();

        let store = CertificateStore::builder()
            .cert_path(&cert_path)
            .key_path(&key_path)
            .build()
            .expect("builder should succeed with raw bytes");

        // 能返回客户端根证书字节（即我们写入的第一段）
        let root = store.client_root_certificate();
        assert!(!root.is_empty());

        let _ = fs::remove_file(&cert_path);
        let _ = fs::remove_file(&key_path);
    }

    #[test]
    fn test_https_config_error_on_invalid_der() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};
        let base = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let cert_path = base.join(format!("silent_tls_test_bad_{}.crt", unique));
        let key_path = base.join(format!("silent_tls_test_bad_{}.key", unique));

        // 无效原始字节：将导致 https_config() 失败
        fs::write(&cert_path, b"BAD_CERT").unwrap();
        fs::write(&key_path, b"BAD_KEY").unwrap();

        let store = CertificateStore::builder()
            .cert_path(&cert_path)
            .key_path(&key_path)
            .build()
            .expect("builder should still construct store with raw bytes");

        let err = store
            .https_config()
            .expect_err("https_config should fail on invalid der");
        let msg = format!("{err:#}");
        assert!(!msg.is_empty());

        let _ = fs::remove_file(&cert_path);
        let _ = fs::remove_file(&key_path);
    }
}
