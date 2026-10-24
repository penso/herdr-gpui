//! RFC 8291 message encryption for Web Push, `aes128gcm` content coding
//! (RFC 8188): one record, so the push service relays bytes it cannot read.

use super::{
    Error, Result, Subscription,
    subscription::{AUTH_LEN, POINT_LEN},
    vapid,
};
use aes_gcm::{Aes128Gcm, KeyInit as _, aead::Aead as _};
use hkdf::Hkdf;
use p256::{PublicKey, SecretKey, elliptic_curve::zeroize::Zeroizing};
use sha2::Sha256;

/// The record size written in the header; the whole message is one record.
pub(super) const RECORD_SIZE: u32 = 4096;
/// The largest plaintext sent. Push services accept 4 KB of ciphertext;
/// this leaves room for the header, padding delimiter, and tag.
pub(crate) const MAX_PAYLOAD: usize = 3072;
const SALT_LEN: usize = 16;
/// Ends the record's content: the last (and only) record.
const DELIMITER: u8 = 0x02;

/// Encrypts `plaintext` for `subscription` with a fresh salt and key pair.
pub(crate) fn encrypt(subscription: &Subscription, plaintext: &[u8]) -> Result<Vec<u8>> {
    let mut salt = [0; SALT_LEN];
    getrandom::fill(&mut salt).map_err(Error::Random)?;
    let sender = vapid::generate()?;
    encrypt_with(
        &subscription.p256dh,
        &subscription.auth,
        plaintext,
        &salt,
        &sender,
    )
}

/// Encrypts with the given salt and sender key; RFC 8291's test vector
/// fixes both.
pub(super) fn encrypt_with(
    receiver: &PublicKey,
    auth: &[u8; AUTH_LEN],
    plaintext: &[u8],
    salt: &[u8; SALT_LEN],
    sender: &SecretKey,
) -> Result<Vec<u8>> {
    if plaintext.len() > MAX_PAYLOAD {
        return Err(Error::PayloadTooLarge {
            len: plaintext.len(),
        });
    }
    let receiver_point = receiver.to_sec1_bytes();
    let sender_point = sender.public_key().to_sec1_bytes();
    let shared = sender.diffie_hellman(receiver);

    // IKM = HKDF(auth, ecdh_secret, "WebPush: info" || 0 || ua_public || as_public)
    let mut key_info = Vec::with_capacity(14 + 2 * POINT_LEN);
    key_info.extend_from_slice(b"WebPush: info\0");
    key_info.extend_from_slice(&receiver_point);
    key_info.extend_from_slice(&sender_point);
    let mut ikm = Zeroizing::new([0; 32]);
    Hkdf::<Sha256>::new(Some(auth), shared.raw_secret_bytes())
        .expand(&key_info, ikm.as_mut_slice())
        .map_err(|_| Error::Encrypt)?;

    let content = Hkdf::<Sha256>::new(Some(salt), ikm.as_slice());
    let mut key = Zeroizing::new([0; 16]);
    content
        .expand(b"Content-Encoding: aes128gcm\0", key.as_mut_slice())
        .map_err(|_| Error::Encrypt)?;
    let mut nonce = [0; 12];
    content
        .expand(b"Content-Encoding: nonce\0", &mut nonce)
        .map_err(|_| Error::Encrypt)?;

    let mut record = Zeroizing::new(Vec::with_capacity(plaintext.len() + 1));
    record.extend_from_slice(plaintext);
    record.push(DELIMITER);
    let cipher = Aes128Gcm::new_from_slice(key.as_slice()).map_err(|_| Error::Encrypt)?;
    let ciphertext = cipher
        .encrypt(&nonce.into(), record.as_slice())
        .map_err(|_| Error::Encrypt)?;

    // Header: salt || record size || key id length || key id (as_public).
    let mut body = Vec::with_capacity(SALT_LEN + 5 + sender_point.len() + ciphertext.len());
    body.extend_from_slice(salt);
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(u8::try_from(sender_point.len()).map_err(|_| Error::Encrypt)?);
    body.extend_from_slice(&sender_point);
    body.extend_from_slice(&ciphertext);
    Ok(body)
}
