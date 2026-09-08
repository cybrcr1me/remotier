//! Signing with a FIDO security key, in this process.
//!
//! An `sk-` key file holds a credential handle and a relying-party name, never a secret,
//! so nothing here can sign with it the ordinary way. The token can, over CTAP2.
//!
//! Going through the ssh-agent is the usual answer and a poor one: macOS hands every GUI
//! app launchd's agent, whose OpenSSH build ships no `ssh-sk-helper` and therefore fails
//! every signature for such a key; Windows has no workable way to feed it a PIN. Talking
//! to the token directly removes the agent, `ssh-add`, and the platform differences with
//! it - and lets the touch and PIN prompts be part of the app.
//!
//! The signature format is OpenSSH's, from PROTOCOL.u2f: the token is asked to sign the
//! SHA-256 of the data SSH wants signed, and the result is wrapped with the authenticator
//! flags and signature counter the verifier needs to reconstruct what was signed.

use russh::keys::ssh_key::private::KeypairData;
use russh::keys::ssh_key::{Algorithm, PrivateKey};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::error::{Error, Result};

/// How long to wait for the user to touch the token before giving up on them.
const TOUCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

/// What an `sk-` key file actually contains.
#[derive(Debug, Clone)]
pub struct SecurityKey {
    pub algorithm: Algorithm,
    /// The relying party, `ssh:` by default, or whatever `-O application=` set.
    pub application: String,
    /// The credential handle. Signing is asking the token to use this one.
    pub key_handle: Vec<u8>,
    /// Authenticator flags recorded at enrolment; bit 2 means a PIN is required.
    pub flags: u8,
}

/*
 * Flag bits OpenSSH records in an `sk-` key file, from its `sk-api.h`. Only verification
 * changes what has to be asked of the user before signing.
 */
const USER_PRESENCE_REQUIRED: u8 = 0x01;
/// Set when the credential was made with `-O verify-required`.
const USER_VERIFICATION_REQUIRED: u8 = 0x04;
const RESIDENT_KEY: u8 = 0x20;

impl SecurityKey {
    /// True when the credential was enrolled with user verification required.
    ///
    /// Worth checking before going near the token. An authenticator does not treat such a
    /// credential as merely locked - it hides it from any assertion that is not doing
    /// verification, and answers `CTAP2_ERR_NO_CREDENTIALS`, which reads as "wrong key"
    /// rather than "needs a PIN". Asking first turns that into the right prompt, and saves
    /// a touch that could never have succeeded.
    ///
    /// Still only a lower bound: a token can demand verification the file said nothing
    /// about, so a refusal has to be handled as well.
    pub fn needs_pin(&self) -> bool {
        self.flags & USER_VERIFICATION_REQUIRED != 0
    }

    /// True when the credential lives on the token rather than only in the file.
    pub fn is_resident(&self) -> bool {
        self.flags & RESIDENT_KEY != 0
    }

    /// True when the token must be touched.
    pub fn needs_touch(&self) -> bool {
        self.flags & USER_PRESENCE_REQUIRED != 0
    }

    /// Read the credential out of a parsed private key.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] when the key is not a FIDO key.
    pub fn from_private(key: &PrivateKey) -> Result<Self> {
        match key.key_data() {
            KeypairData::SkEd25519(sk) => Ok(Self {
                algorithm: Algorithm::SkEd25519,
                application: sk.public().application().to_string(),
                key_handle: sk.key_handle().to_vec(),
                flags: sk.flags(),
            }),
            KeypairData::SkEcdsaSha2NistP256(sk) => Ok(Self {
                algorithm: Algorithm::SkEcdsaSha2NistP256,
                application: sk.public().application().to_string(),
                key_handle: sk.key_handle().to_vec(),
                flags: sk.flags(),
            }),
            _ => Err(Error::Invalid("not a hardware-backed key".into())),
        }
    }
}

/// Length-prefixed string, the one primitive the SSH wire format is built from.
fn put_string(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&u32::try_from(bytes.len()).unwrap_or(u32::MAX).to_be_bytes());
    out.extend_from_slice(bytes);
}

