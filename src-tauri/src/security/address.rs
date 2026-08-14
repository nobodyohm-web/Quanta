//! Public, human-facing Quanta address encoding — **Bech32m** (BIP-350).
//!
//! # Why this module exists
//!
//! On-chain, a Quanta account is a 32-byte address `BLAKE3(ADDR_DOMAIN ‖ ML-DSA
//! pubkey)` (see [`super::CryptoEngine::ml_dsa_address_bytes`]). Internally it is
//! carried as a 64-char **hex** string (the canonical `from`/`to` on the ledger —
//! **unchanged, never touched here**).
//!
//! But every downstream integration surface — wallets, block explorers, and above
//! all **exchange onboarding** (deposit-address generation + `validateaddress`) —
//! expects a compact, human-facing address **with an error-detecting checksum**.
//! A raw 64-hex string has no checksum: a single mistyped character silently points
//! at a different, valid-looking account. This module adds that layer.
//!
//! # Design
//!
//! - **Encoding = Bech32m** (BIP-350), the modern variant that fixes the
//!   length-extension weakness of the original Bech32 (BIP-173). Correct choice for
//!   a brand-new address type.
//! - **HRP = `qta`** → addresses look like `qta1…`, instantly recognizable.
//! - **Bijective over the SAME 32 bytes.** `decode(encode(a)) == a`. This is a pure
//!   *presentation* layer: it changes nothing on the wire, in the ledger, or at
//!   genesis. The hex form remains canonical; [`parse`] accepts **either** form so
//!   boundaries (RPC/UI) are forgiving.
//! - **No new dependency.** Bech32m is ~100 lines and fully specified; implemented
//!   in-house and pinned against the official BIP-350 test vectors (see tests) so
//!   correctness is provable, not assumed.
//!
//! Security posture (matches `.claude/rules/security.md`): no `unwrap`/`expect`, no
//! panics, and every malformed input yields the **opaque** `"Invalid address"` —
//! never the real parse-error type.

/// Bech32 character set (BIP-173/350). Index = 5-bit value.
const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";

/// Human-readable part of a Quanta address → `qta1…`.
pub const HRP: &str = "qta";

/// Bech32m checksum constant (BIP-350). Distinct from Bech32's `1`, which is what
/// makes a Bech32 string fail Bech32m verification and vice-versa.
const BECH32M_CONST: u32 = 0x2bc8_30a3;

/// BCH code generator coefficients (BIP-173).
const GEN: [u32; 5] = [
    0x3b6a_57b2,
    0x2650_8e6d,
    0x1ea1_19fa,
    0x3d42_33dd,
    0x2a14_62b3,
];

/// Opaque error surfaced for every malformed input (security rule §3).
const ERR: &str = "Invalid address";

/// BCH polymod step over GF(2⁵) values.
fn polymod(values: &[u8]) -> u32 {
    let mut chk: u32 = 1;
    for &v in values {
        let top = (chk >> 25) as u8;
        chk = ((chk & 0x01ff_ffff) << 5) ^ (v as u32);
        for (i, g) in GEN.iter().enumerate() {
            if (top >> i) & 1 == 1 {
                chk ^= *g;
            }
        }
    }
    chk
}

/// Expand the HRP into the checksum pre-image (BIP-173): high bits, separator, low bits.
fn hrp_expand(hrp: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(hrp.len() * 2 + 1);
    for &c in hrp {
        out.push(c >> 5);
    }
    out.push(0);
    for &c in hrp {
        out.push(c & 31);
    }
    out
}

/// Verify the Bech32m checksum for `hrp` and `data` (data includes the 6 checksum values).
fn verify_checksum(hrp: &[u8], data: &[u8]) -> bool {
    let mut values = hrp_expand(hrp);
    values.extend_from_slice(data);
    polymod(&values) == BECH32M_CONST
}

/// Compute the 6 Bech32m checksum values for `hrp` + `data`.
fn create_checksum(hrp: &[u8], data: &[u8]) -> Vec<u8> {
    let mut values = hrp_expand(hrp);
    values.extend_from_slice(data);
    values.extend_from_slice(&[0u8; 6]);
    let m = polymod(&values) ^ BECH32M_CONST;
    (0..6)
        .map(|i| ((m >> (5 * (5 - i))) & 31) as u8)
        .collect()
}

