//! A small private CA and a server certificate signed by it, so Tessera can serve HTTPS
//! (and therefore HTTP/2) without a reverse proxy. Import `ca.crt` into the browser once and
//! the certificate is trusted on every client.
//!
//! Files in `<data>/tls` are compatible with Letopis: `ca.crt`/`ca.key` (P-256, SEC1
//! "EC PRIVATE KEY") and `server.crt`/`server.key`. An existing CA keeps being used to sign
//! new server certificates, so browsers that trust it keep trusting the server.

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use p256::pkcs8::{DecodePrivateKey, EncodePrivateKey};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose, SanType, SerialNumber,
};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// Paths of the TLS files.
#[derive(Debug, Clone, Default)]
pub struct Files {
    pub dir: PathBuf,
    pub ca_cert: PathBuf,
    pub ca_key: PathBuf,
    pub cert: PathBuf,
    pub key: PathBuf,
}

fn paths(dir: &Path) -> Files {
    Files {
        dir: dir.to_path_buf(),
        ca_cert: dir.join("ca.crt"),
        ca_key: dir.join("ca.key"),
        cert: dir.join("server.crt"),
        key: dir.join("server.key"),
    }
}

/// A certificate chain and its key, ready for rustls.
pub struct CertKey {
    pub chain: Vec<CertificateDer<'static>>,
    pub key: PrivateKeyDer<'static>,
}

/// Loads a PEM certificate chain + key (user-provided `TESSERA_TLS_CERT`/`_KEY`).
pub fn load_pair(cert: &Path, key: &Path) -> anyhow::Result<CertKey> {
    let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert)
        .with_context(|| format!("read {}", cert.display()))?
        .collect::<Result<_, _>>()
        .map_err(|e| anyhow!("{}: {e}", cert.display()))?;
    if chain.is_empty() {
        bail!("{}: no certificates", cert.display());
    }
    let key = PrivateKeyDer::from_pem_file(key).map_err(|e| anyhow!("{}: {e}", key.display()))?;
    Ok(CertKey { chain, key })
}

/// Returns a certificate for `hosts`, creating the CA and/or server certificate if missing,
/// expiring within 30 days, or not covering every host.
pub fn ensure(dir: &Path, hosts: &[String]) -> anyhow::Result<(CertKey, Files)> {
    let f = paths(dir);
    create_private_dir(dir)?;
    let (ca_der, ca_secret) = load_or_create_ca(&f)?;
    if let Ok(ck) = load_pair(&f.cert, &f.key) {
        if covers(&ck, hosts, &ca_der) {
            return Ok((ck, f));
        }
    }
    create_server(&f, hosts, &ca_der, &ca_secret)?;
    let ck = load_pair(&f.cert, &f.key)?;
    Ok((ck, f))
}

fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    if dir.is_dir() {
        return Ok(());
    }
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

fn until(not_after: i64) -> i64 {
    not_after - chrono::Utc::now().timestamp()
}

const THIRTY_DAYS: i64 = 30 * 24 * 3600;

