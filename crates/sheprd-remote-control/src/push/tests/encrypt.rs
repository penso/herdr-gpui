use super::*;
use crate::push::encrypt;
use crate::push::encrypt::{RECORD_SIZE, encrypt_with};
use p256::SecretKey;

/// RFC 8291 Appendix A: fixed sender key and salt give a known message.
#[test]
fn rfc8291_test_vector() {
    let receiver = subscription(FCM);
    let sender =
        SecretKey::from_slice(&b64("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw")).unwrap();
    assert_eq!(
        sender.public_key().to_sec1_bytes().as_ref(),
        b64(
            "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8"
        )
    );
    let salt: [u8; 16] = b64("DGv6ra1nlYgDCS1FRnbzlw").try_into().unwrap();

    let body = encrypt_with(
        &receiver.p256dh,
        &receiver.auth,
        b"When I grow up, I want to be a watermelon",
        &salt,
        &sender,
    )
    .unwrap();

    let expected = b64(concat!(
        "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27ml",
        "mlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPT",
        "pK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN",
    ));
    assert_eq!(body, expected);
}

#[test]
fn fresh_messages_use_a_new_salt_and_key_each_time() {
    let receiver = subscription(FCM);
    let first = encrypt(&receiver, b"hello").unwrap();
    let second = encrypt(&receiver, b"hello").unwrap();
    assert_ne!(first[..16], second[..16], "salt");
    assert_ne!(first[21..86], second[21..86], "sender key");
    assert_eq!(first[16..20], RECORD_SIZE.to_be_bytes());
    assert_eq!(first[20], 65);
    // Header, plaintext, delimiter, tag.
    assert_eq!(first.len(), 86 + 5 + 1 + 16);
}

#[test]
fn payloads_over_the_limit_are_refused() {
    let receiver = subscription(FCM);
    assert!(encrypt(&receiver, &[b'x'; MAX_PAYLOAD]).is_ok());
    let error = encrypt(&receiver, &[b'x'; MAX_PAYLOAD + 1]).unwrap_err();
    assert!(matches!(error, Error::PayloadTooLarge { len } if len == MAX_PAYLOAD + 1));
}
