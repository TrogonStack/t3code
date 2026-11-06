// TODO(gpui-credentials): HACK. Reads Electron's private IndexedDB and non-extractable DPoP key. Replace with GPUI's own pairing + Keychain before shipping.
//
//! The Electron app's DPoP proof key, and RFC 9449 proofs signed with it.
//!
//! apps/web/src/cloud/dpop.ts keeps the key as `{privateKey: CryptoKey, publicJwk, thumbprint}` in
//! IndexedDB (`t3code:cloud-auth` / `keys` / `relay-dpop-proof-key`). Chromium stores that in a
//! LevelDB directory as a Blink structured clone, where an EC CryptoKey is the `K` tag followed by
//! its algorithm, curve, usages and PKCS8 bytes. This module copies the directory to a temp dir,
//! scans the table and log files for the record (no LevelDB crate: the `idb_cmp1` comparator is
//! irrelevant to a scan), and decodes the key. Claims mirror
//! packages/client-runtime/src/relay/dpopProof.ts and packages/shared/src/dpopCommon.ts.
//! Nothing here logs or returns key material beyond the `SigningKey` itself.

use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use p256::pkcs8::DecodePrivateKey;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Where Electron's dev app keeps IndexedDB for the `t3code-dev` app.
pub const ENV_IDB_DIR: &str = "T3CODE_GPUI_ELECTRON_IDB";
const KEY_ID: &str = "relay-dpop-proof-key";

pub fn default_idb_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(ENV_IDB_DIR).filter(|dir| !dir.is_empty()) {
        return Some(dir.into());
    }
    let home = std::env::var_os("HOME")?;
    Some(
        Path::new(&home)
            .join("Library/Application Support/t3code-dev/IndexedDB")
            .join("t3code-dev_app_0.indexeddb.leveldb"),
    )
}

// ---------------------------------------------------------------------------------------------
// base64url (RFC 4648 section 5, no padding)

pub fn b64url_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Proof signing

/// The key's public JWK members, in RFC 7638 order.
fn public_jwk(key: &SigningKey) -> serde_json::Value {
    let point = key.verifying_key().to_sec1_point(false);
    let (x, y) = (
        point.x().expect("uncompressed point has x"),
        point.y().expect("uncompressed point has y"),
    );
    json!({"crv": "P-256", "kty": "EC", "x": b64url_encode(x), "y": b64url_encode(y)})
}

/// RFC 7638 thumbprint, as stored in `RemoteDpopAccessToken.dpopThumbprint`.
pub fn thumbprint(key: &SigningKey) -> String {
    // serde_json's map is sorted, which is the lexicographic order the RFC asks for.
    b64url_encode(&Sha256::digest(public_jwk(key).to_string().as_bytes()))
}

/// `normalizeDpopHtu`: the URL without query or fragment.
pub fn normalize_htu(url: &str) -> Result<String, String> {
    let mut parsed = reqwest::Url::parse(url).map_err(|_| "DPoP URL is invalid.".to_owned())?;
    parsed.set_fragment(None);
    parsed.set_query(None);
    Ok(parsed.to_string())
}

fn random_uuid_v4() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| format!("could not read random bytes: {error}"))?;
    bytes[6] = bytes[6] & 0x0f | 0x40;
    bytes[8] = bytes[8] & 0x3f | 0x80;
    let hex = hex::encode(bytes);
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

/// `createProof`: an ES256 JWS with `typ: dpop+jwt` and the public JWK, bound to the request
/// method and URL, and to `access_token` through `ath` when there is one.
pub fn create_proof(
    key: &SigningKey,
    method: &str,
    url: &str,
    access_token: Option<&str>,
    now_ms: u64,
) -> Result<String, String> {
    let header = json!({"typ": "dpop+jwt", "alg": "ES256", "jwk": public_jwk(key)});
    let mut claims = json!({
        "htm": method.to_uppercase(),
        "htu": normalize_htu(url)?,
        "jti": random_uuid_v4()?,
        "iat": now_ms / 1_000,
    });
    if let Some(token) = access_token.filter(|token| !token.is_empty()) {
        claims["ath"] = json!(b64url_encode(&Sha256::digest(token.as_bytes())));
    }
    let signing_input = format!(
        "{}.{}",
        b64url_encode(header.to_string().as_bytes()),
        b64url_encode(claims.to_string().as_bytes())
    );
    let signature: Signature = key.sign(signing_input.as_bytes());
    Ok(format!(
        "{signing_input}.{}",
        b64url_encode(&signature.to_bytes())
    ))
}

