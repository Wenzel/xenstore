/// Some NUL-string payload related utilities.
///
use std::{
    io::Write,
    str::{self, Utf8Error},
};

pub fn make_payload(strings: &[&str]) -> Box<[u8]> {
    let mut payload: Vec<u8> = Vec::new();

    for s in strings {
        payload.write_all(s.as_bytes()).unwrap(); // infailble
        payload.push(0);
    }

    payload.into_boxed_slice()
}

pub fn parse_nul_string(mut buffer: &[u8]) -> Result<Option<&str>, Utf8Error> {
    // Assuming terminating NUL
    if buffer.is_empty() {
        Ok(None)
    } else {
        // Discard latest NUL character (if present)
        if buffer.last() == Some(&0) {
            buffer = &buffer[..buffer.len() - 1];
        }

        Some(str::from_utf8(buffer)).transpose()
    }
}

pub fn parse_nul_list(buffer: &[u8]) -> Result<Box<[&str]>, Utf8Error> {
    buffer
        .split_inclusive(|&c| c == 0)
        .filter_map(|s| parse_nul_string(s).transpose())
        .collect()
}
