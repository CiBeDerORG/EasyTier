//! TLS 1.3 verifier pinned to the SPKI SHA-256 shown by the host's
//! `easyhostagent -identity`. Pin validation happens IN THE TLS HANDSHAKE,
//! including on every connection/reconnection before sending a bearer token.
use rustls::{
    DigitallySignedStruct, Error as TlsError, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use subtle::ConstantTimeEq;

#[derive(Clone)]
pub(crate) struct Fingerprint([u8; 32]);
impl Fingerprint {
    pub(crate) fn parse(raw: &str) -> Result<Self, &'static str> {
        let raw = raw.trim();
        if raw.len() != 64 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("TLS SPKI SHA-256 must have exactly 64 hexadecimal digits");
        }
        let mut pin = [0u8; 32];
        for (i, pair) in raw.as_bytes().chunks_exact(2).enumerate() {
            let s = std::str::from_utf8(pair).map_err(|_| "invalid fingerprint")?;
            pin[i] = u8::from_str_radix(s, 16).map_err(|_| "invalid fingerprint")?;
        }
        Ok(Self(pin))
    }
    pub(crate) fn hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[derive(Debug)]
struct PinVerifier([u8; 32]);

// Minimal, strict DER TLV reader: only walks the outer X.509 certificate
// sequence and tbsCertificate fields to obtain the *complete* encoded SPKI.
// No untrusted lengths are used for allocation.
fn tlv(bytes: &[u8], at: usize) -> Option<(u8, usize, usize)> {
    let tag = *bytes.get(at)?;
    let first = *bytes.get(at.checked_add(1)?)?;
    let (length, start) = if first & 0x80 == 0 {
        (usize::from(first), at.checked_add(2)?)
    } else {
        let count = usize::from(first & 0x7f);
        if count == 0 || count > 4 {
            return None;
        }
        let len_start = at.checked_add(2)?;
        let slice = bytes.get(len_start..len_start.checked_add(count)?)?;
        if slice[0] == 0 {
            return None;
        }
        let mut size = 0usize;
        for b in slice {
            size = size.checked_mul(256)?.checked_add(usize::from(*b))?;
        }
        if size < 128 {
            return None;
        }
        (size, len_start.checked_add(count)?)
    };
    let end = start.checked_add(length)?;
    bytes.get(start..end)?;
    Some((tag, start, end))
}
fn spki_der(cert: &[u8]) -> Option<&[u8]> {
    let (tag, begin, end) = tlv(cert, 0)?;
    if tag != 0x30 || end != cert.len() {
        return None;
    }
    let (tag, tbs_start, tbs_end) = tlv(cert, begin)?;
    if tag != 0x30 {
        return None;
    }
    let tbs = cert.get(tbs_start..tbs_end)?;
    let mut at = 0;
    if tlv(tbs, at)?.0 == 0xa0 {
        at = tlv(tbs, at)?.2;
    }
    // serialNumber, signature algorithm, issuer, validity, subject.
    for tag in [0x02, 0x30, 0x30, 0x30, 0x30] {
        let (found, _, end) = tlv(tbs, at)?;
        if found != tag {
            return None;
        }
        at = end;
    }
    let (tag, _, spki_end) = tlv(tbs, at)?;
    if tag != 0x30 {
        return None;
    }
    // Reject empty SPKI or missing algorithm/key fields.
    let (_, inner, _) = tlv(tbs, at)?;
    if inner == spki_end {
        return None;
    }
    let (algorithm, _, algorithm_end) = tlv(tbs, inner)?;
    if algorithm != 0x30 {
        return None;
    }
    let (key_tag, _, key_end) = tlv(tbs, algorithm_end)?;
    if key_tag != 0x03 || key_end != spki_end {
        return None;
    }
    tbs.get(at..spki_end)
}

// Enforce the embedded certificate's validity interval in addition to the
// unique trusted SPKI. The Agent certificate uses standard UTCTime/GeneralizedTime.
fn valid_at(cert: &[u8], now: UnixTime) -> bool {
    fn check(cert: &[u8], now: UnixTime) -> Option<bool> {
        let (_, begin, _) = tlv(cert, 0)?;
        let (_, start, end) = tlv(cert, begin)?;
        let tbs = cert.get(start..end)?;
        let mut at = 0;
        if tlv(tbs, at)?.0 == 0xa0 {
            at = tlv(tbs, at)?.2;
        }
        for expected in [0x02, 0x30, 0x30] {
            let (tag, _, end) = tlv(tbs, at)?;
            if tag != expected {
                return None;
            }
            at = end;
        }
        let (tag, inner, end) = tlv(tbs, at)?;
        if tag != 0x30 {
            return None;
        }
        fn timestamp(data: &[u8], at: usize) -> Option<(i64, usize)> {
            let (tag, start, end) = tlv(data, at)?;
            let value = std::str::from_utf8(data.get(start..end)?).ok()?;
            let (count, format) = match tag {
                0x17 => (13, "%y%m%d%H%M%SZ"),
                0x18 => (15, "%Y%m%d%H%M%SZ"),
                _ => return None,
            };
            if value.len() != count || !value.ends_with('Z') {
                return None;
            }
            let time = chrono::NaiveDateTime::parse_from_str(value, format).ok()?;
            Some((time.and_utc().timestamp(), end))
        }
        let (not_before, next) = timestamp(tbs, inner)?;
        let (not_after, tail) = timestamp(tbs, next)?;
        if tail != end || not_after <= not_before {
            return None;
        }
        let now = i64::try_from(now.as_secs()).ok()?;
        Some(now >= not_before && now < not_after)
    }
    check(cert, now).unwrap_or(false)
}

