use std::io::BufReader;
use mapsforge_rs::utils::{read_vbe_u, read_vbe_u_int, read_vbe_s_int};

#[test]
fn test_read_vbe_u() {
    // Test simple string
    let data = [0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f]; // "Hello" with length 5
    let mut reader = BufReader::new(&data[..]);
    assert_eq!(read_vbe_u(&mut reader).unwrap(), "Hello");

    // Test empty string
    let data = [0x00];
    let mut reader = BufReader::new(&data[..]);
    assert_eq!(read_vbe_u(&mut reader).unwrap(), "");

    // Test string with multi-byte length
    let long_string = "A".repeat(130);
    let mut data = vec![0x82, 0x01]; // Length 130 in VBE format
    data.extend(long_string.as_bytes());
    let mut reader = BufReader::new(&data[..]);
    assert_eq!(read_vbe_u(&mut reader).unwrap(), long_string);
}

#[test]
fn test_read_vbe_u_int() {
    // Test small number
    let data = [0x05];
    let mut reader = BufReader::new(&data[..]);
    assert_eq!(read_vbe_u_int(&mut reader).unwrap(), 5);

    // Test zero
    let data = [0x00];
    let mut reader = BufReader::new(&data[..]);
    assert_eq!(read_vbe_u_int(&mut reader).unwrap(), 0);

    // Test larger number requiring multiple bytes
    let data = [0x82, 0x01]; // 130 in VBE format
    let mut reader = BufReader::new(&data[..]);
    assert_eq!(read_vbe_u_int(&mut reader).unwrap(), 130);
}

#[test]
fn test_read_vbe_s_int() {
    // Test positive small number
    let data = [0x05];
    let mut reader = BufReader::new(&data[..]);
    assert_eq!(read_vbe_s_int(&mut reader).unwrap(), 5);

    // Test zero
    let data = [0x00];
    let mut reader = BufReader::new(&data[..]);
    assert_eq!(read_vbe_s_int(&mut reader).unwrap(), 0);

    // Test negative number
    let data = [0x45]; // -5 in VBE format
    let mut reader = BufReader::new(&data[..]);
    assert_eq!(read_vbe_s_int(&mut reader).unwrap(), -5);

    // Test larger negative number
    let data = [0xc2, 0x40]; // -66 in VBE format
    let mut reader = BufReader::new(&data[..]);
    assert_eq!(read_vbe_s_int(&mut reader).unwrap(), -66);

    // Test even larger negative number
    let data = [0x81, 0x41]; // -129 in VBE format
    let mut reader = BufReader::new(&data[..]);
    assert_eq!(read_vbe_s_int(&mut reader).unwrap(), -129);
    
}