/// General base-conversion between bit widths (BIP-173 `convertbits`).
/// Returns `None` on an out-of-range symbol or invalid non-zero padding.
fn convert_bits(data: &[u8], from: u32, to: u32, pad: bool) -> Option<Vec<u8>> {
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let maxv: u32 = (1 << to) - 1;
    let max_acc: u32 = (1 << (from + to - 1)) - 1;
    let mut out = Vec::new();
    for &value in data {
        let v = value as u32;
        if (v >> from) != 0 {
            return None;
        }
        acc = ((acc << from) | v) & max_acc;
        bits += from;
        while bits >= to {
            bits -= to;
            out.push(((acc >> bits) & maxv) as u8);
        }
    }
    if pad {
        if bits > 0 {
            out.push(((acc << (to - bits)) & maxv) as u8);
        }
    } else if bits >= from || ((acc << (to - bits)) & maxv) != 0 {
        return None;
    }
    Some(out)
}

/// Encode `(hrp, data5)` (5-bit groups, no checksum) into a Bech32m string.
fn encode_generic(hrp: &str, data5: &[u8]) -> String {
    let checksum = create_checksum(hrp.as_bytes(), data5);
    let mut s = String::with_capacity(hrp.len() + 1 + data5.len() + 6);
    s.push_str(hrp);
    s.push('1');
    for &d in data5.iter().chain(checksum.iter()) {
        // d is always < 32 (5-bit), so indexing CHARSET is in-bounds.
        s.push(CHARSET[(d & 31) as usize] as char);
    }
    s
}

/// Decode a Bech32m string into `(hrp, data5)` (payload 5-bit groups, checksum stripped).
/// Enforces the BIP rules: length, single case, valid charset, correct checksum.
fn decode_generic(s: &str) -> Result<(String, Vec<u8>), String> {
    if s.len() < 8 || s.len() > 90 {
        return Err(ERR.to_string());
    }
    let has_lower = s.bytes().any(|b| b.is_ascii_lowercase());
    let has_upper = s.bytes().any(|b| b.is_ascii_uppercase());
    if has_lower && has_upper {
        return Err(ERR.to_string());
    }
    if s.bytes().any(|b| !(33..=126).contains(&b)) {
        return Err(ERR.to_string());
    }
    let s_lower = s.to_ascii_lowercase();
    let pos = s_lower.rfind('1').ok_or_else(|| ERR.to_string())?;
    // HRP must be non-empty; data part must hold at least the 6 checksum symbols.
    if pos < 1 || pos + 7 > s_lower.len() {
        return Err(ERR.to_string());
    }
    let hrp = &s_lower[..pos];
    let data_part = &s_lower[pos + 1..];
    let mut data5 = Vec::with_capacity(data_part.len());
    for c in data_part.bytes() {
        match CHARSET.iter().position(|&x| x == c) {
            Some(idx) => data5.push(idx as u8),
            None => return Err(ERR.to_string()),
        }
    }
    if !verify_checksum(hrp.as_bytes(), &data5) {
        return Err(ERR.to_string());
    }
    let payload = data5[..data5.len() - 6].to_vec();
    Ok((hrp.to_string(), payload))
}

/// Encode a 32-byte Quanta address into its canonical `qta1…` Bech32m string.
pub fn encode(addr: &[u8; 32]) -> String {
    // 8→5 regrouping of raw bytes (with padding) can never fail: every input is a
    // byte (< 256) so no symbol is out of range. The fallback keeps this panic-free.
    match convert_bits(addr, 8, 5, true) {
        Some(data5) => encode_generic(HRP, &data5),
        None => String::new(),
    }
}

/// Decode a `qta1…` Bech32m string back to its 32 address bytes.
/// Rejects a wrong HRP, a bad checksum, or a payload that is not exactly 32 bytes.
pub fn decode(s: &str) -> Result<[u8; 32], String> {
    let (hrp, payload5) = decode_generic(s)?;
    if hrp != HRP {
        return Err(ERR.to_string());
    }
    let bytes = convert_bits(&payload5, 5, 8, false).ok_or_else(|| ERR.to_string())?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| ERR.to_string())
}

/// True iff `s` is a well-formed Quanta `qta1…` address (checksum + length valid).
/// This is the function an exchange's `validateaddress` equivalent calls.
pub fn is_valid(s: &str) -> bool {
    decode(s).is_ok()
}

