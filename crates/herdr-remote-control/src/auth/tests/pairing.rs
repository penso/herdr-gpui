use super::*;
use crate::auth::pairing::{APPROVAL_TTL_MS, TOKEN_TTL_MS, matching_code};

fn claim(pairings: &mut Pairings, offer: &Offer, seed: u8, now_ms: u64) -> Result<Claim> {
    pairings.claim(
        &token(offer),
        "iPhone",
        &public_key(&signing_key(seed)),
        now_ms,
    )
}

#[test]
fn an_offer_carries_the_token_and_the_bare_fingerprint_in_its_fragment() {
    let mut pairings = Pairings::default();
    let offer = pairings.mint(T0, &origins(), FINGERPRINT).unwrap();
    assert!(offer.url.starts_with(&format!("{ORIGIN}/pair#t=")));
    assert!(offer.url.ends_with("&fp=ABCDEF01"));
    assert_eq!(decode_fixed::<32>(&token(&offer)).unwrap().len(), 32);
    assert_eq!(offer.fingerprint, FINGERPRINT);
    assert_eq!(offer.expires_in_ms, TOKEN_TTL_MS);
    assert_eq!(offer.addresses, origins());
}

#[test]
fn minting_needs_an_address() {
    let error = Pairings::default().mint(T0, &[], FINGERPRINT).unwrap_err();
    assert!(matches!(error, Error::NoAddress));
}

#[test]
fn a_token_expires_after_sixty_seconds() {
    let mut pairings = Pairings::default();
    let offer = pairings.mint(T0, &origins(), FINGERPRINT).unwrap();
    assert_eq!(
        pairings.state(&offer.id, T0 + TOKEN_TTL_MS - 1).unwrap(),
        PairingState::Waiting { expires_in_ms: 1 }
    );
    let error = claim(&mut pairings, &offer, 1, T0 + TOKEN_TTL_MS).unwrap_err();
    assert!(matches!(error, Error::ExpiredToken));
    assert_eq!(
        pairings.state(&offer.id, T0 + TOKEN_TTL_MS).unwrap(),
        PairingState::Expired
    );

    let offer = pairings.mint(T0, &origins(), FINGERPRINT).unwrap();
    claim(&mut pairings, &offer, 1, T0 + TOKEN_TTL_MS - 1).unwrap();
}

#[test]
fn a_second_claim_of_a_token_denies_the_pairing() {
    let mut pairings = Pairings::default();
    let offer = pairings.mint(T0, &origins(), FINGERPRINT).unwrap();
    let first = claim(&mut pairings, &offer, 1, T0).unwrap();
    let error = claim(&mut pairings, &offer, 2, T0 + 1).unwrap_err();
    assert!(matches!(error, Error::AlreadyClaimed));
    assert_eq!(
        pairings.state(&offer.id, T0 + 2).unwrap(),
        PairingState::Denied
    );
    assert_eq!(
        pairings.claim_status(&first.id, T0 + 2).unwrap(),
        PairingState::Denied
    );
    let error = pairings.answer(&offer.id, true, T0 + 2, keep).unwrap_err();
    assert!(matches!(error, Error::NotClaimed));
}

#[test]
fn rotating_voids_the_old_token_and_keeps_the_pairing() {
    let mut pairings = Pairings::default();
    let old = pairings.mint(T0, &origins(), FINGERPRINT).unwrap();
    let new = pairings.mint(T0 + 50_000, &origins(), FINGERPRINT).unwrap();
    assert_eq!(old.id, new.id);
    assert_ne!(token(&old), token(&new));
    let error = claim(&mut pairings, &old, 1, T0 + 50_001).unwrap_err();
    assert!(matches!(error, Error::InvalidToken));
    // The rotated token lives sixty seconds from its own minting.
    claim(&mut pairings, &new, 1, T0 + 50_000 + TOKEN_TTL_MS - 1).unwrap();
}

#[test]
fn an_expired_unclaimed_pairing_rotates_back_to_waiting() {
    let mut pairings = Pairings::default();
    let old = pairings.mint(T0, &origins(), FINGERPRINT).unwrap();
    assert_eq!(
        pairings.state(&old.id, T0 + TOKEN_TTL_MS).unwrap(),
        PairingState::Expired
    );
    let new = pairings
        .mint(T0 + TOKEN_TTL_MS, &origins(), FINGERPRINT)
        .unwrap();
    assert_eq!(old.id, new.id);
    assert!(matches!(
        pairings.state(&new.id, T0 + TOKEN_TTL_MS).unwrap(),
        PairingState::Waiting { .. }
    ));
}