// ---------------------------------------------------------------------------------------------
// Blink CryptoKey

/// V8 `ValueSerializer` varint (base-128, little endian).
fn read_varint(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let (mut value, mut shift) = (0u64, 0u32);
    loop {
        let byte = *bytes.get(*at)?;
        *at += 1;
        if shift >= 64 {
            return None;
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
    }
}

const CRYPTO_KEY_TAG: u8 = b'K';
const EC_KEY_TAG: u8 = 5;
const PRIVATE_KEY_TYPE: u64 = 2;
const P256_CURVE_TAG: u64 = 1;

/// Decodes the EC private `CryptoKey` that starts at `bytes[0]` (`V8ScriptValueSerializerForModules`
/// layout: `K`, EC sub-tag, algorithm, curve, key type, usages, length, PKCS8 bytes).
fn decode_crypto_key_at(bytes: &[u8]) -> Option<SigningKey> {
    let mut at = 0;
    if bytes.get(at)? != &CRYPTO_KEY_TAG || bytes.get(at + 1)? != &EC_KEY_TAG {
        return None;
    }
    at += 2;
    let _algorithm = read_varint(bytes, &mut at)?;
    if read_varint(bytes, &mut at)? != P256_CURVE_TAG {
        return None;
    }
    if read_varint(bytes, &mut at)? != PRIVATE_KEY_TYPE {
        return None;
    }
    let _usages = read_varint(bytes, &mut at)?;
    let length = usize::try_from(read_varint(bytes, &mut at)?).ok()?;
    let pkcs8 = bytes.get(at..at.checked_add(length)?)?;
    SigningKey::from_pkcs8_der(pkcs8).ok()
}

/// Finds the stored private CryptoKey anywhere in an IndexedDB record value. The surrounding
/// object (`publicJwk`, `thumbprint`) is skipped rather than parsed: the key's tag bytes are
/// validated by their PKCS8 payload, so a false hit cannot decode.
pub fn decode_crypto_key(value: &[u8]) -> Option<SigningKey> {
    (0..value.len())
        .filter(|&at| value[at] == CRYPTO_KEY_TAG)
        .find_map(|at| decode_crypto_key_at(&value[at..]))
}

/// A record value may be wrapped in Chromium's snappy envelope (tag 0x11 after the version).
fn key_from_record(value: &[u8]) -> Option<SigningKey> {
    decode_crypto_key(value).or_else(|| {
        (0..value.len().min(16))
            .filter(|&at| value[at] == 0x11)
            .find_map(|at| {
                let inflated = snap::raw::Decoder::new()
                    .decompress_vec(&value[at + 1..])
                    .ok()?;
                decode_crypto_key(&inflated)
            })
    })
}

// ---------------------------------------------------------------------------------------------
// LevelDB scan

struct Record {
    sequence: u64,
    deleted: bool,
    value: Vec<u8>,
}

/// IndexedDB's key for `relay-dpop-proof-key` in an object store: a prefix (database, store and
/// index ids; index 1 is the object store data) and the string key (`0x01`, length, UTF-16BE).
fn is_key_record(user_key: &[u8]) -> bool {
    let Some(&flags) = user_key.first() else {
        return false;
    };
    let db_bytes = usize::from(flags >> 5 & 7) + 1;
    let store_bytes = usize::from(flags >> 2 & 7) + 1;
    let index_bytes = usize::from(flags & 3) + 1;
    let prefix = 1 + db_bytes + store_bytes + index_bytes;
    let Some(index) = user_key.get(prefix - index_bytes..prefix) else {
        return false;
    };
    let index_id = index
        .iter()
        .enumerate()
        .fold(0u64, |id, (i, byte)| id | u64::from(*byte) << (8 * i));
    let mut expected = vec![0x01, KEY_ID.len() as u8];
    expected.extend(KEY_ID.encode_utf16().flat_map(u16::to_be_bytes));
    index_id == 1 && user_key.get(prefix..) == Some(expected.as_slice())
}

fn read_u32_le(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn read_slice<'a>(bytes: &'a [u8], at: &mut usize) -> Option<&'a [u8]> {
    let length = usize::try_from(read_varint(bytes, at)?).ok()?;
    let slice = bytes.get(*at..at.checked_add(length)?)?;
    *at += length;
    Some(slice)
}