/// Parse an address from **either** form: the `qta1…` Bech32m public form, or the
/// 64-char canonical hex used on the ledger.
///
/// **BAS-1 (AUDIT-2026-08-13) — le repli hexadécimal désarmait le module.**
/// Ce module existe pour une raison : une somme de contrôle, pour qu'une adresse
/// mal recopiée soit **refusée** au lieu d'envoyer des fonds dans le vide. Le
/// repli silencieux vers `hex::decode` la retirait à tout appelant recevant de
/// l'hexadécimal brut — `ledger_transfer` et le RPC, c'est-à-dire précisément les
/// deux chemins de dépense. Une adresse hexadécimale d'un caractère faux reste 64
/// caractères hexadécimaux valides ; elle passait, et les fonds partaient vers une
/// adresse qui n'appartient à personne.
///
/// La tolérance à l'hexadécimal est **conservée** — la chaîne elle-même est en
/// hexadécimal, un opérateur en copie légitimement — mais elle est désormais
/// explicite : `parse_hex_unchecked` le dit dans son nom, et cette fonction-ci
/// reste le décodeur strict que les frontières utilisateur doivent appeler.
pub fn parse(s: &str) -> Result<[u8; 32], String> {
    decode(s)
}

/// Accepte **en plus** l'hexadécimal canonique de 64 caractères, **sans somme de
/// contrôle** (BAS-1).
///
/// À n'appeler que là où l'entrée est déjà d'origine machine ou explicitement
/// assumée : un identifiant lu depuis la chaîne, un outil d'opérateur. Sur une
/// saisie humaine, préférer [`parse`] — c'est la somme de contrôle qui distingue
/// « adresse erronée » de « fonds perdus ».
pub fn parse_hex_unchecked(s: &str) -> Result<[u8; 32], String> {
    if let Ok(addr) = decode(s) {
        return Ok(addr);
    }
    let bytes = hex::decode(s).map_err(|_| ERR.to_string())?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| ERR.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Official BIP-350 (Bech32m) conformance vectors ───────────────────────
    // Pinning against the standard proves our polymod/checksum/charset are correct
    // independently of Quanta's HRP.

    /// Valid Bech32m strings from BIP-350: decode succeeds AND re-encoding the
    /// recovered (hrp, payload) reproduces the (lowercased) input — the checksum
    /// creator and verifier therefore agree.
    #[test]
    fn bip350_valid_vectors_roundtrip() {
        let valid = [
            "A1LQFN3A",
            "a1lqfn3a",
            "abcdef1l7aum6echk45nj3s0wdvt2fg8x9yrzpqzd3ryx",
            "?1v759aa",
            "split1checkupstagehandshakeupstreamerranterredcaperredlc445v",
        ];
        for v in valid {
            let (hrp, payload) = decode_generic(v).expect("BIP-350 vector must decode");
            let re = encode_generic(&hrp, &payload);
            assert_eq!(re, v.to_ascii_lowercase(), "re-encode must match {v}");
        }
    }

    /// Strings that must be REJECTED under Bech32m, including a valid *Bech32*
    /// (non-m) string — the checksum constant must tell them apart.
    #[test]
    fn bip350_invalid_vectors_rejected() {
        let invalid = [
            "A12UEL5L",        // valid Bech32, INVALID Bech32m (wrong constant)
            "A1Lqfn3a",        // mixed case
            "1lqfn3a",         // empty HRP
            "a1lqfn3",         // too short / truncated checksum
            "qta1",            // no data
            "qta1qqqqqqqb",    // corrupted checksum
        ];
        for v in invalid {
            assert!(decode_generic(v).is_err(), "must reject {v}");
        }
    }

    // ── Quanta address round-trip ────────────────────────────────────────────

    #[test]
    fn quanta_address_roundtrips_and_is_wellformed() {
        let cases: [[u8; 32]; 3] = [
            [0u8; 32],
            [0xffu8; 32],
            *blake3::hash(b"quanta-address-test-vector").as_bytes(),
        ];
        for addr in cases {
            let s = encode(&addr);
            assert!(s.starts_with("qta1"), "must be prefixed qta1: {s}");
            // hrp(3) + '1'(1) + 52 data (256 bits / 5, padded) + 6 checksum = 62.
            assert_eq!(s.len(), 62, "fixed length for a 32-byte address: {s}");
            assert!(is_valid(&s), "self-encoded address must validate");
            assert_eq!(decode(&s).expect("must decode"), addr, "bijective round-trip");
        }
    }

    /// A single-character typo must be caught by the checksum (the whole point).
    #[test]
    fn single_char_typo_is_rejected() {
        let addr = *blake3::hash(b"typo-guard").as_bytes();
        let good = encode(&addr);
        let mut chars: Vec<char> = good.chars().collect();
        // Flip the last data character to a different valid charset symbol.
        let last = chars.len() - 1;
        chars[last] = if chars[last] == 'q' { 'p' } else { 'q' };
        let bad: String = chars.into_iter().collect();
        assert_ne!(bad, good);
        assert!(!is_valid(&bad), "checksum must catch a 1-char typo");
    }

    /// A well-formed Bech32m string under a DIFFERENT hrp is not a Quanta address.
    #[test]
    fn wrong_hrp_is_rejected() {
        let addr = [7u8; 32];
        let data5 = convert_bits(&addr, 8, 5, true).expect("convert");
        let foreign = encode_generic("btc", &data5);
        assert!(decode_generic(&foreign).is_ok(), "valid bech32m, just wrong hrp");
        assert!(decode(&foreign).is_err(), "decode must require hrp=qta");
        assert!(!is_valid(&foreign));
    }

    /// **BAS-1 (AUDIT-2026-08-13)** — `parse` est un décodeur Bech32m STRICT ;
    /// la tolérance à l'hexadécimal a un nom, et il dit ce qu'elle coûte.
    ///
    /// Le repli silencieux vers `hex::decode` retirait la somme de contrôle à
    /// tout appelant recevant de l'hexadécimal brut — c'est-à-dire aux deux
    /// chemins de dépense. Une adresse hexadécimale d'un caractère faux reste 64
    /// caractères hexadécimaux valides : elle passait, et les fonds partaient
    /// vers une adresse qui n'appartient à personne.
    #[test]
    fn bas1_parse_is_strict_and_the_hex_escape_hatch_is_named() {
        let addr = *blake3::hash(b"dual-form").as_bytes();
        let bech = encode(&addr);
        let hexs = hex::encode(addr);

        assert_eq!(parse(&bech).expect("bech32m"), addr);
        assert!(
            parse(&hexs).is_err(),
            "l'hexadécimal nu n'a pas de somme de contrôle : `parse` le refuse"
        );
        assert_eq!(parse_hex_unchecked(&hexs).expect("hex"), addr);
        assert_eq!(parse_hex_unchecked(&bech).expect("bech32m"), addr);
        assert!(parse("not-an-address").is_err());
        assert!(parse_hex_unchecked("not-an-address").is_err());

        // La propriété qui compte : UNE faute de frappe dans la forme publique est
        // refusée, la même faute dans la forme hexadécimale ne l'est pas — c'est
        // exactement pourquoi les deux fonctions ne portent plus le même nom.
        let mut typo: Vec<char> = bech.chars().collect();
        let last = typo.len() - 1;
        typo[last] = if typo[last] == 'q' { 'p' } else { 'q' };
        let typo: String = typo.into_iter().collect();
        assert!(parse(&typo).is_err(), "somme de contrôle Bech32m : la faute est vue");

        let mut hex_typo: Vec<char> = hexs.chars().collect();
        hex_typo[0] = if hex_typo[0] == 'a' { 'b' } else { 'a' };
        let hex_typo: String = hex_typo.into_iter().collect();
        assert!(
            parse_hex_unchecked(&hex_typo).is_ok(),
            "l'hexadécimal ne peut PAS voir la faute — la fonction le dit dans son nom"
        );
    }

    /// The public encoding is a pure view over the SAME bytes the engine derives —
    /// it must agree with `ml_dsa_address_bytes` and stay reversible.
    #[test]
    fn matches_engine_address_derivation() {
        let pk = b"a-fake-ml-dsa-public-key-blob-for-derivation-only";
        let addr = crate::security::CryptoEngine::ml_dsa_address_bytes(pk);
        let bech = encode(&addr);
        assert_eq!(decode(&bech).expect("decode"), addr);
        // And the hex the ledger uses decodes to the very same bytes.
        assert_eq!(
            parse_hex_unchecked(&crate::security::CryptoEngine::ml_dsa_address_hex(pk)).expect("hex"),
            addr
        );
    }
}