fn covers(ck: &CertKey, hosts: &[String], ca_der: &[u8]) -> bool {
    let Ok((_, leaf)) = x509_parser::parse_x509_certificate(&ck.chain[0]) else {
        return false;
    };
    if until(leaf.validity().not_after.timestamp()) < THIRTY_DAYS {
        return false;
    }
    let Ok((_, ca)) = x509_parser::parse_x509_certificate(ca_der) else {
        return false;
    };
    if leaf.verify_signature(Some(ca.public_key())).is_err() {
        return false;
    }
    // the key must belong to the certificate
    if let PrivateKeyDer::Sec1(k) = &ck.key {
        match p256::SecretKey::from_sec1_der(k.secret_sec1_der()) {
            Ok(sk) => {
                let pubkey = sk.public_key();
                let point =
                    p256::elliptic_curve::sec1::ToEncodedPoint::to_encoded_point(&pubkey, false);
                if leaf.public_key().subject_public_key.data.as_ref() != point.as_bytes() {
                    return false;
                }
            }
            Err(_) => return false,
        }
    }
    let mut dns: Vec<String> = Vec::new();
    let mut ips: Vec<IpAddr> = Vec::new();
    if let Ok(Some(san)) = leaf.subject_alternative_name() {
        for n in &san.value.general_names {
            match n {
                x509_parser::extensions::GeneralName::DNSName(s) => dns.push(s.to_string()),
                x509_parser::extensions::GeneralName::IPAddress(b) => {
                    if b.len() == 4 {
                        ips.push(IpAddr::from(<[u8; 4]>::try_from(*b).unwrap_or_default()));
                    } else if b.len() == 16 {
                        ips.push(IpAddr::from(<[u8; 16]>::try_from(*b).unwrap_or_default()));
                    }
                }
                _ => {}
            }
        }
    }
    hosts.iter().all(|h| match h.parse::<IpAddr>() {
        Ok(ip) => ips.iter().any(|x| same_ip(x, &ip)),
        Err(_) => dns.iter().any(|d| d == h),
    })
}

fn same_ip(a: &IpAddr, b: &IpAddr) -> bool {
    a.to_canonical() == b.to_canonical()
}

/// PEM like Go's `pem.EncodeToMemory` (64-column base64).
pub fn pem_encode(typ: &str, der: &[u8]) -> String {
    let b64 = STANDARD.encode(der);
    let mut s = format!("-----BEGIN {typ}-----\n");
    for chunk in b64.as_bytes().chunks(64) {
        s.push_str(std::str::from_utf8(chunk).unwrap_or(""));
        s.push('\n');
    }
    s.push_str(&format!("-----END {typ}-----\n"));
    s
}

fn write_file(p: &Path, data: &[u8], mode: u32) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(mode)
        .open(p)?;
    f.write_all(data)
}

fn serial() -> SerialNumber {
    let mut b = [0u8; 15];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut b);
    b[0] &= 0x7f;
    if b[0] == 0 {
        b[0] = 1;
    }
    SerialNumber::from_slice(&b)
}

fn rcgen_key(sk: &p256::SecretKey) -> anyhow::Result<KeyPair> {
    let der = sk.to_pkcs8_der().map_err(|e| anyhow!("pkcs8: {e}"))?;
    KeyPair::try_from(der.as_bytes()).map_err(|e| anyhow!("key: {e}"))
}

fn new_secret() -> anyhow::Result<p256::SecretKey> {
    let kp = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    p256::SecretKey::from_pkcs8_der(&kp.serialize_der()).map_err(|e| anyhow!("key: {e}"))
}

fn sec1_pem(sk: &p256::SecretKey) -> anyhow::Result<String> {
    let der = sk.to_sec1_der().map_err(|e| anyhow!("sec1: {e}"))?;
    Ok(pem_encode("EC PRIVATE KEY", &der))
}

fn offset(secs: i64) -> time::OffsetDateTime {
    time::OffsetDateTime::now_utc() + time::Duration::seconds(secs)
}

fn load_or_create_ca(f: &Files) -> anyhow::Result<(Vec<u8>, p256::SecretKey)> {
    if let (Ok(cb), Ok(kb)) = (
        std::fs::read(&f.ca_cert),
        std::fs::read_to_string(&f.ca_key),
    ) {
        if let Ok(cert) = CertificateDer::from_pem_slice(&cb) {
            if let Ok(sk) = p256::SecretKey::from_sec1_pem(&kb) {
                if let Ok((_, parsed)) = x509_parser::parse_x509_certificate(&cert) {
                    if until(parsed.validity().not_after.timestamp()) > THIRTY_DAYS {
                        return Ok((cert.to_vec(), sk));
                    }
                }
            }
        }
    }
    let sk = new_secret()?;
    let key = rcgen_key(&sk)?;
    let mut params = CertificateParams::default();
    params.serial_number = Some(serial());
    params
        .distinguished_name
        .push(DnType::OrganizationName, "Tessera");
    params
        .distinguished_name
        .push(DnType::CommonName, "Tessera local CA");
    params.not_before = offset(-3600);
    params.not_after = offset(20 * 365 * 24 * 3600 + 5 * 24 * 3600);
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    let cert = params.self_signed(&key)?;
    write_file(&f.ca_key, sec1_pem(&sk)?.as_bytes(), 0o600)?;
    write_file(
        &f.ca_cert,
        pem_encode("CERTIFICATE", cert.der()).as_bytes(),
        0o644,
    )?;
    // a new CA invalidates the old server certificate
    let _ = std::fs::remove_file(&f.cert);
    Ok((cert.der().to_vec(), sk))
}

