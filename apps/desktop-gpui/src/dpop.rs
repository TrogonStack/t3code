//! RFC 9449 DPoP proofs with an ES256 (P-256) key, mirroring
//! `packages/client-runtime/src/relay/dpopProof.ts`.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use p256::elliptic_curve::sec1::ToSec1Point as _;
use serde_json::json;
use sha2::{Digest, Sha256};

/// A device proof key. The private scalar is a secret: `Debug` hides it.
pub struct DpopKey {
    signing: SigningKey,
}

impl std::fmt::Debug for DpopKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DpopKey(<redacted>)")
    }
}

impl DpopKey {
    pub fn generate() -> Result<Self, String> {
        loop {
            let mut bytes = [0u8; 32];
            getrandom::fill(&mut bytes)
                .map_err(|error| format!("could not read random bytes: {error}"))?;
            if let Ok(signing) = SigningKey::from_slice(&bytes) {
                return Ok(Self { signing });
            }
        }
    }

    /// The 32-byte private scalar, for Keychain storage.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.signing.to_bytes().into()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        SigningKey::from_slice(bytes)
            .map(|signing| Self { signing })
            .map_err(|_| "the stored DPoP key is unreadable".to_owned())
    }

    /// `(x, y)` as unpadded base64url, the JWK coordinates.
    fn coordinates(&self) -> (String, String) {
        let point = p256::PublicKey::from(self.signing.verifying_key()).to_sec1_point(false);
        let x = point.x().map(|v| B64.encode(v)).unwrap_or_default();
        let y = point.y().map(|v| B64.encode(v)).unwrap_or_default();
        (x, y)
    }

    pub fn public_jwk(&self) -> serde_json::Value {
        let (x, y) = self.coordinates();
        json!({ "kty": "EC", "crv": "P-256", "x": x, "y": y })
    }

    /// RFC 7638 thumbprint: SHA-256 over the required members in lexicographic order.
    pub fn thumbprint(&self) -> String {
        let (x, y) = self.coordinates();
        let canonical = format!(r#"{{"crv":"P-256","kty":"EC","x":"{x}","y":"{y}"}}"#);
        B64.encode(Sha256::digest(canonical.as_bytes()))
    }

    /// A proof for one request. `access_token` adds the `ath` claim.
    pub fn proof(
        &self,
        method: &str,
        url: &str,
        access_token: Option<&str>,
        now_secs: u64,
        jti: &str,
    ) -> Result<String, String> {
        let htu = normalize_htu(url).ok_or("DPoP URL is invalid.")?;
        let header = json!({ "typ": "dpop+jwt", "alg": "ES256", "jwk": self.public_jwk() });
        let mut payload = json!({
            "htm": method.to_uppercase(),
            "htu": htu,
            "jti": jti,
            "iat": now_secs,
        });
        if let Some(token) = access_token.filter(|token| !token.is_empty()) {
            payload["ath"] = json!(B64.encode(Sha256::digest(token.as_bytes())));
        }
        let signing_input = format!(
            "{}.{}",
            B64.encode(header.to_string()),
            B64.encode(payload.to_string())
        );
        let signature: Signature = self.signing.sign(signing_input.as_bytes());
        Ok(format!(
            "{signing_input}.{}",
            B64.encode(signature.to_bytes())
        ))
    }
}

/// `normalizeDpopHtu`: the URL without query or fragment.
pub fn normalize_htu(url: &str) -> Option<String> {
    let mut parsed = url::Url::parse(url).ok()?;
    parsed.set_fragment(None);
    parsed.set_query(None);
    Some(parsed.to_string())
}

pub fn random_jti() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| format!("could not read random bytes: {error}"))?;
    Ok(hex::encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::{VerifyingKey, signature::Verifier};

    fn decode(segment: &str) -> serde_json::Value {
        serde_json::from_slice(&B64.decode(segment).unwrap()).unwrap()
    }

    #[test]
    fn proof_verifies_and_carries_claims() {
        let key = DpopKey::generate().unwrap();
        let proof = key
            .proof(
                "post",
                "https://relay.example/v1/x?a=1#f",
                Some("tok"),
                1_700_000_000,
                "j1",
            )
            .unwrap();
        let parts: Vec<&str> = proof.split('.').collect();
        let header = decode(parts[0]);
        let payload = decode(parts[1]);
        assert_eq!(header["typ"], "dpop+jwt");
        assert_eq!(header["alg"], "ES256");
        assert_eq!(header["jwk"], key.public_jwk());
        assert_eq!(payload["htm"], "POST");
        assert_eq!(payload["htu"], "https://relay.example/v1/x");
        assert_eq!(payload["iat"], 1_700_000_000u64);
        assert_eq!(payload["ath"], B64.encode(Sha256::digest(b"tok")));

        let jwk = &header["jwk"];
        let mut sec1 = vec![4u8];
        sec1.extend(B64.decode(jwk["x"].as_str().unwrap()).unwrap());
        sec1.extend(B64.decode(jwk["y"].as_str().unwrap()).unwrap());
        let verifying = VerifyingKey::from_sec1_bytes(&sec1).unwrap();
        let signature = Signature::from_slice(&B64.decode(parts[2]).unwrap()).unwrap();
        verifying
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
            .unwrap();
    }

    #[test]
    fn no_ath_without_token_and_bad_url_fails() {
        let key = DpopKey::generate().unwrap();
        let proof = key.proof("GET", "https://a.test/p", None, 1, "j").unwrap();
        assert!(
            decode(proof.split('.').nth(1).unwrap())
                .get("ath")
                .is_none()
        );
        assert!(key.proof("GET", "not a url", None, 1, "j").is_err());
    }

    #[test]
    fn thumbprint_is_stable_and_round_trips_through_bytes() {
        let key = DpopKey::generate().unwrap();
        let again = DpopKey::from_bytes(&key.to_bytes()).unwrap();
        assert_eq!(key.thumbprint(), again.thumbprint());
        assert_eq!(key.thumbprint().len(), 43);
    }

    #[test]
    fn rfc7638_thumbprint_canonical_form() {
        // Required members only, lexicographic: crv, kty, x, y.
        let key = DpopKey::generate().unwrap();
        let jwk = key.public_jwk();
        let canonical = format!(
            r#"{{"crv":"P-256","kty":"EC","x":"{}","y":"{}"}}"#,
            jwk["x"].as_str().unwrap(),
            jwk["y"].as_str().unwrap()
        );
        assert_eq!(
            key.thumbprint(),
            B64.encode(Sha256::digest(canonical.as_bytes()))
        );
    }
}