#[test]
fn minting_after_a_claim_starts_a_new_pairing_and_denies_the_claim() {
    let mut pairings = Pairings::default();
    let old = pairings.mint(T0, &origins(), FINGERPRINT).unwrap();
    let claimed = claim(&mut pairings, &old, 1, T0).unwrap();
    let new = pairings.mint(T0 + 1, &origins(), FINGERPRINT).unwrap();
    assert_ne!(old.id, new.id);
    assert!(matches!(
        pairings.state(&old.id, T0 + 1).unwrap_err(),
        Error::UnknownPairing
    ));
    assert_eq!(
        pairings.claim_status(&claimed.id, T0 + 1).unwrap(),
        PairingState::Denied
    );
}

#[test]
fn the_matching_code_is_six_digits_and_the_same_when_recomputed() {
    let code = matching_code(&[7; 32], &[4; 65], &[9; 32]);
    assert_eq!(code.len(), 6);
    assert!(code.bytes().all(|byte| byte.is_ascii_digit()));
    assert_eq!(code, matching_code(&[7; 32], &[4; 65], &[9; 32]));
    assert_ne!(code, matching_code(&[7; 32], &[4; 65], &[8; 32]));
    // Zero-padded: a digest whose first bytes are small still gives six digits.
    for nonce in 0..=255u8 {
        assert_eq!(matching_code(&[1; 32], &[2; 65], &[nonce; 32]).len(), 6);
    }

    let mut pairings = Pairings::default();
    let offer = pairings.mint(T0, &origins(), FINGERPRINT).unwrap();
    let claimed = claim(&mut pairings, &offer, 1, T0).unwrap();
    let expected = PairingState::Claimed {
        device_name: "iPhone".into(),
        code: claimed.code.clone(),
    };
    assert_eq!(pairings.state(&offer.id, T0).unwrap(), expected);
    assert_eq!(pairings.claim_status(&claimed.id, T0).unwrap(), expected);
}

#[test]
fn an_unanswered_claim_expires_after_two_minutes() {
    let mut pairings = Pairings::default();
    let offer = pairings.mint(T0, &origins(), FINGERPRINT).unwrap();
    let claimed = claim(&mut pairings, &offer, 1, T0).unwrap();
    assert_eq!(claimed.expires_ms, T0 + APPROVAL_TTL_MS);
    let error = pairings
        .answer(&offer.id, true, T0 + APPROVAL_TTL_MS, keep)
        .unwrap_err();
    assert!(matches!(error, Error::NotClaimed));
    assert_eq!(
        pairings.state(&offer.id, T0 + APPROVAL_TTL_MS).unwrap(),
        PairingState::Expired
    );
    assert_eq!(
        pairings
            .claim_status(&claimed.id, T0 + APPROVAL_TTL_MS)
            .unwrap(),
        PairingState::Expired
    );
}

#[test]
fn approval_just_before_the_deadline_returns_the_device() {
    let mut pairings = Pairings::default();
    let offer = pairings.mint(T0, &origins(), FINGERPRINT).unwrap();
    let claimed = claim(&mut pairings, &offer, 1, T0).unwrap();
    let device = pairings
        .answer(&offer.id, true, T0 + APPROVAL_TTL_MS - 1, keep)
        .unwrap()
        .unwrap();
    assert_eq!(device.name, "iPhone");
    assert_eq!(
        device.public_key,
        PublicKey::parse(&public_key(&signing_key(1))).unwrap()
    );
    let approved = PairingState::Approved { device: device.id };
    assert_eq!(pairings.state(&offer.id, T0 + 1).unwrap(), approved);
    assert_eq!(
        pairings.claim_status(&claimed.id, T0 + 1).unwrap(),
        approved
    );
}

#[test]
fn denying_tells_both_sides() {
    let mut pairings = Pairings::default();
    let offer = pairings.mint(T0, &origins(), FINGERPRINT).unwrap();
    let claimed = claim(&mut pairings, &offer, 1, T0).unwrap();
    assert_eq!(pairings.answer(&offer.id, false, T0, keep).unwrap(), None);
    assert_eq!(pairings.state(&offer.id, T0).unwrap(), PairingState::Denied);
    assert_eq!(
        pairings.claim_status(&claimed.id, T0).unwrap(),
        PairingState::Denied
    );
}

#[test]
fn only_a_claimed_pairing_takes_an_answer() {
    let mut pairings = Pairings::default();
    let offer = pairings.mint(T0, &origins(), FINGERPRINT).unwrap();
    assert!(matches!(
        pairings.answer(&offer.id, true, T0, keep).unwrap_err(),
        Error::NotClaimed
    ));
    assert!(matches!(
        pairings
            .answer(&PairingId("nope".into()), true, T0, keep)
            .unwrap_err(),
        Error::UnknownPairing
    ));
    assert!(matches!(
        pairings
            .claim_status(&ClaimId("nope".into()), T0)
            .unwrap_err(),
        Error::UnknownClaim
    ));
}