fn keep_newest(found: &mut HashMap<Vec<u8>, Record>, user_key: &[u8], record: Record) {
    if !is_key_record(user_key) {
        return;
    }
    match found.get(user_key) {
        Some(existing) if existing.sequence >= record.sequence => {}
        _ => {
            found.insert(user_key.to_vec(), record);
        }
    }
}

/// `.log` files: 32 KiB blocks of `crc(4) length(2) type(1)` fragments that reassemble into
/// WriteBatches (`sequence(8) count(4)` then `type key [value]` entries).
fn scan_log(data: &[u8], found: &mut HashMap<Vec<u8>, Record>) {
    const BLOCK: usize = 32 * 1024;
    let mut batch = Vec::new();
    let mut batches = Vec::new();
    for block in data.chunks(BLOCK) {
        let mut at = 0;
        while at + 7 <= block.len() {
            let length = usize::from(u16::from_le_bytes([block[at + 4], block[at + 5]]));
            let kind = block[at + 6];
            let Some(payload) = block.get(at + 7..at + 7 + length) else {
                break;
            };
            at += 7 + length;
            match kind {
                1 => batches.push(payload.to_vec()),
                2 => batch = payload.to_vec(),
                3 => batch.extend_from_slice(payload),
                4 => {
                    batch.extend_from_slice(payload);
                    batches.push(std::mem::take(&mut batch));
                }
                _ => {}
            }
        }
    }
    for batch in batches {
        let Some(sequence) = batch
            .get(..8)
            .map(|bytes| u64::from_le_bytes(bytes.try_into().expect("8 bytes")))
        else {
            continue;
        };
        let Some(count) = read_u32_le(&batch, 8) else {
            continue;
        };
        let mut at = 12;
        for index in 0..u64::from(count) {
            let Some(&kind) = batch.get(at) else { break };
            at += 1;
            let Some(key) = read_slice(&batch, &mut at) else {
                break;
            };
            let value = if kind == 1 {
                match read_slice(&batch, &mut at) {
                    Some(value) => value,
                    None => break,
                }
            } else {
                &[]
            };
            keep_newest(
                found,
                key,
                Record {
                    sequence: sequence + index,
                    deleted: kind == 0,
                    value: value.to_vec(),
                },
            );
        }
    }
}

const TABLE_MAGIC: [u8; 8] = [0x57, 0xfb, 0x80, 0x8b, 0x24, 0x75, 0x47, 0xdb];

fn read_handle(bytes: &[u8], at: &mut usize) -> Option<(usize, usize)> {
    let offset = usize::try_from(read_varint(bytes, at)?).ok()?;
    let size = usize::try_from(read_varint(bytes, at)?).ok()?;
    Some((offset, size))
}

/// A table block's contents, snappy-inflated when its trailer says so.
fn read_block(data: &[u8], (offset, size): (usize, usize)) -> Option<Vec<u8>> {
    let contents = data.get(offset..offset.checked_add(size)?)?;
    match data.get(offset + size)? {
        0 => Some(contents.to_vec()),
        1 => snap::raw::Decoder::new().decompress_vec(contents).ok(),
        _ => None,
    }
}

/// The `(key, value)` entries of a block (prefix-compressed, restart array at the end).
fn block_entries(block: &[u8]) -> Vec<(Vec<u8>, &[u8])> {
    let mut entries = Vec::new();
    let Some(restarts) = read_u32_le(block, block.len().saturating_sub(4)) else {
        return entries;
    };
    let Some(end) = block.len().checked_sub(4 + 4 * restarts as usize) else {
        return entries;
    };
    let (mut at, mut key) = (0, Vec::new());
    while at < end {
        let (Some(shared), Some(unshared), Some(value_length)) = (
            read_varint(block, &mut at),
            read_varint(block, &mut at),
            read_varint(block, &mut at),
        ) else {
            break;
        };
        let (shared, unshared, value_length) =
            (shared as usize, unshared as usize, value_length as usize);
        let (Some(delta), Some(value)) = (
            block.get(at..at.saturating_add(unshared)),
            block.get(at + unshared..(at + unshared).saturating_add(value_length)),
        ) else {
            break;
        };
        if shared > key.len() {
            break;
        }
        key.truncate(shared);
        key.extend_from_slice(delta);
        at += unshared + value_length;
        entries.push((key.clone(), value));
    }
    entries
}

