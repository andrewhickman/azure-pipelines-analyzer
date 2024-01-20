use std::{
    borrow::Cow,
    char::DecodeUtf16Error,
    error::Error,
    fmt,
    str::{self, Utf8Error},
};

pub(crate) fn decode(text: &[u8]) -> Result<Cow<'_, str>, DecodeError> {
    match text {
        // Explicit BOM
        [0x00, 0x00, 0xfe, 0xff, ..] => decode_utf32_be(text).map(Cow::Owned),
        // ASCII first character
        [0x00, 0x00, 0x00, _, ..] => decode_utf32_be(text).map(Cow::Owned),
        // Explicit BOM
        [0xff, 0xfe, 0x00, 0x00, ..] => decode_utf32_le(text).map(Cow::Owned),
        // ASCII first character
        [_, 0x00, 0x00, 0x00, ..] => decode_utf32_le(text).map(Cow::Owned),
        // Explicit BOM
        [0xfe, 0xff, ..] => decode_utf16_be(text).map(Cow::Owned),
        // ASCII first character
        [0x00, _, ..] => decode_utf16_be(text).map(Cow::Owned),
        // Explicit BOM
        [0xff, 0xfe, ..] => decode_utf16_le(text).map(Cow::Owned),
        // ASCII first character
        [_, 0x00, ..] => decode_utf16_le(text).map(Cow::Owned),
        // Explicit BOM
        [0xef, 0xbb, 0xbf, ..] => decode_utf8(text).map(Cow::Borrowed),
        // Default
        _ => decode_utf8(text).map(Cow::Borrowed),
    }
}

#[derive(Debug)]
pub(crate) enum DecodeError {
    Utf8(Utf8Error),
    Utf16(Option<DecodeUtf16Error>),
    Utf32,
}

fn decode_utf32_be(text: &[u8]) -> Result<String, DecodeError> {
    if text.len() % 4 != 0 {
        return Err(DecodeError::Utf32);
    }

    text.chunks(4)
        .map(|chunk| [chunk[0], chunk[1], chunk[2], chunk[3]])
        .map(u32::from_be_bytes)
        .map(char::from_u32)
        .collect::<Option<String>>()
        .ok_or(DecodeError::Utf32)
}

fn decode_utf32_le(text: &[u8]) -> Result<String, DecodeError> {
    if text.len() % 4 != 0 {
        return Err(DecodeError::Utf32);
    }

    text.chunks(4)
        .map(|chunk| [chunk[0], chunk[1], chunk[2], chunk[3]])
        .map(u32::from_le_bytes)
        .map(char::from_u32)
        .collect::<Option<String>>()
        .ok_or(DecodeError::Utf32)
}

fn decode_utf16_be(text: &[u8]) -> Result<String, DecodeError> {
    if text.len() % 2 != 0 {
        return Err(DecodeError::Utf16(None));
    }

    char::decode_utf16(
        text.chunks(2)
            .map(|chunk| [chunk[0], chunk[1]])
            .map(u16::from_be_bytes),
    )
    .collect::<Result<String, DecodeUtf16Error>>()
    .map_err(|err| DecodeError::Utf16(Some(err)))
}

fn decode_utf16_le(text: &[u8]) -> Result<String, DecodeError> {
    if text.len() % 2 != 0 {
        return Err(DecodeError::Utf16(None));
    }

    char::decode_utf16(
        text.chunks(2)
            .map(|chunk| [chunk[0], chunk[1]])
            .map(u16::from_le_bytes),
    )
    .collect::<Result<String, DecodeUtf16Error>>()
    .map_err(|err| DecodeError::Utf16(Some(err)))
}

fn decode_utf8(text: &[u8]) -> Result<&str, DecodeError> {
    str::from_utf8(text).map_err(DecodeError::Utf8)
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Utf8(_) => write!(f, "source file was not valid utf-8"),
            DecodeError::Utf16(_) => write!(f, "source file was not valid utf-16"),
            DecodeError::Utf32 => write!(f, "source file was not valid utf-32"),
        }
    }
}

impl Error for DecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DecodeError::Utf8(err) => Some(err),
            DecodeError::Utf16(Some(err)) => Some(err),
            _ => None,
        }
    }
}
