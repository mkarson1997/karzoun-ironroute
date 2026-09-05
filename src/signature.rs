use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::http::{HeaderMap, HeaderName, Method, Uri};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::config::HmacConfig;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub struct HmacVerifier {
    secret: Vec<u8>,
    signature_header: HeaderName,
    timestamp_header: HeaderName,
    max_clock_skew: Duration,
    protected_prefixes: Vec<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SignatureError {
    #[error("missing signature headers")]
    Missing,
    #[error("invalid timestamp")]
    Timestamp,
    #[error("request timestamp outside allowed clock skew")]
    ClockSkew,
    #[error("invalid signature encoding")]
    Encoding,
    #[error("signature verification failed")]
    Invalid,
    #[error("invalid configured header name")]
    HeaderName,
}

impl HmacVerifier {
    pub fn from_config(config: &HmacConfig, secret: Vec<u8>) -> Result<Self, SignatureError> {
        let signature_header = HeaderName::from_bytes(config.signature_header.as_bytes())
            .map_err(|_| SignatureError::HeaderName)?;
        let timestamp_header = HeaderName::from_bytes(config.timestamp_header.as_bytes())
            .map_err(|_| SignatureError::HeaderName)?;
        Ok(Self {
            secret,
            signature_header,
            timestamp_header,
            max_clock_skew: Duration::from_secs(config.max_clock_skew_seconds),
            protected_prefixes: config.protected_prefixes.clone(),
        })
    }

    pub fn protects(&self, path: &str) -> bool {
        self.protected_prefixes
            .iter()
            .any(|prefix| path.starts_with(prefix))
    }

    pub fn verify(
        &self,
        method: &Method,
        uri: &Uri,
        headers: &HeaderMap,
        body: &[u8],
    ) -> Result<(), SignatureError> {
        let timestamp = headers
            .get(&self.timestamp_header)
            .ok_or(SignatureError::Missing)?
            .to_str()
            .map_err(|_| SignatureError::Timestamp)?
            .parse::<u64>()
            .map_err(|_| SignatureError::Timestamp)?;
        let signature = headers
            .get(&self.signature_header)
            .ok_or(SignatureError::Missing)?
            .to_str()
            .map_err(|_| SignatureError::Encoding)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| SignatureError::Timestamp)?
            .as_secs();
        let skew = now.abs_diff(timestamp);
        if skew > self.max_clock_skew.as_secs() {
            return Err(SignatureError::ClockSkew);
        }

        let body_hash = Sha256::digest(body);
        let path = uri.path_and_query().map_or("/", |value| value.as_str());
        let canonical = format!(
            "{}\n{}\n{}\n{}",
            method.as_str(),
            path,
            timestamp,
            hex::encode(body_hash)
        );
        let expected = hex::decode(signature).map_err(|_| SignatureError::Encoding)?;
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).map_err(|_| SignatureError::Invalid)?;
        mac.update(canonical.as_bytes());
        mac.verify_slice(&expected)
            .map_err(|_| SignatureError::Invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn verifies_valid_signature_and_rejects_tampering() {
        let config = HmacConfig {
            secret_env: "TEST".into(),
            signature_header: "x-ironroute-signature".into(),
            timestamp_header: "x-ironroute-timestamp".into(),
            max_clock_skew_seconds: 60,
            protected_prefixes: vec!["/private".into()],
        };
        let verifier = HmacVerifier::from_config(&config, b"secret".to_vec()).unwrap();
        let method = Method::POST;
        let uri: Uri = "/private?a=1".parse().unwrap();
        let body = b"hello";
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let body_hash = Sha256::digest(body);
        let canonical = format!(
            "POST\n/private?a=1\n{}\n{}",
            timestamp,
            hex::encode(body_hash)
        );
        let mut mac = HmacSha256::new_from_slice(b"secret").unwrap();
        mac.update(canonical.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-ironroute-timestamp",
            HeaderValue::from_str(&timestamp.to_string()).unwrap(),
        );
        headers.insert(
            "x-ironroute-signature",
            HeaderValue::from_str(&signature).unwrap(),
        );
        assert!(verifier.verify(&method, &uri, &headers, body).is_ok());
        assert_eq!(
            verifier.verify(&method, &uri, &headers, b"tampered"),
            Err(SignatureError::Invalid)
        );
    }
}