fn create_server(
    f: &Files,
    hosts: &[String],
    ca_der: &[u8],
    ca_sk: &p256::SecretKey,
) -> anyhow::Result<()> {
    if hosts.is_empty() {
        bail!("no TLS hosts");
    }
    let issuer =
        Issuer::from_ca_cert_der(&CertificateDer::from(ca_der.to_vec()), rcgen_key(ca_sk)?)?;
    let sk = new_secret()?;
    let key = rcgen_key(&sk)?;
    let mut params = CertificateParams::default();
    params.serial_number = Some(serial());
    params
        .distinguished_name
        .push(DnType::OrganizationName, "Tessera");
    params
        .distinguished_name
        .push(DnType::CommonName, hosts[0].as_str());
    params.not_before = offset(-3600);
    // browsers reject server certs valid for more than ~398 days, even from private CAs
    params.not_after = offset(395 * 24 * 3600);
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.use_authority_key_identifier_extension = true;
    for h in hosts {
        let h = h.trim();
        if let Ok(ip) = h.parse::<IpAddr>() {
            params.subject_alt_names.push(SanType::IpAddress(ip));
        } else if !h.is_empty() {
            params.subject_alt_names.push(SanType::DnsName(
                h.try_into().map_err(|e| anyhow!("TLS host {h:?}: {e}"))?,
            ));
        }
    }
    let cert = params.signed_by(&key, &issuer)?;
    write_file(&f.key, sec1_pem(&sk)?.as_bytes(), 0o600)?;
    write_file(
        &f.cert,
        pem_encode("CERTIFICATE", cert.der()).as_bytes(),
        0o644,
    )?;
    Ok(())
}

/// How often the serving certificate is re-checked (renewed before expiry).
pub const RECHECK: Duration = Duration::from_secs(24 * 3600);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ca_and_server_cert() {
        let dir = tempfile::tempdir().unwrap();
        let hosts = vec!["localhost".to_string(), "127.0.0.1".to_string()];
        let (ck, f) = ensure(dir.path(), &hosts).unwrap();
        let ca1 = std::fs::read(&f.ca_cert).unwrap();
        let cert1 = std::fs::read(&f.cert).unwrap();
        assert!(String::from_utf8_lossy(&std::fs::read(&f.ca_key).unwrap())
            .starts_with("-----BEGIN EC PRIVATE KEY-----"));
        assert!(covers(
            &ck,
            &hosts,
            &CertificateDer::from_pem_slice(&ca1).unwrap()
        ));
        // second call reuses everything
        ensure(dir.path(), &hosts).unwrap();
        assert_eq!(std::fs::read(&f.cert).unwrap(), cert1);
        // a new host → new server cert, same CA
        let more = vec!["localhost".to_string(), "nas.lan".to_string()];
        ensure(dir.path(), &more).unwrap();
        assert_ne!(std::fs::read(&f.cert).unwrap(), cert1);
        assert_eq!(std::fs::read(&f.ca_cert).unwrap(), ca1);
        if crate::util::look_path("openssl").is_some() {
            let out = std::process::Command::new("openssl")
                .args(["verify", "-CAfile"])
                .arg(&f.ca_cert)
                .arg(&f.cert)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stdout)
            );
        }
    }
}
