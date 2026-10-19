//! Device identity and payload sealing.
//!
//! Two keys matter here. A **device key** is an ed25519 keypair: its public half
//! *is* the device's id, in hex, so any peer can verify an event knowing only
//! the author id it carries — there is no key directory to trust. A **group
//! key** is one symmetric key per baby; it seals event payloads so a peer that
//! holds only the readable headers cannot read them.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

use crate::error::{CoreError, Result};

/// Length of an ed25519 signature, in bytes.
pub const SIGNATURE_LEN: usize = 64;
/// Length of a device's ed25519 secret key, in bytes.
pub const SECRET_KEY_LEN: usize = 32;
/// Length of a device public key, in bytes.
pub const PUBLIC_KEY_LEN: usize = 32;
/// Length of the per-baby symmetric group key, in bytes.
pub const GROUP_KEY_LEN: usize = 32;
/// Length of the XChaCha20-Poly1305 nonce, in bytes.
pub const NONCE_LEN: usize = 24;

/// A device's ed25519 signing key.
pub struct DeviceKey(SigningKey);

impl DeviceKey {
    /// A fresh key from the operating system's random source.
    pub fn generate() -> Result<Self> {
        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret).map_err(random_error)?;
        Ok(Self(SigningKey::from_bytes(&secret)))
    }

    /// Rebuild a key from the secret bytes it was stored as.
    pub fn from_secret_bytes(secret: &[u8; SECRET_KEY_LEN]) -> Self {
        Self(SigningKey::from_bytes(secret))
    }

    /// The secret half, for this device's own storage only. Never share it.
    pub fn secret_bytes(&self) -> [u8; SECRET_KEY_LEN] {
        self.0.to_bytes()
    }

    /// This device's id: the hex of its public key.
    pub fn id(&self) -> String {
        to_hex(&self.0.verifying_key().to_bytes())
    }

    /// Sign `message`, returning the raw signature bytes.
    pub fn sign(&self, message: &[u8]) -> [u8; SIGNATURE_LEN] {
        self.0.sign(message).to_bytes()
    }
}

/// Verify `signature` over `message` against the device id named by `author`.
///
/// Every failure — a malformed author id, a key that cannot be parsed, a
/// mismatched key, a tampered message — reports the same `BadSignature`, so a
/// caller cannot tell a forgery attempt from a corrupt one.
pub fn verify_device(author: &str, message: &[u8], signature: &[u8; SIGNATURE_LEN]) -> Result<()> {
    let public = from_hex(author).ok_or(CoreError::BadSignature)?;
    let public: [u8; PUBLIC_KEY_LEN] = public.try_into().map_err(|_| CoreError::BadSignature)?;
    let key = VerifyingKey::from_bytes(&public).map_err(|_| CoreError::BadSignature)?;
    key.verify_strict(message, &Signature::from_bytes(signature))
        .map_err(|_| CoreError::BadSignature)
}

/// The symmetric key shared by every device in one baby's household.
pub struct GroupKey([u8; GROUP_KEY_LEN]);

impl GroupKey {
    /// A fresh key from the operating system's random source.
    pub fn generate() -> Result<Self> {
        let mut bytes = [0u8; GROUP_KEY_LEN];
        getrandom::fill(&mut bytes).map_err(random_error)?;
        Ok(Self(bytes))
    }

    pub fn from_bytes(bytes: [u8; GROUP_KEY_LEN]) -> Self {
        Self(bytes)
    }

    pub fn to_bytes(&self) -> [u8; GROUP_KEY_LEN] {
        self.0
    }

    /// Seal `plaintext`, binding `aad` to the result.
    ///
    /// A fresh random nonce is generated per call; the caller stores it beside
    /// the ciphertext, because the same key must never reuse one.
    pub fn seal(&self, aad: &[u8], plaintext: &[u8]) -> Result<([u8; NONCE_LEN], Vec<u8>)> {
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce).map_err(random_error)?;
        let sealed = self
            .cipher()
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| CoreError::Unseal)?;
        Ok((nonce, sealed))
    }

    /// Open a sealed payload. `aad` must be byte-identical to what was sealed
    /// with, so a payload cannot be moved to a different event.
    pub fn open(&self, aad: &[u8], nonce: &[u8; NONCE_LEN], ciphertext: &[u8]) -> Result<Vec<u8>> {
        self.cipher()
            .decrypt(
                &XNonce::from(*nonce),
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|_| CoreError::Unseal)
    }

    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(&Key::from(self.0))
    }
}

