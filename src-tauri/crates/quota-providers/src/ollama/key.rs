//! The signing key Ollama keeps on this computer, and the request signature
//! its CLI sends to ollama.com.
//!
//! Ollama creates an Ed25519 key at `.ollama/id_ed25519` in the user profile
//! (an unencrypted OpenSSH private key), and `ollama signin` links it to the
//! person's ollama.com account. A request to ollama.com is signed over
//! `<METHOD>,<path>?ts=<unix seconds>` and sent as
//! `Authorization: <public key>:<signature>`, both base64. Only the signature
//! leaves the computer, and the timestamp stops it being replayed. Quota reads
//! the key on every read and never changes it.

use quota_core::ports::ProviderError;
use ring::signature::Ed25519KeyPair;

use crate::credentials::{process_lookup, profile_directory};
use crate::decode::base64;
use crate::platform::Lookup;

/// The profile label a connection that uses Ollama's own sign-in carries.
pub(crate) const PROFILE: &str = "ollama-key";

/// The OpenSSH private key format's magic.
const MAGIC: &[u8] = b"openssh-key-v1\0";

/// The key type this signer accepts.
const KEY_TYPE: &[u8] = b"ssh-ed25519";

/// The largest key file read, in bytes.
const MAX_KEY_BYTES: u64 = 16 * 1024;

/// Ollama's signing key.
pub(crate) struct SigningKey {
    pair: Ed25519KeyPair,
    /// The public key in SSH wire form, base64.
    public: String,
}

impl SigningKey {
    /// The `Authorization` value for one request.
    pub(crate) fn authorization(&self, method: &str, request_uri: &str) -> String {
        let signature = self.pair.sign(format!("{method},{request_uri}").as_bytes());
        format!("{}:{}", self.public, base64::encode(signature.as_ref()))
    }
}

impl std::fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SigningKey(<redacted>)")
    }
}

/// Reads Ollama's key, when Ollama is installed.
pub(crate) async fn load() -> Result<SigningKey, ProviderError> {
    let lookup: Lookup<'_> = &process_lookup;
    let path = profile_directory(crate::platform::system(), lookup)?
        .join(".ollama")
        .join("id_ed25519");
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| ProviderError::Authentication)?;
    if !metadata.is_file() || metadata.len() > MAX_KEY_BYTES {
        return Err(ProviderError::Authentication);
    }
    let text = tokio::fs::read_to_string(&path)
        .await
        .map_err(|_| ProviderError::Authentication)?;
    parse(&text).ok_or_else(|| ProviderError::InvalidData {
        detail: "Ollama's key is not an unencrypted Ed25519 key".to_owned(),
    })
}

/// Parses an unencrypted OpenSSH Ed25519 private key.
fn parse(pem: &str) -> Option<SigningKey> {
    let body: String = pem
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("-----"))
        .collect();
    let blob = base64::decode(&body)?;
    let mut reader = Reader(blob.strip_prefix(MAGIC)?);
    // Only an unencrypted key: cipher and KDF "none", no KDF options.
    if reader.string()? != b"none" || reader.string()? != b"none" {
        return None;
    }
    reader.string()?;
    if reader.number()? != 1 {
        return None;
    }
    let public_blob = reader.string()?;
    let mut private = Reader(reader.string()?);
    let (check, again) = (private.number()?, private.number()?);
    if check != again || private.string()? != KEY_TYPE {
        return None;
    }
    private.string()?;
    let secret = private.string()?;
    let (seed, public_raw) = (secret.get(..32)?, secret.get(32..64)?);
    let pair = Ed25519KeyPair::from_seed_and_public_key(seed, public_raw).ok()?;
    let mut public = Reader(public_blob);
    if public.string()? != KEY_TYPE || public.string()? != public_raw {
        return None;
    }
    Some(SigningKey {
        pair,
        public: base64::encode(public_blob),
    })
}

/// Reads SSH wire fields: big-endian lengths followed by their bytes.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn number(&mut self) -> Option<u32> {
        let (head, rest) = self.0.split_at_checked(4)?;
        self.0 = rest;
        Some(u32::from_be_bytes(head.try_into().ok()?))
    }

    fn string(&mut self) -> Option<&'a [u8]> {
        let length = usize::try_from(self.number()?).ok()?;
        let (value, rest) = self.0.split_at_checked(length)?;
        self.0 = rest;
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An unencrypted OpenSSH Ed25519 key, built from a fixed seed so no
    /// private key file is kept in the repository.
    fn test_key() -> String {
        fn field(out: &mut Vec<u8>, bytes: &[u8]) {
            out.extend_from_slice(&u32::try_from(bytes.len()).expect("short").to_be_bytes());
            out.extend_from_slice(bytes);
        }
        let seed = [7_u8; 32];
        let pair = Ed25519KeyPair::from_seed_unchecked(&seed).expect("a key");
        let public_raw = ring::signature::KeyPair::public_key(&pair)
            .as_ref()
            .to_vec();
        let mut public_blob = Vec::new();
        field(&mut public_blob, KEY_TYPE);
        field(&mut public_blob, &public_raw);
        let mut private = vec![0, 0, 0, 42, 0, 0, 0, 42];
        field(&mut private, KEY_TYPE);
        field(&mut private, &public_raw);
        field(
            &mut private,
            &[seed.as_slice(), public_raw.as_slice()].concat(),
        );
        field(&mut private, b"quota-test");
        private.extend_from_slice(&[1, 2, 3, 4, 5]);
        let mut blob = MAGIC.to_vec();
        field(&mut blob, b"none");
        field(&mut blob, b"none");
        field(&mut blob, b"");
        blob.extend_from_slice(&1_u32.to_be_bytes());
        field(&mut blob, &public_blob);
        field(&mut blob, &private);
        format!(
            "-----BEGIN OPENSSH PRIVATE KEY-----\n{}\n-----END OPENSSH PRIVATE KEY-----\n",
            base64::encode(&blob)
        )
    }

    #[test]
    fn an_openssh_key_signs_the_request_line() {
        let key = parse(&test_key()).expect("the test key parses");
        let header = key.authorization("GET", "/api/usage?ts=1700000000");
        let (public, signature) = header.split_once(':').expect("public:signature");
        assert!(
            public.starts_with("AAAAC3NzaC1lZDI1NTE5"),
            "the SSH wire form of the public key"
        );
        let signature = base64::decode(signature).expect("base64");
        assert_eq!(signature.len(), 64);
        // The signature verifies against the key's own public half.
        let public_raw = &base64::decode(public).expect("base64")[19..];
        ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public_raw)
            .verify(b"GET,/api/usage?ts=1700000000", &signature)
            .expect("the signature verifies");
    }

    #[test]
    fn anything_but_an_unencrypted_ed25519_key_is_refused() {
        assert!(parse("not a key").is_none());
        assert!(parse("-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA\n-----END").is_none());
    }
}