impl ServerCertVerifier for PinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        // EasyHostAgent emits exactly one self-signed leaf; certificates cannot
        // be substituted using a public CA, a redirect, or a malicious gateway.
        if !intermediates.is_empty() {
            return Err(TlsError::General(
                "unexpected Agent certificate chain".into(),
            ));
        }
        if !valid_at(end_entity.as_ref(), now) {
            return Err(TlsError::General(
                "Agent certificate expired or not yet valid".into(),
            ));
        }
        let spki = spki_der(end_entity.as_ref())
            .ok_or_else(|| TlsError::General("invalid Agent certificate SPKI".into()))?;
        let actual = Sha256::digest(spki);
        if self.0.as_slice().ct_eq(actual.as_slice()).unwrap_u8() != 1 {
            return Err(TlsError::General(
                "Agent TLS SPKI fingerprint mismatch".into(),
            ));
        }
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        Err(TlsError::General("TLS 1.2 is disabled".into()))
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        sig: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            sig,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
pub(crate) fn pinned_client(pin: &Fingerprint) -> Result<reqwest::Client, String> {
    let crypto = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(crypto)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|_| "cannot configure TLS 1.3")?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinVerifier(pin.0)))
        .with_no_client_auth();
    reqwest::Client::builder()
        .tls_backend_preconfigured(config)
        .no_proxy()
        .https_only(true)
        .http1_only()
        .timeout(Duration::from_secs(8))
        .connect_timeout(Duration::from_secs(4))
        .redirect(reqwest::redirect::Policy::none())
        .pool_max_idle_per_host(0)
        .retry(reqwest::retry::never())
        .build()
        .map_err(|_| "cannot initialize pinned TLS client".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pin_validation() {
        assert!(Fingerprint::parse(&"a".repeat(64)).is_ok());
        assert!(Fingerprint::parse(&"A".repeat(64)).is_ok());
        for invalid in ["", "test", "a", "0x123", "g".repeat(64).as_str()] {
            assert!(Fingerprint::parse(invalid).is_err());
        }
    }
    #[test]
    fn pinned_x509_spki_and_certificate_dates_match_external_fixture() {
        use base64::Engine as _;
        // ECDSA P-256 X.509 generated independently using cryptography.
        let encoded = "MIIBgDCCASWgAwIBAgIEALxhTjAKBggqhkjOPQQDAjA9MTswOQYDVQQDDDJFYXN5SG9zdEFnZW50IDExMTExMTExLTExMTEtNDExMS04MTExLTExMTExMTExMTExMTAeFw0yNTAxMDEwMDAwMDBaFw0zMTAxMDEwMDAwMDBaMD0xOzA5BgNVBAMMMkVhc3lIb3N0QWdlbnQgMTExMTExMTEtMTExMS00MTExLTgxMTEtMTExMTExMTExMTExMFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE+7vu1ZvVcQ7tEeDxLAKD8iggRr3qg16WJInpbFtef6gNLG96zWHXPIKukgdkgAt13APnR8vpLv98yKjDe5jQMqMTMBEwDwYDVR0TAQH/BAUwAwEB/zAKBggqhkjOPQQDAgNJADBGAiEA0zLUygpFHbNzsU3mbaHa3QDBVw9DBE1x5aaTd+loMQYCIQD+QTlu/EMej6Lidkl0SHhvfALdIvX23lsXTYK4r0tFlw==";
        let cert = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .expect("fixture DER");
        let spki = spki_der(&cert).expect("certificate SPKI must be parsable");
        let actual = Sha256::digest(spki);
        assert_eq!(
            actual.as_slice(),
            Fingerprint::parse("9344ddfa06099f5822cc347d660519bb0ab8dc371f19ae85f35281030c22999e")
                .unwrap().0.as_slice()
        );
        let in_date = UnixTime::since_unix_epoch(Duration::from_secs(1_800_000_000));
        let expired = UnixTime::since_unix_epoch(Duration::from_secs(2_100_000_000));
        assert!(valid_at(&cert, in_date));
        assert!(!valid_at(&cert, expired));
        assert!(PinVerifier(Fingerprint::parse("9344ddfa06099f5822cc347d660519bb0ab8dc371f19ae85f35281030c22999e").unwrap().0)
            .verify_server_cert(&CertificateDer::from(cert.clone()), &[], &ServerName::try_from("host").unwrap(), &[], in_date).is_ok());
        assert!(PinVerifier([0u8;32])
            .verify_server_cert(&CertificateDer::from(cert), &[], &ServerName::try_from("host").unwrap(), &[], in_date).is_err());
    }
    #[test]
    fn malformed_x509_rejected() {
        for bad in [
            vec![],
            vec![0x30, 0x80],
            vec![0x30, 0x02, 0xff, 0xff],
            vec![0x30, 0x00],
        ] {
            assert!(spki_der(&bad).is_none());
        }
    }
}