fn random_error(error: getrandom::Error) -> CoreError {
    CoreError::Random(error.to_string())
}

/// Lowercase hex, so ids are stable strings rather than raw bytes.
pub fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Parse lowercase or uppercase hex. `None` on any invalid character or an odd
/// length — the caller treats that as a bad signature.
pub(crate) fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(text.len() / 2);
    for pair in bytes.as_chunks::<2>().0 {
        let high = (pair[0] as char).to_digit(16)?;
        let low = (pair[1] as char).to_digit(16)?;
        out.push(((high << 4) | low) as u8);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_key_signs_and_verifies_under_its_own_id() {
        let key = DeviceKey::generate().unwrap();
        let message = b"an event";
        let signature = key.sign(message);

        assert!(verify_device(&key.id(), message, &signature).is_ok());
    }

    #[test]
    fn a_different_message_does_not_verify() {
        let key = DeviceKey::generate().unwrap();
        let signature = key.sign(b"one event");

        assert!(verify_device(&key.id(), b"another event", &signature).is_err());
    }

    #[test]
    fn another_device_cannot_claim_the_signature() {
        let signer = DeviceKey::generate().unwrap();
        let impostor = DeviceKey::generate().unwrap();
        let signature = signer.sign(b"an event");

        assert!(verify_device(&impostor.id(), b"an event", &signature).is_err());
    }

    #[test]
    fn a_malformed_author_id_is_a_bad_signature_not_a_panic() {
        let key = DeviceKey::generate().unwrap();
        let signature = key.sign(b"an event");

        for author in ["", "not hex", "abc", &"a".repeat(64)] {
            assert!(
                verify_device(author, b"an event", &signature).is_err(),
                "{author:?} should be rejected"
            );
        }
    }

    #[test]
    fn a_key_round_trips_through_its_secret_bytes() {
        let key = DeviceKey::generate().unwrap();
        let restored = DeviceKey::from_secret_bytes(&key.secret_bytes());

        assert_eq!(restored.id(), key.id());
    }

    #[test]
    fn a_sealed_payload_round_trips() {
        let group = GroupKey::generate().unwrap();
        let aad = b"headers";

        let (nonce, sealed) = group.seal(aad, b"120 ml").unwrap();

        assert_ne!(sealed, b"120 ml");
        assert_eq!(group.open(aad, &nonce, &sealed).unwrap(), b"120 ml");
    }

    #[test]
    fn the_wrong_group_key_cannot_open_a_payload() {
        let group = GroupKey::generate().unwrap();
        let other = GroupKey::generate().unwrap();
        let (nonce, sealed) = group.seal(b"headers", b"120 ml").unwrap();

        assert!(other.open(b"headers", &nonce, &sealed).is_err());
    }

    #[test]
    fn a_payload_cannot_be_moved_to_different_headers() {
        // The associated data is what stops a valid payload being replayed
        // under another event's headers.
        let group = GroupKey::generate().unwrap();
        let (nonce, sealed) = group.seal(b"event one", b"120 ml").unwrap();

        assert!(group.open(b"event two", &nonce, &sealed).is_err());
    }

    #[test]
    fn a_tampered_ciphertext_does_not_open() {
        let group = GroupKey::generate().unwrap();
        let (nonce, mut sealed) = group.seal(b"headers", b"120 ml").unwrap();
        sealed[0] ^= 0x01;

        assert!(group.open(b"headers", &nonce, &sealed).is_err());
    }

    #[test]
    fn hex_round_trips_and_rejects_junk() {
        let bytes = [0x00, 0x0f, 0xff, 0xa5];
        assert_eq!(to_hex(&bytes), "000fffa5");
        assert_eq!(from_hex("000fffa5"), Some(bytes.to_vec()));
        assert_eq!(from_hex("000FFFA5"), Some(bytes.to_vec()));

        assert_eq!(from_hex("abc"), None, "odd length");
        assert_eq!(from_hex("zz"), None, "not hex");
    }
}