/// `.ldb` files: footer, index block of data block handles, data blocks of internal keys
/// (`user key` + `sequence << 8 | type`).
fn scan_table(data: &[u8], found: &mut HashMap<Vec<u8>, Record>) {
    let Some(footer) = data.len().checked_sub(48).map(|start| &data[start..]) else {
        return;
    };
    if footer[40..] != TABLE_MAGIC {
        return;
    }
    let mut at = 0;
    if read_handle(footer, &mut at).is_none() {
        return;
    }
    let Some(index_handle) = read_handle(footer, &mut at) else {
        return;
    };
    let Some(index) = read_block(data, index_handle) else {
        return;
    };
    for (_, handle) in block_entries(&index) {
        let mut at = 0;
        let Some(block) = read_handle(handle, &mut at).and_then(|h| read_block(data, h)) else {
            continue;
        };
        for (internal_key, value) in block_entries(&block) {
            let Some(split) = internal_key.len().checked_sub(8) else {
                continue;
            };
            let trailer = u64::from_le_bytes(internal_key[split..].try_into().expect("8 bytes"));
            keep_newest(
                found,
                &internal_key[..split],
                Record {
                    sequence: trailer >> 8,
                    deleted: trailer & 0xff == 0,
                    value: value.to_vec(),
                },
            );
        }
    }
}

/// Removes the temp copy on every exit path.
struct TempCopy(PathBuf);

impl Drop for TempCopy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scan_directory(dir: &Path) -> Result<Vec<Record>, String> {
    let mut found = HashMap::new();
    let entries = std::fs::read_dir(dir).map_err(|error| error.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(data) = std::fs::read(&path) else {
            continue;
        };
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("ldb") | Some("sst") => scan_table(&data, &mut found),
            Some("log") => scan_log(&data, &mut found),
            _ => {}
        }
    }
    Ok(found.into_values().filter(|r| !r.deleted).collect())
}

