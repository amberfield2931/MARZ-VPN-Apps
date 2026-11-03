//! TLS policy, same as Android RC6 res/xml/marz_app_network.xml: the system's CAs, and for the App API
//! host a pin on the CA keys Cloudflare issues from. The pin set ends at its expiration date, so a
//! silent CA change after that date cannot lock users out. Certificate validation is never disabled.
use base64::Engine;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, DigitallySignedStruct, Error, RootCertStore, SignatureScheme};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub const PINNED_HOST: &str = "app-api.marzvpn.com";
/// 2027-10-01T00:00:00Z.
pub const PIN_EXPIRY_UNIX: u64 = 1_822_348_800;
pub const PINS: [&str; 15] = [
    "C5+lpZ7tcVwmwQIMcRtPbsQtWLABXhQzejna0wHFr8M=", // ISRG Root X1
    "diGVwiVYbubAI3RW4hB9xU8e/CH2GnkuvVFZE8zmgzI=", // ISRG Root X2
    "sCkq5UWXjg+7mKu9lMhhYF5bGLsy7VI/UNW3tccdR7w=", // ISRG Root YE
    "fk6IOKit1ild5647BH06ujSIq5XbCgqlbYl6ANhhi88=", // ISRG Root YR
    "hxqRlPTu1bMS/0DITB1SSu0vd4u/8l8TjPgfaAp63Gc=", // GTS Root R1
    "QXnt2YHvdHR3tJYmQIr0Paosp6t/nggsEGD4QJZ3Q0g=", // GTS Root R3
    "mEflZT5enoR1FuXLgYYGqnVEoZvmf9c2bVBpiOjYQ0c=", // GTS Root R4
    "oyD01TTXvpfBro3QSZc1vIlcMjrdLTiL/M9mLCPX+Zo=", // SSL.com
    "0cRTd+vc1hjNFlHcLgLCHXUeWqn80bNDH/bs9qMTSPo=", // SSL.com
    "G/ANXI8TwJTdF+AFBM8IiIUPEv0Gf6H5LA/b9guG4yE=", // SSL.com
    "K89VOmb1cJAN3TK6bf4ezAbJGC1mLcG2Dh97dnwr3VQ=", // SSL.com
    "sLVjNUaFYfW7n6EtgBeEpjOlcnBdNPMrZDRF36iwBdE=", // Sectigo / USERTrust
    "Douxi77vs4G+Ib/BogbTFymEYq0QSFXwSgVCaZcI09Q=", // Sectigo / USERTrust
    "x4QzPSC810K5/cMjb05Qm4k3Bw5zBn4lTdO/nEW/Td4=", // Sectigo / USERTrust
    "ICGRfpgmOUXIWcQ/HXPLQTkFPEFPoDyjvH7ohhQpjzs=", // Sectigo / USERTrust
];

pub fn pin_of_spki(spki_der: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(Sha256::digest(spki_der))
}

/// DER SEQUENCE header around the bytes a trust anchor keeps (its SPKI without the outer tag).
fn wrap_sequence(inner: &[u8]) -> Vec<u8> {
    let mut out = vec![0x30];
    let n = inner.len();
    if n < 0x80 {
        out.push(n as u8);
    } else if n < 0x100 {
        out.extend([0x81, n as u8]);
    } else {
        out.extend([0x82, (n >> 8) as u8, n as u8]);
    }
    out.extend_from_slice(inner);
    out
}

fn is_pinned(der: &[u8]) -> bool {
    use x509_parser::prelude::FromDer;
    x509_parser::certificate::X509Certificate::from_der(der)
        .map(|(_, c)| PINS.contains(&pin_of_spki(c.tbs_certificate.subject_pki.raw).as_str()))
        .unwrap_or(false)
}

#[derive(Debug)]
struct PinningVerifier {
    roots: Arc<RootCertStore>,
    provider: Arc<CryptoProvider>,
}

impl PinningVerifier {
    fn host(name: &ServerName<'_>) -> String {
        match name {
            ServerName::DnsName(d) => d.as_ref().to_ascii_lowercase(),
            _ => String::new(),
        }
    }
}

impl ServerCertVerifier for PinningVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        _ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let bad = |e: webpki::Error| Error::InvalidCertificate(CertificateError::Other(rustls::OtherError(Arc::new(e))));
        let cert = webpki::EndEntityCert::try_from(end_entity).map_err(bad)?;
        let path = cert
            .verify_for_usage(
                self.provider.signature_verification_algorithms.all,
                &self.roots.roots,
                intermediates,
                now,
                webpki::KeyUsage::server_auth(),
                None,
                None,
            )
            .map_err(bad)?;
        cert.verify_is_valid_for_subject_name(server_name).map_err(bad)?;
        if Self::host(server_name) == PINNED_HOST && now.as_secs() < PIN_EXPIRY_UNIX {
            // Only certificates of the verified path count, never extra ones the server merely sent.
            let anchor = pin_of_spki(&wrap_sequence(path.anchor().subject_public_key_info.as_ref()));
            let on_path = PINS.contains(&anchor.as_str()) || path.intermediate_certificates().any(|c| is_pinned(c.der().as_ref()));
            if !on_path {
                return Err(Error::InvalidCertificate(CertificateError::ApplicationVerificationFailure));
            }
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

/// Client config over the given trust store (the system's in the app, a test CA in tests).
pub fn client_config_with(roots: RootCertStore) -> Result<Arc<rustls::ClientConfig>, Error> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Arc::new(PinningVerifier { roots: Arc::new(roots), provider: provider.clone() });
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .dangerous() // the name of rustls's API for a custom verifier; this one only adds to the normal checks
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

pub fn system_roots() -> RootCertStore {
    let mut roots = RootCertStore::empty();
    let found = rustls_native_certs::load_native_certs();
    roots.add_parsable_certificates(found.certs);
    roots
}

pub fn client_config() -> Result<Arc<rustls::ClientConfig>, Error> {
    client_config_with(system_roots())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_are_well_formed_and_expire_on_the_android_date() {
        for p in PINS {
            let raw = base64::engine::general_purpose::STANDARD.decode(p).unwrap();
            assert_eq!(raw.len(), 32);
        }
        assert_eq!(PIN_EXPIRY_UNIX, 1_822_348_800); // date -u -d 2027-10-01 +%s
    }

    #[test]
    fn sequence_header_lengths() {
        assert_eq!(wrap_sequence(&[1, 2])[..2], [0x30, 2]);
        assert_eq!(wrap_sequence(&[0; 200])[..3], [0x30, 0x81, 200]);
        assert_eq!(wrap_sequence(&[0; 300])[..4], [0x30, 0x82, 1, 44]);
    }

    /// The pinned root's real key, from the system store when present: the wrapping must reproduce the
    /// pin Android carries, or every pinned connection would fail.
    #[test]
    fn system_isrg_root_matches_its_pin_when_installed() {
        let roots = system_roots();
        let found = roots.roots.iter().any(|a| PINS.contains(&pin_of_spki(&wrap_sequence(a.subject_public_key_info.as_ref())).as_str()));
        if !roots.roots.is_empty() {
            assert!(found, "no system root matches any pin");
        }
    }
}
