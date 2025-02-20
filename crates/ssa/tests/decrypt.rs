use std::io::Cursor;

use iori_ssa::decrypt;

const KEY: [u8; 16] = u128::to_be_bytes(0xa8cda0ee5390b716298ffad0a1f1a021);
const IV: [u8; 16] = u128::to_be_bytes(0xE60C79C314E3C9B471E7E51ABAA0B24A);

#[test]
fn decrypt_ac3() {
    let mut encrypted = Cursor::new(include_bytes!("fixtures/ac3/segment-0.ts"));
    let mut decrypted = Vec::new();
    let expected_decrypted = include_bytes!("fixtures/ac3/segment-0.ts.dec");

    decrypt(&mut encrypted, &mut decrypted, KEY, IV).unwrap();
    assert_eq!(decrypted, expected_decrypted);
}

#[test]
fn decrypt_eac3() {
    let mut encrypted = Cursor::new(include_bytes!("fixtures/eac3/segment-0.ts"));
    let mut decrypted = Vec::new();
    let expected_decrypted = include_bytes!("fixtures/eac3/segment-0.ts.dec");

    decrypt(&mut encrypted, &mut decrypted, KEY, IV).unwrap();
    assert_eq!(decrypted, expected_decrypted);
}