#[test]
fn cancelling_voids_the_token_and_denies_a_claim() {
    let mut pairings = Pairings::default();
    let offer = pairings.mint(T0, &origins(), FINGERPRINT).unwrap();
    let claimed = claim(&mut pairings, &offer, 1, T0).unwrap();
    pairings.cancel(&offer.id).unwrap();
    assert!(matches!(
        pairings.state(&offer.id, T0).unwrap_err(),
        Error::UnknownPairing
    ));
    assert!(matches!(
        pairings.cancel(&offer.id).unwrap_err(),
        Error::UnknownPairing
    ));
    assert_eq!(
        pairings.claim_status(&claimed.id, T0).unwrap(),
        PairingState::Denied
    );
    let error = claim(&mut pairings, &offer, 2, T0).unwrap_err();
    assert!(matches!(error, Error::InvalidToken));
}

#[test]
fn a_rejected_name_or_key_leaves_the_token_usable() {
    let mut pairings = Pairings::default();
    let offer = pairings.mint(T0, &origins(), FINGERPRINT).unwrap();
    let key = public_key(&signing_key(1));
    let error = pairings.claim(&token(&offer), " ", &key, T0).unwrap_err();
    assert!(matches!(error, Error::InvalidName));
    let error = pairings
        .claim(&token(&offer), "iPhone", "AAAA", T0)
        .unwrap_err();
    assert!(matches!(error, Error::InvalidKey));
    claim(&mut pairings, &offer, 1, T0).unwrap();
}

#[test]
fn malformed_or_wrong_tokens_are_invalid() {
    let mut pairings = Pairings::default();
    let key = public_key(&signing_key(1));
    // No pairing open at all.
    let error = pairings
        .claim(&encode(&[0; 32]), "iPhone", &key, T0)
        .unwrap_err();
    assert!(matches!(error, Error::InvalidToken));

    let offer = pairings.mint(T0, &origins(), FINGERPRINT).unwrap();
    let real = token(&offer);
    for bad in [
        String::new(),
        "!".repeat(43),
        format!("{real}A"),
        format!("{real}="),
        real[..42].to_owned(),
        encode(&[0; 32]),
        "A".repeat(10_000),
    ] {
        let error = pairings.claim(&bad, "iPhone", &key, T0).unwrap_err();
        assert!(matches!(error, Error::InvalidToken), "{bad:?}: {error:?}");
    }
    claim(&mut pairings, &offer, 1, T0).unwrap();
}

#[test]
fn a_phone_can_poll_only_a_bounded_number_of_finished_claims() {
    let mut pairings = Pairings::default();
    let mut claims = Vec::new();
    for round in 0..12 {
        let offer = pairings.mint(T0 + round, &origins(), FINGERPRINT).unwrap();
        claims.push(claim(&mut pairings, &offer, 1, T0 + round).unwrap().id);
        pairings.answer(&offer.id, false, T0 + round, keep).unwrap();
    }
    assert!(matches!(
        pairings.claim_status(&claims[0], T0 + 20).unwrap_err(),
        Error::UnknownClaim
    ));
    assert_eq!(
        pairings.claim_status(&claims[11], T0 + 20).unwrap(),
        PairingState::Denied
    );
}

#[test]
fn the_facade_reports_and_cancels_the_dialogs_pairing() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let offer = auth.mint(T0, &origins(), FINGERPRINT).unwrap();
    assert_eq!(
        auth.pairing(&offer.id, T0 + 10_000).unwrap(),
        PairingState::Waiting {
            expires_in_ms: TOKEN_TTL_MS - 10_000
        }
    );
    auth.cancel(&offer.id).unwrap();
    assert!(matches!(
        auth.pairing(&offer.id, T0).unwrap_err(),
        Error::UnknownPairing
    ));
}

#[test]
fn an_approval_for_another_code_denies_the_pairing() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = Auth::load(dir.path(), Limits::default()).unwrap();
    let offer = auth.mint(T0, &origins(), FINGERPRINT).unwrap();
    let claim = auth
        .claim(
            IpAddr::from([10, 0, 0, 3]),
            &token(&offer),
            "iPhone",
            &public_key(&signing_key(2)),
            T0,
        )
        .unwrap();
    let wrong = if claim.code == "000000" {
        "000001"
    } else {
        "000000"
    };
    assert!(matches!(
        auth.answer(&offer.id, true, wrong, T0).unwrap_err(),
        Error::CodeMismatch
    ));
    assert_eq!(auth.pairing(&offer.id, T0).unwrap(), PairingState::Denied);
    assert_eq!(auth.devices().len(), 0);
}