/// Assemble the signature blob SSH expects for a security key.
///
/// From PROTOCOL.u2f:
///
/// ```text
/// string  algorithm name
/// string  signature
/// byte    authenticator flags
/// uint32  signature counter
/// ```
///
/// The flags and counter are not decoration: the token signed
/// `sha256(application) || flags || counter || sha256(data)`, so a verifier cannot
/// reconstruct that without them.
pub fn signature_blob(algorithm: &Algorithm, signature: &[u8], flags: u8, counter: u32) -> Vec<u8> {
    let mut blob = Vec::with_capacity(algorithm.as_str().len() + signature.len() + 16);

    put_string(&mut blob, algorithm.as_str().as_bytes());
    put_string(&mut blob, signature);
    blob.push(flags);
    blob.extend_from_slice(&counter.to_be_bytes());

    blob
}

/// Append a finished signature to the buffer russh gave us, framed as SSH expects.
///
/// `Signer::auth_sign` returns the **whole authentication request with the signature added**,
/// not the signature on its own - which is what the agent implementation does, and is easy
/// to get wrong: returning just the signature makes russh write it as the packet, and the
/// server sees a malformed message and disconnects without a word.
pub fn append_signature(to_sign: &mut Vec<u8>, blob: &[u8]) {
    put_string(to_sign, blob);
}

