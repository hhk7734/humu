use humu::notification::crypto;

#[test]
fn round_trip_encrypt_decrypt() {
    let plaintext = "123456:ABCDEF";
    let encrypted = crypto::encrypt(plaintext).unwrap();
    let decrypted = crypto::decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn different_plaintexts_produce_different_ciphertexts() {
    let a = crypto::encrypt("aaa").unwrap();
    let b = crypto::encrypt("bbb").unwrap();
    assert_ne!(a, b);
}

#[test]
fn same_plaintext_produces_different_ciphertexts() {
    let a = crypto::encrypt("same").unwrap();
    let b = crypto::encrypt("same").unwrap();
    assert_ne!(a, b);
}

#[test]
fn tampered_ciphertext_fails() {
    let encrypted = crypto::encrypt("secret").unwrap();
    let mut bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &encrypted,
    ).unwrap();
    if let Some(b) = bytes.last_mut() {
        *b ^= 0xFF;
    }
    let tampered = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &bytes,
    );
    assert!(crypto::decrypt(&tampered).is_err());
}

#[test]
fn empty_string_round_trips() {
    let encrypted = crypto::encrypt("").unwrap();
    let decrypted = crypto::decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, "");
}
