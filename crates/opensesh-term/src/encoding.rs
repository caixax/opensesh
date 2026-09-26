//! Character encodings other than UTF-8 (PLAN §6.2), through `encoding_rs`.
//!
//! The engine's parser reads UTF-8, so a session in a legacy encoding decodes the program's
//! output to UTF-8 before parsing it, and encodes what the user types back into the program's
//! encoding. Only ASCII-compatible encodings are offered
//! ([`opensesh_core::terminal::settings::ENCODINGS`]): escape sequences pass through unchanged.

use encoding_rs::{CoderResult, Decoder, Encoder, EncoderResult, Encoding, UTF_8};

/// Streaming conversion between one legacy encoding and UTF-8.
pub struct Codec {
    encoding: &'static Encoding,
    decoder: Decoder,
    encoder: Encoder,
}

impl std::fmt::Debug for Codec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Codec")
            .field("encoding", &self.encoding.name())
            .finish_non_exhaustive()
    }
}

impl Codec {
    /// A codec for the encoding called `name` (any WHATWG label). `None` for UTF-8, which needs
    /// no conversion, and for names `encoding_rs` doesn't know.
    #[must_use]
    pub fn new(name: &str) -> Option<Self> {
        let encoding = Encoding::for_label(name.trim().as_bytes())?;
        if encoding == UTF_8 {
            return None;
        }
        Some(Self {
            encoding,
            decoder: encoding.new_decoder_without_bom_handling(),
            encoder: encoding.new_encoder(),
        })
    }

    /// The encoding's canonical name.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.encoding.name()
    }

    /// Decodes program output, appending UTF-8 to `out`. A character split across calls is
    /// completed by the next call; invalid bytes become U+FFFD.
    pub fn decode(&mut self, mut input: &[u8], out: &mut Vec<u8>) {
        while !input.is_empty() {
            let room = self
                .decoder
                .max_utf8_buffer_length(input.len())
                .unwrap_or(input.len().saturating_mul(4).saturating_add(16));
            let start = out.len();
            out.resize(start + room, 0);
            let (result, read, written, _) =
                self.decoder.decode_to_utf8(input, &mut out[start..], false);
            out.truncate(start + written);
            input = &input[read..];
            if result == CoderResult::InputEmpty {
                break;
            }
        }
    }

    /// Encodes typed input (UTF-8) for the program. Characters the encoding lacks become `?`.
    /// Bytes that aren't UTF-8 (raw mouse reports) pass through unchanged.
    #[must_use]
    pub fn encode(&mut self, input: &[u8]) -> Vec<u8> {
        let Ok(mut text) = std::str::from_utf8(input) else {
            return input.to_vec();
        };
        let mut out = Vec::with_capacity(
            self.encoder
                .max_buffer_length_from_utf8_without_replacement(text.len())
                .unwrap_or(text.len().saturating_mul(4)),
        );
        loop {
            let (result, read) = self
                .encoder
                .encode_from_utf8_to_vec_without_replacement(text, &mut out, false);
            text = &text[read..];
            match result {
                EncoderResult::InputEmpty => break,
                EncoderResult::OutputFull => out.reserve(text.len().saturating_mul(4) + 16),
                EncoderResult::Unmappable(_) => {
                    out.reserve(1);
                    out.push(b'?');
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opensesh_core::terminal::settings::ENCODINGS;

    #[test]
    fn every_offered_encoding_exists_under_its_canonical_name() {
        for name in ENCODINGS {
            let encoding = Encoding::for_label(name.as_bytes()).unwrap();
            assert_eq!(encoding.name(), *name);
            assert!(encoding.is_ascii_compatible(), "{name}");
        }
        assert!(Codec::new("UTF-8").is_none(), "UTF-8 needs no codec");
        assert!(Codec::new("klingon").is_none());
    }

    #[test]
    fn latin_output_is_decoded_and_escapes_survive() {
        let mut codec = Codec::new("windows-1252").unwrap();
        let mut out = Vec::new();
        codec.decode(b"\x1b[1mcaf\xe9 \x80\x1b[0m", &mut out);
        assert_eq!(String::from_utf8(out).unwrap(), "\x1b[1mcafé €\x1b[0m");
    }

    #[test]
    fn a_character_split_across_reads_is_completed() {
        let mut codec = Codec::new("Shift_JIS").unwrap();
        // "日本" in Shift_JIS: 93 FA 96 7B, split in the middle of the first character.
        let mut out = Vec::new();
        codec.decode(b"\x93", &mut out);
        codec.decode(b"\xfa\x96\x7b", &mut out);
        assert_eq!(String::from_utf8(out).unwrap(), "日本");
    }

    #[test]
    fn typed_text_is_encoded_and_unmappable_characters_become_question_marks() {
        let mut codec = Codec::new("ISO-8859-15").unwrap();
        assert_eq!(codec.encode("é€\r".as_bytes()), b"\xe9\xa4\r");
        assert_eq!(codec.encode("日x".as_bytes()), b"?x");
        assert_eq!(
            codec.encode(b"\x1b[M \xff\xff"),
            b"\x1b[M \xff\xff",
            "raw bytes"
        );
        assert_eq!(codec.name(), "ISO-8859-15");
    }
}