/// Reads the Electron app's DPoP proof key from its IndexedDB directory. Only a copy of the
/// `.ldb` and `.log` files is read, and the copy is deleted before this returns.
pub fn load_key(idb_dir: &Path) -> Result<SigningKey, String> {
    let mut suffix = [0u8; 8];
    getrandom::fill(&mut suffix).map_err(|error| error.to_string())?;
    let copy = TempCopy(std::env::temp_dir().join(format!("t3-gpui-idb-{}", hex::encode(suffix))));
    std::fs::create_dir(&copy.0).map_err(|error| error.to_string())?;
    let entries = std::fs::read_dir(idb_dir)
        .map_err(|error| format!("could not read the Electron IndexedDB: {error}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("ldb") | Some("sst") | Some("log")
        ) && let Some(name) = path.file_name()
        {
            std::fs::copy(&path, copy.0.join(name)).map_err(|error| error.to_string())?;
        }
    }
    let mut records = scan_directory(&copy.0)?;
    records.sort_by_key(|record| std::cmp::Reverse(record.sequence));
    records
        .iter()
        .find_map(|record| key_from_record(&record.value))
        .ok_or_else(|| "no DPoP proof key in the Electron IndexedDB".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::pkcs8::EncodePrivateKey;

    fn varint(mut value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                return out;
            }
            out.push(byte | 0x80);
        }
    }

    fn fresh_key() -> SigningKey {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).unwrap();
        SigningKey::from_slice(&seed).unwrap()
    }

    /// What Blink writes for an extractable-false ECDSA P-256 private key with the `sign` usage.
    fn serialized_crypto_key(key: &SigningKey) -> Vec<u8> {
        let pkcs8 = key.to_pkcs8_der().unwrap();
        let mut out = vec![CRYPTO_KEY_TAG, EC_KEY_TAG];
        out.extend(varint(13)); // ECDSA
        out.extend(varint(P256_CURVE_TAG));
        out.extend(varint(PRIVATE_KEY_TYPE));
        out.extend(varint(1 << 3)); // sign
        out.extend(varint(pkcs8.as_bytes().len() as u64));
        out.extend_from_slice(pkcs8.as_bytes());
        out
    }

    /// An IndexedDB value: version, wrapper bytes, then an object whose first property is the key.
    fn record_value(key: &SigningKey) -> Vec<u8> {
        let mut value = vec![0x00, 0xff, 0x11, 0xff, 0x0f, b'o', b'"', 10];
        value.extend_from_slice(b"privateKey");
        value.extend(serialized_crypto_key(key));
        value.extend_from_slice(b"\"\x09publicJwk{");
        value
    }

    #[test]
    fn decodes_a_blink_serialized_key_inside_a_record() {
        let key = fresh_key();
        let decoded = decode_crypto_key(&record_value(&key)).expect("key");
        assert_eq!(thumbprint(&decoded), thumbprint(&key));
        assert!(decode_crypto_key(b"K\x05\x0d\x01\x02").is_none());
        assert!(decode_crypto_key(&[]).is_none());
    }

    #[test]
    fn rejects_public_and_other_curve_keys() {
        let mut other_curve = serialized_crypto_key(&fresh_key());
        other_curve[3] = 2; // P-384
        assert!(decode_crypto_key(&other_curve).is_none());
        let mut public = serialized_crypto_key(&fresh_key());
        public[4] = 1;
        assert!(decode_crypto_key(&public).is_none());
    }

    #[test]
    fn decodes_a_snappy_wrapped_record() {
        let key = fresh_key();
        let mut value = vec![0x00, 0x11];
        value.extend(
            snap::raw::Encoder::new()
                .compress_vec(&record_value(&key))
                .unwrap(),
        );
        // Without the envelope the compressed bytes hold no key, so the wrapper path is the one used.
        let decoded = key_from_record(&value).expect("key");
        assert_eq!(thumbprint(&decoded), thumbprint(&key));
    }

    #[test]
    fn recognises_the_idb_object_store_key() {
        let mut key = vec![0x00, 0x01, 0x02, 0x01]; // 1-byte ids, db 1, store 2, index 1
        key.extend([0x01, KEY_ID.len() as u8]);
        key.extend(KEY_ID.encode_utf16().flat_map(u16::to_be_bytes));
        assert!(is_key_record(&key));
        let mut other_index = key.clone();
        other_index[3] = 2;
        assert!(!is_key_record(&other_index));
        assert!(!is_key_record(&key[..key.len() - 2]));
        assert!(!is_key_record(&[]));
    }

    #[test]
    fn scans_a_leveldb_log_and_keeps_the_newest_value() {
        let key = fresh_key();
        let mut user_key = vec![0x00, 0x01, 0x02, 0x01, 0x01, KEY_ID.len() as u8];
        user_key.extend(KEY_ID.encode_utf16().flat_map(u16::to_be_bytes));
        let batch = |sequence: u64, value: &[u8]| {
            let mut batch = sequence.to_le_bytes().to_vec();
            batch.extend(1u32.to_le_bytes());
            batch.push(1);
            batch.extend(varint(user_key.len() as u64));
            batch.extend(&user_key);
            batch.extend(varint(value.len() as u64));
            batch.extend(value);
            batch
        };
        let mut log = Vec::new();
        for (sequence, value) in [(5, b"stale".to_vec()), (9, record_value(&key))] {
            let payload = batch(sequence, &value);
            log.extend([0u8; 4]);
            log.extend((payload.len() as u16).to_le_bytes());
            log.push(1);
            log.extend(payload);
        }
        let mut found = HashMap::new();
        scan_log(&log, &mut found);
        let record = found.get(&user_key).expect("record");
        assert_eq!(record.sequence, 9);
        let decoded = key_from_record(&record.value).expect("key");
        assert_eq!(thumbprint(&decoded), thumbprint(&key));
    }

    #[test]
    fn scans_a_table_block_with_prefix_compression() {
        let mut user_key = vec![0x00, 0x01, 0x02, 0x01, 0x01, KEY_ID.len() as u8];
        user_key.extend(KEY_ID.encode_utf16().flat_map(u16::to_be_bytes));
        let mut internal = user_key.clone();
        internal.extend(((7u64 << 8) | 1).to_le_bytes());
        let key = fresh_key();
        let value = record_value(&key);
        // Two entries: a full key, then the same key sharing all but its trailer.
        let mut block = Vec::new();
        block.extend(varint(0));
        block.extend(varint(internal.len() as u64));
        block.extend(varint(value.len() as u64));
        block.extend(&internal);
        block.extend(&value);
        let mut second = user_key.clone();
        second.extend(((8u64 << 8) | 1).to_le_bytes());
        block.extend(varint(user_key.len() as u64));
        block.extend(varint(8));
        block.extend(varint(1));
        block.extend(&second[user_key.len()..]);
        block.push(0);
        block.extend(0u32.to_le_bytes());
        block.extend(1u32.to_le_bytes());
        let entries = block_entries(&block);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].0, second);

        let mut table = Vec::new();
        let data_handle = (table.len(), block.len());
        table.extend(&block);
        table.extend([0, 0, 0, 0, 0]);
        let mut index = Vec::new();
        let mut handle = varint(data_handle.0 as u64);
        handle.extend(varint(data_handle.1 as u64));
        index.extend(varint(0));
        index.extend(varint(second.len() as u64));
        index.extend(varint(handle.len() as u64));
        index.extend(&second);
        index.extend(&handle);
        index.extend(0u32.to_le_bytes());
        index.extend(1u32.to_le_bytes());
        let index_handle = (table.len(), index.len());
        table.extend(&index);
        table.extend([0, 0, 0, 0, 0]);
        let mut footer = varint(0);
        footer.extend(varint(0));
        footer.extend(varint(index_handle.0 as u64));
        footer.extend(varint(index_handle.1 as u64));
        footer.resize(40, 0);
        footer.extend(TABLE_MAGIC);
        table.extend(footer);

        let mut found = HashMap::new();
        scan_table(&table, &mut found);
        let record = found.get(&user_key).expect("record");
        assert_eq!(record.sequence, 8);
        assert!(!record.deleted);
    }

    #[test]
    fn proof_has_the_client_runtime_shape_and_verifies() {
        use p256::ecdsa::{VerifyingKey, signature::Verifier};
        let key = fresh_key();
        let proof = create_proof(
            &key,
            "post",
            "https://Host.example:443/api/auth/websocket-ticket/rpc?x=1#frag",
            Some("access"),
            1_700_000_123_456,
        )
        .unwrap();
        let parts: Vec<&str> = proof.split('.').collect();
        assert_eq!(parts.len(), 3);
        let decode = |part: &str| {
            let mut padded = part.replace('-', "+").replace('_', "/");
            while !padded.len().is_multiple_of(4) {
                padded.push('=');
            }
            crate::catalog::b64::decode(&padded).unwrap()
        };
        let header: serde_json::Value = serde_json::from_slice(&decode(parts[0])).unwrap();
        let claims: serde_json::Value = serde_json::from_slice(&decode(parts[1])).unwrap();
        assert_eq!(header["typ"], "dpop+jwt");
        assert_eq!(header["alg"], "ES256");
        assert_eq!(header["jwk"], public_jwk(&key));
        assert!(header["jwk"].get("d").is_none());
        assert_eq!(claims["htm"], "POST");
        assert_eq!(
            claims["htu"],
            "https://host.example/api/auth/websocket-ticket/rpc"
        );
        assert_eq!(claims["iat"], 1_700_000_123);
        assert_eq!(claims["jti"].as_str().unwrap().len(), 36);
        // SHA-256("access"), base64url.
        assert_eq!(
            claims["ath"],
            "oFYf1knNtrqnhAVfBRuteW6gr-8X_KOCGVSd7rpOjBo".to_owned()
        );
        let signature = Signature::from_slice(&decode(parts[2])).unwrap();
        VerifyingKey::from(&key)
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
            .unwrap();
        let no_ath = create_proof(&key, "GET", "http://h/ws", None, 0).unwrap();
        assert!(!no_ath.contains("ath"));
        assert!(create_proof(&key, "GET", "not a url", None, 0).is_err());
    }

    #[test]
    fn thumbprint_matches_rfc_7638_over_the_public_jwk() {
        let key = fresh_key();
        let jwk = public_jwk(&key);
        let canonical = format!(
            r#"{{"crv":"P-256","kty":"EC","x":"{}","y":"{}"}}"#,
            jwk["x"].as_str().unwrap(),
            jwk["y"].as_str().unwrap()
        );
        assert_eq!(
            thumbprint(&key),
            b64url_encode(&Sha256::digest(canonical.as_bytes()))
        );
    }

    #[test]
    fn base64url_matches_known_vectors() {
        assert_eq!(b64url_encode(b""), "");
        assert_eq!(b64url_encode(b"f"), "Zg");
        assert_eq!(b64url_encode(b"fo"), "Zm8");
        assert_eq!(b64url_encode(&[0xfb, 0xff]), "-_8");
    }
}
