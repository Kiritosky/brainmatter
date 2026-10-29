//! Printable recovery key: 160 random bits plus a 16-bit checksum, encoded as
//! Crockford Base32 in groups of four (e.g. `7K3Q-…`), 36 characters total.

use std::fmt;

use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::crypto::{SecretKey, random_fill};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const SECRET_LEN: usize = 20;
const ENCODED_LEN: usize = 36;

pub struct RecoveryKey(Zeroizing<[u8; SECRET_LEN]>);

impl RecoveryKey {
    pub(crate) fn generate() -> Self {
        let mut bytes = Zeroizing::new([0u8; SECRET_LEN]);
        random_fill(bytes.as_mut());
        Self(bytes)
    }

    /// Parses user input; tolerant of case, dashes, spaces and the Crockford
    /// look-alikes (I/L → 1, O → 0). Returns `None` on a typo (bad checksum).
    pub fn parse(input: &str) -> Option<Self> {
        let mut bits: u64 = 0;
        let mut nbits = 0;
        let mut out = Zeroizing::new(Vec::with_capacity(SECRET_LEN + 2));
        let mut chars = 0;
        for c in input.chars().filter(|c| !matches!(c, '-' | ' ')) {
            let c = match c.to_ascii_uppercase() {
                'I' | 'L' => '1',
                'O' => '0',
                c => c,
            };
            let v = ALPHABET.iter().position(|&a| a as char == c)? as u64;
            bits = (bits << 5) | v;
            nbits += 5;
            chars += 1;
            if nbits >= 8 {
                nbits -= 8;
                out.push((bits >> nbits) as u8);
                bits &= (1 << nbits) - 1;
            }
        }
        if chars != ENCODED_LEN || out.len() != SECRET_LEN + 2 {
            return None;
        }
        let secret: [u8; SECRET_LEN] = out[..SECRET_LEN].try_into().ok()?;
        (checksum(&secret) == out[SECRET_LEN..]).then(|| Self(Zeroizing::new(secret)))
    }

    /// Key-encryption key for the recovery slot.
    pub(crate) fn kek(&self) -> SecretKey {
        let mut padded = Zeroizing::new([0u8; 32]);
        padded[..SECRET_LEN].copy_from_slice(self.0.as_ref());
        SecretKey::from_bytes(*padded).derive("brainmatter/recovery-kek/v1")
    }
}

fn checksum(secret: &[u8]) -> [u8; 2] {
    let h = Sha256::digest(secret);
    [h[0], h[1]]
}

impl fmt::Display for RecoveryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut data = Zeroizing::new(self.0.to_vec());
        data.extend_from_slice(&checksum(self.0.as_ref()));
        let mut bits: u64 = 0;
        let mut nbits = 0;
        let mut written = 0;
        let mut emit = |f: &mut fmt::Formatter<'_>, v: u64| -> fmt::Result {
            if written > 0 && written % 4 == 0 {
                f.write_str("-")?;
            }
            written += 1;
            write!(f, "{}", ALPHABET[v as usize] as char)
        };
        for &b in data.iter() {
            bits = (bits << 8) | b as u64;
            nbits += 8;
            while nbits >= 5 {
                nbits -= 5;
                emit(f, (bits >> nbits) & 31)?;
            }
            bits &= (1 << nbits) - 1;
        }
        if nbits > 0 {
            emit(f, (bits << (5 - nbits)) & 31)?;
        }
        Ok(())
    }
}

impl fmt::Debug for RecoveryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoveryKey(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_typo_detection() {
        let key = RecoveryKey::generate();
        let text = key.to_string();
        assert_eq!(text.replace('-', "").len(), ENCODED_LEN);
        assert_eq!(text.split('-').count(), 9);

        let parsed = RecoveryKey::parse(&text.to_lowercase().replace('-', " ")).unwrap();
        assert_eq!(parsed.0.as_ref(), key.0.as_ref());

        let mut typo: Vec<char> = text.chars().collect();
        typo[0] = if typo[0] == 'A' { 'B' } else { 'A' };
        assert!(RecoveryKey::parse(&typo.into_iter().collect::<String>()).is_none());
        assert!(RecoveryKey::parse("too-short").is_none());
    }
}