/// What the token is asked to sign: the SHA-256 of SSH's own signature input.
///
/// CTAP calls this the client data hash, and hashes it again inside the assertion.
pub fn challenge(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// Ask the token to sign, returning the SSH signature blob.
///
/// Blocks while the user touches the token, so callers must keep it off any thread that
/// has to stay responsive.
///
/// # Errors
///
/// [`Error::Ssh`] when no token is present, the assertion fails, or the token refuses.
/// A refusal for want of a PIN is reported as [`Error::PinRequired`] so the caller can
/// ask for one and try again, rather than guessing from the message.
pub fn sign(key: &SecurityKey, data: &[u8], pin: Option<&Zeroizing<String>>) -> Result<Vec<u8>> {
    // Asked for before the token is touched: without verification the authenticator will
    // not even admit to holding this credential.
    if key.needs_pin() && pin.is_none() {
        return Err(Error::PinRequired);
    }

    let devices = ctap_hid_fido2::get_fidokey_devices();
    if devices.is_empty() {
        return Err(Error::Ssh(
            "no security key found - plug the token in and try again".into(),
        ));
    }

    log::debug!("security key: {} device(s) found", devices.len());

    let device = ctap_hid_fido2::FidoKeyHidFactory::create(&ctap_hid_fido2::Cfg::init())
        .map_err(|e| Error::Ssh(format!("could not open the security key: {e}")))?;

    // Built by hand rather than through `get_assertion`, whose defaults ask for `uv: true`
    // on every assertion. A token with no user verification configured answers that with
    // CTAP2_ERR_INVALID_OPTION, so the convenience wrapper cannot sign an ordinary
    // touch-only key at all. OpenSSH asks for verification only when the credential was
    // enrolled with it; supplying a PIN conveys it instead, which is why `.pin()` clears
    // the option itself.
    let challenge = challenge(data);
    let builder = ctap_hid_fido2::fidokey::GetAssertionArgsBuilder::new(&key.application, &challenge)
        .add_credential_id(&key.key_handle);

    let builder = match pin {
        Some(pin) => builder.pin(pin.as_str()),
        None => builder.without_pin_and_uv(),
    };

    log::debug!(
        "security key: requesting assertion for {:?} ({} byte handle, pin: {})",
        key.application,
        key.key_handle.len(),
        pin.is_some()
    );

    let assertions = device
        .get_assertion_with_args(&builder.build())
        .map_err(|e| classify(&e.to_string(), pin.is_some()))?;

    let assertion = assertions.first().ok_or_else(|| {
        Error::Ssh("the security key returned no assertion for this credential".into())
    })?;

    let flags = assertion_flags(&assertion.auth_data)?;
    log::debug!(
        "security key: signed, {} byte signature, flags 0x{:02x}, counter {}",
        assertion.signature.len(),
        flags,
        assertion.sign_count
    );

    Ok(signature_blob(
        &key.algorithm,
        &assertion.signature,
        flags,
        assertion.sign_count,
    ))
}

/// Byte offset of the flags in CTAP authenticator data: `rpIdHash[32] || flags[1] || …`.
const FLAGS_OFFSET: usize = 32;

/// The raw authenticator flags byte.
///
/// Taken from the authenticator data verbatim rather than rebuilt from the parsed struct:
/// the verifier recomputes `sha256(rpId) || flags || counter || challenge`, so a flags byte
/// that merely means the same thing is not good enough - it has to be the same byte.
fn assertion_flags(auth_data: &[u8]) -> Result<u8> {
    auth_data.get(FLAGS_OFFSET).copied().ok_or_else(|| {
        Error::Ssh("the security key returned authenticator data that is too short".into())
    })
}

/// Turn a token error into something the caller can act on.
///
/// A PIN refusal is the one case worth distinguishing: it is recoverable by asking, and
/// the file's own flags do not reliably predict it.
fn classify(message: &str, had_pin: bool) -> Error {
    let lowered = message.to_lowercase();

    if !had_pin && (lowered.contains("pin") || lowered.contains("uv") || lowered.contains("0x36"))
    {
        return Error::PinRequired;
    }

    if lowered.contains("no_credentials") || lowered.contains("0x2e") {
        return Error::Ssh(
            "the security key does not hold this credential - it may belong to a different \
             token, or the key file may not match the one it was enrolled with"
                .into(),
        );
    }

    Error::Ssh(format!("the security key refused to sign: {message}"))
}

/// A russh [`Signer`] backed by the token.
///
/// The same seam the agent client uses, which is what lets a FIDO key authenticate without
/// any agent at all: russh hands over the bytes to sign and takes back a finished blob.
pub struct TokenSigner {
    key: SecurityKey,
    pin: Option<Zeroizing<String>>,
    /// Kept so a failure can be reported precisely; russh only sees "signing failed".
    failure: Option<Error>,
}

impl TokenSigner {
    pub fn new(key: SecurityKey, pin: Option<Zeroizing<String>>) -> Self {
        Self {
            key,
            pin,
            failure: None,
        }
    }

    /// Why signing failed, if it did.
    ///
    /// russh's `Signer::Error` has to be its own type, so the real error is kept here
    /// rather than flattened into "authentication failed" on the way out.
    pub fn take_failure(&mut self) -> Option<Error> {
        self.failure.take()
    }
}

impl russh::Signer for TokenSigner {
    // `AgentAuthError` is what the agent client uses and the only public error type that
    // satisfies the trait's `From<SendError>` bound. The real cause is kept in `failure`.
    type Error = russh::AgentAuthError;

    async fn auth_sign(
        &mut self,
        _key: &russh::keys::agent::AgentIdentity,
        _hash_alg: Option<russh::keys::ssh_key::HashAlg>,
        to_sign: Vec<u8>,
    ) -> std::result::Result<Vec<u8>, Self::Error> {
        let key = self.key.clone();
        let pin = self.pin.clone();
        // The blocking task takes a copy; the original becomes the reply russh writes.
        let data = to_sign.clone();

        // `sign` blocks for as long as it takes the user to touch the token, which would
        // otherwise stall every other task on this runtime thread.
        // Bounded: a token that is never touched, or one left busy by an attempt that was
        // abandoned, would otherwise leave the connection waiting for ever with nothing on
        // screen to say why.
        let signed = match tokio::time::timeout(
            TOUCH_TIMEOUT,
            tokio::task::spawn_blocking(move || sign(&key, &data, pin.as_ref())),
        )
        .await
        {
            Ok(joined) => joined.unwrap_or_else(|e| Err(Error::Ssh(format!("signing task failed: {e}")))),
            Err(_) => Err(Error::Ssh(
                "the security key was not touched in time - unplug it, plug it back in and \
                 try again"
                    .into(),
            )),
        };

        match signed {
            Ok(blob) => {
                let mut request = to_sign;
                append_signature(&mut request, &blob);
                Ok(request)
            }
            Err(e) => {
                self.failure = Some(e);
                // The trait's error carries nothing useful; `take_failure` has the reason.
                Err(russh::AgentAuthError::Send(russh::SendError {}))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_with_flags(flags: u8) -> SecurityKey {
        SecurityKey {
            algorithm: Algorithm::SkEd25519,
            application: "ssh:".into(),
            key_handle: vec![0; 48],
            flags,
        }
    }

    #[test]
    fn flags_decode_the_way_openssh_wrote_them() {
        // 0x25 is what `ssh-keygen -t ed25519-sk -O resident -O verify-required` records.
        let key = key_with_flags(0x25);
        assert!(key.needs_touch());
        assert!(key.needs_pin());
        assert!(key.is_resident());

        // A plain touch-only key.
        let plain = key_with_flags(0x01);
        assert!(plain.needs_touch());
        assert!(!plain.needs_pin());
        assert!(!plain.is_resident());
    }

    #[test]
    fn a_verify_required_key_asks_for_its_pin_before_the_token_is_touched() {
        let err = sign(&key_with_flags(0x25), b"data", None).unwrap_err();
        assert!(matches!(err, Error::PinRequired));
    }

    #[test]
    fn a_blob_carries_the_algorithm_signature_flags_and_counter() {
        let blob = signature_blob(&Algorithm::SkEd25519, &[0xAA; 64], 0x05, 7);
        let name = Algorithm::SkEd25519.as_str().as_bytes();

        // string algorithm
        assert_eq!(&blob[..4], &(name.len() as u32).to_be_bytes());
        assert_eq!(&blob[4..4 + name.len()], name);

        // string signature
        let sig_at = 4 + name.len();
        assert_eq!(&blob[sig_at..sig_at + 4], &64u32.to_be_bytes());
        assert_eq!(&blob[sig_at + 4..sig_at + 68], &[0xAA; 64]);

        // byte flags, uint32 counter
        let tail = sig_at + 68;
        assert_eq!(blob[tail], 0x05);
        assert_eq!(&blob[tail + 1..], &7u32.to_be_bytes());
        assert_eq!(blob.len(), tail + 5);
    }

    #[test]
    fn a_signature_is_appended_to_the_request_with_a_length_prefix() {
        // russh writes what comes back as the packet, so the reply is the request it gave
        // us plus the signature - length-prefixed, exactly as the agent path frames it.
        let mut request = b"REQUEST".to_vec();
        let blob = signature_blob(&Algorithm::SkEd25519, &[0x11; 64], 0x05, 3);

        append_signature(&mut request, &blob);

        assert_eq!(&request[..7], b"REQUEST");
        assert_eq!(&request[7..11], &(blob.len() as u32).to_be_bytes());
        assert_eq!(&request[11..], &blob[..]);
    }

    #[test]
    fn the_framed_length_matches_what_the_agent_path_declares() {
        // russh's agent writer declares `name.len() + sig.len() + 8 + 5` for an sk
        // signature; the blob has to be exactly that long or the packet is malformed.
        let name = Algorithm::SkEd25519.as_str();
        let blob = signature_blob(&Algorithm::SkEd25519, &[0; 64], 0, 0);
        assert_eq!(blob.len(), name.len() + 64 + 8 + 5);
    }

    #[test]
    fn the_counter_is_big_endian() {
        // A little-endian counter verifies on the machine that wrote it and nowhere else,
        // which is the worst way for this to be wrong.
        let blob = signature_blob(&Algorithm::SkEd25519, &[0; 64], 0, 0x0102_0304);
        assert_eq!(&blob[blob.len() - 4..], &[0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn the_challenge_is_the_sha256_of_the_data() {
        // Known answer: SHA-256 of "abc".
        let expected =
            hex_literal_ba7816bf();
        assert_eq!(challenge(b"abc"), expected);
    }

    fn hex_literal_ba7816bf() -> [u8; 32] {
        [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad,
        ]
    }

    #[test]
    fn the_flags_byte_comes_from_the_authenticator_data() {
        let mut auth_data = vec![0u8; 37];
        auth_data[FLAGS_OFFSET] = 0x05;
        assert_eq!(assertion_flags(&auth_data).unwrap(), 0x05);
    }

    #[test]
    fn truncated_authenticator_data_is_an_error_not_a_zero() {
        // Defaulting to zero would produce a signature that verifies nowhere and no
        // explanation of why.
        assert!(assertion_flags(&[0u8; 10]).is_err());
    }

    #[test]
    fn a_pin_refusal_is_recoverable_and_anything_else_is_not() {
        assert!(matches!(classify("CTAP2_ERR_PIN_REQUIRED", false), Error::PinRequired));
        // A key enrolled with verify-required refuses this way, and asking for the PIN is
        // exactly the recovery.
        assert!(matches!(classify("err = 0x36", false), Error::PinRequired));
        // Not a PIN problem: this is the token rejecting an option we sent, and prompting
        // for a PIN would send the user looking for one they may not have set.
        assert!(matches!(
            classify("response_status err = 0x2C CTAP2_ERR_INVALID_OPTION", false),
            Error::Ssh(_)
        ));
        assert!(matches!(classify("keepalive cancelled", false), Error::Ssh(_)));
        // Already tried a PIN: asking again for the same one would be a loop.
        assert!(matches!(classify("pin invalid", true), Error::Ssh(_)));
    }
}
