use std::fs::{DirBuilder, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, bail, ensure};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAGIC: &[u8; 16] = b"EMBERS-CONT-V1\0\0";
const NONCE_BYTES: usize = 12;
const DEFAULT_MAX_OBJECT_BYTES: u64 = 64 * 1024 * 1024;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub struct F1r3driveContinuationStore {
    root: Arc<PathBuf>,
    encryption_key: [u8; 32],
    capability_key: [u8; 32],
    max_object_bytes: u64,
    write_lock: Arc<Mutex<()>>,
}

impl F1r3driveContinuationStore {
    pub fn open(
        mount_path: impl AsRef<Path>,
        encryption_key: [u8; 32],
        capability_key: [u8; 32],
        max_object_bytes: Option<u64>,
    ) -> anyhow::Result<Self> {
        let mount_path = mount_path.as_ref();
        verify_f1r3drive_mount(mount_path)?;
        Self::open_at_path(mount_path, encryption_key, capability_key, max_object_bytes)
    }

    fn open_at_path(
        root: impl AsRef<Path>,
        encryption_key: [u8; 32],
        capability_key: [u8; 32],
        max_object_bytes: Option<u64>,
    ) -> anyhow::Result<Self> {
        let root = root.as_ref();
        ensure!(root.is_dir(), "continuation store root is not a directory");
        ensure!(
            !root.symlink_metadata()?.file_type().is_symlink(),
            "continuation store root cannot be a symbolic link"
        );
        let object_root = root.join("embers-agent-teams").join("v1");
        let mut directory_builder = DirBuilder::new();
        directory_builder.recursive(true).mode(0o700);
        directory_builder.create(&object_root).with_context(|| {
            format!(
                "failed to create continuation store at {}",
                object_root.display()
            )
        })?;
        enforce_private_directory(&object_root)?;
        let root = object_root.canonicalize()?;
        let max_object_bytes = max_object_bytes.unwrap_or(DEFAULT_MAX_OBJECT_BYTES);
        ensure!(
            max_object_bytes > 0,
            "continuation object size limit must be positive"
        );
        Ok(Self {
            root: Arc::new(root),
            encryption_key,
            capability_key,
            max_object_bytes,
            write_lock: Arc::new(Mutex::new(())),
        })
    }

    #[cfg(test)]
    pub(crate) fn open_for_test(
        root: impl AsRef<Path>,
        encryption_key: [u8; 32],
        capability_key: [u8; 32],
    ) -> anyhow::Result<Self> {
        Self::open_at_path(root, encryption_key, capability_key, None)
    }

    pub fn put<T: Serialize>(
        &self,
        kind: ArtifactKind,
        run_id: &str,
        object_id: &str,
        value: &T,
    ) -> anyhow::Result<ArtifactPointer> {
        validate_identity(run_id, "run id")?;
        validate_identity(object_id, "object id")?;
        let plaintext = serde_json::to_vec(value)?;
        ensure!(
            plaintext.len() as u64 <= self.max_object_bytes,
            "continuation object exceeds configured size limit"
        );
        let capability = self.derive_capability(kind, run_id, object_id);
        let capability_hash = hex::encode(Sha256::digest(capability.as_bytes()));
        let path = self.object_path(kind, &capability)?;
        let aad = aad(kind, run_id, object_id, &capability_hash);
        let _guard = self
            .write_lock
            .lock()
            .expect("continuation store write mutex poisoned");

        if path.exists() {
            let existing = self.read_existing_after_publication(&path, &aad)?;
            ensure!(
                existing == plaintext,
                "idempotency conflict: continuation object identity already stores different bytes"
            );
            return self.pointer_for(kind, run_id, object_id, capability_hash, &path);
        }

        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .expect("AES-256-GCM accepts a 32-byte key");
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: &plaintext,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| anyhow::anyhow!("failed to encrypt continuation object"))?;
        let mut bytes = Vec::with_capacity(MAGIC.len() + NONCE_BYTES + ciphertext.len());
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&nonce);
        bytes.extend_from_slice(&ciphertext);

        let parent = path
            .parent()
            .context("continuation object path has no parent")?;
        let mut directory_builder = DirBuilder::new();
        directory_builder.recursive(true).mode(0o700);
        directory_builder.create(parent)?;
        enforce_private_directory(parent)?;
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = self.read_existing_after_publication(&path, &aad)?;
                ensure!(
                    existing == plaintext,
                    "idempotency conflict: another process stored different bytes for this continuation object identity"
                );
                return self.pointer_for(kind, run_id, object_id, capability_hash, &path);
            }
            Err(error) => return Err(error.into()),
        };
        if let Err(error) = file.write_all(&bytes) {
            drop(file);
            let _ = std::fs::remove_file(&path);
            return Err(error.into());
        }
        // F1r3drive's fsync/flush contract waits for its deployment queue.  In
        // manual-propose mode this does not return until the encrypted object is
        // finalized and reconstructible after a process or machine restart.
        file.sync_all()?;
        drop(file);
        self.pointer_for(kind, run_id, object_id, capability_hash, &path)
    }

    pub fn get<T: for<'de> Deserialize<'de>>(
        &self,
        pointer: &ArtifactPointer,
        run_id: &str,
        object_id: &str,
    ) -> anyhow::Result<T> {
        ensure!(
            pointer.run_id == run_id,
            "artifact pointer run identity mismatch"
        );
        ensure!(
            pointer.object_id == object_id,
            "artifact pointer object identity mismatch"
        );
        let capability = self.derive_capability(pointer.kind, run_id, object_id);
        let capability_hash = hex::encode(Sha256::digest(capability.as_bytes()));
        ensure!(
            pointer.capability_hash == capability_hash,
            "artifact pointer capability hash mismatch"
        );
        let path = self.object_path(pointer.kind, &capability)?;
        ensure!(path.is_file(), "continuation object is missing");
        let bytes = std::fs::read(&path)?;
        ensure!(
            pointer.content_hash == hex::encode(Sha256::digest(&bytes)),
            "continuation object content hash mismatch"
        );
        let aad = aad(pointer.kind, run_id, object_id, &capability_hash);
        let plaintext = self.decrypt_bytes(&bytes, &aad)?;
        serde_json::from_slice(&plaintext).context("continuation object plaintext is invalid JSON")
    }

    /// Resolves an artifact's public pointer from its stable private identity.
    /// The derived capability itself never leaves this store boundary.
    pub fn locate(
        &self,
        kind: ArtifactKind,
        run_id: &str,
        object_id: &str,
    ) -> anyhow::Result<Option<ArtifactPointer>> {
        validate_identity(run_id, "run id")?;
        validate_identity(object_id, "object id")?;
        let capability = self.derive_capability(kind, run_id, object_id);
        let path = self.object_path(kind, &capability)?;
        if !path.is_file() {
            return Ok(None);
        }
        let capability_hash = hex::encode(Sha256::digest(capability.as_bytes()));
        self.pointer_for(kind, run_id, object_id, capability_hash, &path)
            .map(Some)
    }

    fn pointer_for(
        &self,
        kind: ArtifactKind,
        run_id: &str,
        object_id: &str,
        capability_hash: String,
        path: &Path,
    ) -> anyhow::Result<ArtifactPointer> {
        let metadata = path.metadata()?;
        ensure!(
            metadata.len() <= self.max_object_bytes + 1024,
            "encrypted continuation object exceeds configured size limit"
        );
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 16 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok(ArtifactPointer {
            schema_version: "embers-artifact-pointer-v1".to_owned(),
            kind,
            run_id: run_id.to_owned(),
            object_id: object_id.to_owned(),
            content_hash: hex::encode(hasher.finalize()),
            capability_hash,
            ciphertext_bytes: metadata.len(),
        })
    }

    fn read_plaintext(&self, path: &Path, aad: &str) -> anyhow::Result<Vec<u8>> {
        let bytes = std::fs::read(path)?;
        self.decrypt_bytes(&bytes, aad)
    }

    fn read_existing_after_publication(&self, path: &Path, aad: &str) -> anyhow::Result<Vec<u8>> {
        let mut last_error = None;
        for _ in 0..250 {
            match self.read_plaintext(path, aad) {
                Ok(plaintext) => {
                    // A competing process can expose O_EXCL-created bytes before
                    // it reaches fsync.  Flushing the completed authenticated
                    // object here joins the same global deployment queue and
                    // establishes the identical durability boundary.
                    File::open(path)?.sync_all()?;
                    return Ok(plaintext);
                }
                Err(error) => {
                    last_error = Some(error);
                    thread::sleep(Duration::from_millis(20));
                }
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("continuation object was not published")))
    }

    fn decrypt_bytes(&self, bytes: &[u8], aad: &str) -> anyhow::Result<Vec<u8>> {
        ensure!(
            bytes.len() >= MAGIC.len() + NONCE_BYTES,
            "continuation object is truncated"
        );
        ensure!(
            &bytes[..MAGIC.len()] == MAGIC,
            "continuation object magic mismatch"
        );
        let mut nonce_bytes = [0u8; NONCE_BYTES];
        nonce_bytes.copy_from_slice(&bytes[MAGIC.len()..MAGIC.len() + NONCE_BYTES]);
        let nonce = Nonce::from(nonce_bytes);
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .expect("AES-256-GCM accepts a 32-byte key");
        cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &bytes[MAGIC.len() + NONCE_BYTES..],
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| anyhow::anyhow!("continuation object authentication failed"))
    }

    fn derive_capability(&self, kind: ArtifactKind, run_id: &str, object_id: &str) -> String {
        let mut hmac = <HmacSha256 as Mac>::new_from_slice(&self.capability_key)
            .expect("HMAC-SHA256 accepts arbitrary key bytes");
        hmac.update(kind.as_str().as_bytes());
        hmac.update(&[0]);
        hmac.update(run_id.as_bytes());
        hmac.update(&[0]);
        hmac.update(object_id.as_bytes());
        hex::encode(hmac.finalize().into_bytes())
    }

    fn object_path(&self, kind: ArtifactKind, capability: &str) -> anyhow::Result<PathBuf> {
        ensure!(
            capability.len() == 64 && capability.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "derived artifact capability is malformed"
        );
        Ok(self
            .root
            .join(kind.as_str())
            .join(&capability[..2])
            .join(format!("{capability}.encrypted")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArtifactKind {
    Command,
    Continuation,
    EffectOutbox,
    Observation,
    Output,
    RunRequest,
}

impl ArtifactKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Command => "commands",
            Self::Continuation => "continuations",
            Self::EffectOutbox => "effect-outbox",
            Self::Observation => "observations",
            Self::Output => "outputs",
            Self::RunRequest => "run-requests",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactPointer {
    pub schema_version: String,
    pub kind: ArtifactKind,
    pub run_id: String,
    pub object_id: String,
    pub content_hash: String,
    pub capability_hash: String,
    pub ciphertext_bytes: u64,
}

fn aad(kind: ArtifactKind, run_id: &str, object_id: &str, capability_hash: &str) -> String {
    format!(
        "embers-agent-teams-v1\u{0}{}\u{0}{run_id}\u{0}{object_id}\u{0}{capability_hash}",
        kind.as_str()
    )
}

fn validate_identity(value: &str, label: &str) -> anyhow::Result<()> {
    ensure!(!value.is_empty(), "{label} cannot be empty");
    ensure!(value.len() <= 512, "{label} exceeds 512 bytes");
    ensure!(
        value
            .chars()
            .all(|character| !character.is_control() && character != '\0'),
        "{label} contains a control character"
    );
    Ok(())
}

fn enforce_private_directory(path: &Path) -> anyhow::Result<()> {
    let private = std::fs::Permissions::from_mode(0o700);
    if let Err(error) = std::fs::set_permissions(path, private) {
        // Some FUSE implementations do not implement chmod.  Creation still
        // receives mode 0700; accept the filesystem only if getattr confirms
        // that no group/other permission survived the caller's umask.
        let actual_mode = path.metadata()?.permissions().mode();
        ensure!(
            actual_mode.trailing_zeros() >= 6 || is_mount_owner_isolated_f1r3drive(path)?,
            "continuation directory {} is neither mode-private nor confined to the F1r3drive mount owner, and chmod failed: {error}",
            path.display()
        );
    }
    Ok(())
}

fn is_mount_owner_isolated_f1r3drive(path: &Path) -> anyhow::Result<bool> {
    let canonical = path.canonicalize()?;
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo")?;
    let mut best_match: Option<(usize, bool)> = None;
    for line in mountinfo.lines() {
        let Some((before, after)) = line.split_once(" - ") else {
            continue;
        };
        let fields = before.split_whitespace().collect::<Vec<_>>();
        let after = after.split_whitespace().collect::<Vec<_>>();
        let Some(mount) = fields.get(4).map(Path::new) else {
            continue;
        };
        if !canonical.starts_with(mount)
            || !after
                .first()
                .is_some_and(|filesystem| filesystem.starts_with("fuse"))
            || !after
                .get(1)
                .is_some_and(|source| source.eq_ignore_ascii_case("f1r3drive"))
        {
            continue;
        }
        let options = fields
            .get(5)
            .into_iter()
            .chain(after.get(2))
            .flat_map(|options| options.split(','))
            .collect::<Vec<_>>();
        let mount_uid = options
            .iter()
            .find_map(|option| option.strip_prefix("user_id="))
            .and_then(|uid| uid.parse::<u32>().ok());
        let isolated = !options.contains(&"allow_other")
            && mount_uid
                .is_some_and(|uid| uid == path.metadata().ok().map_or(u32::MAX, |m| m.uid()));
        let depth = mount.as_os_str().len();
        if best_match.is_none_or(|(best_depth, _)| depth > best_depth) {
            best_match = Some((depth, isolated));
        }
    }
    Ok(best_match.is_some_and(|(_, isolated)| isolated))
}

fn verify_f1r3drive_mount(path: &Path) -> anyhow::Result<()> {
    ensure!(path.is_absolute(), "F1r3drive mount path must be absolute");
    ensure!(path.is_dir(), "F1r3drive mount path is not a directory");
    let canonical = path.canonicalize()?;
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo")
        .context("cannot inspect Linux mount table for F1r3drive")?;
    let matched = mountinfo.lines().any(|line| {
        let Some((before, after)) = line.split_once(" - ") else {
            return false;
        };
        let fields = before.split_whitespace().collect::<Vec<_>>();
        let after = after.split_whitespace().collect::<Vec<_>>();
        fields
            .get(4)
            .map(Path::new)
            .is_some_and(|mount| canonical.starts_with(mount))
            && after
                .first()
                .is_some_and(|filesystem| filesystem.starts_with("fuse"))
            && after
                .get(1)
                .is_some_and(|source| source.to_ascii_lowercase().contains("f1r3drive"))
    });
    if !matched {
        bail!(
            "{} is not inside the fsname=f1r3drive FUSE mount; refusing to place continuations on ordinary disk",
            canonical.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    #[test]
    fn encrypted_objects_round_trip_and_are_idempotent() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store =
            F1r3driveContinuationStore::open_for_test(directory.path(), [7u8; 32], [9u8; 32])
                .expect("test store should open");
        let value = json!({"private":"prompt", "cursor":["node-a"]});
        let first = store
            .put(ArtifactKind::Continuation, "run-1", "revision-0", &value)
            .expect("first write should succeed");
        let second = store
            .put(ArtifactKind::Continuation, "run-1", "revision-0", &value)
            .expect("idempotent write should succeed");
        assert_eq!(first, second);
        let restored: serde_json::Value = store
            .get(&first, "run-1", "revision-0")
            .expect("object should decrypt");
        assert_eq!(restored, value);

        let capability = store.derive_capability(ArtifactKind::Continuation, "run-1", "revision-0");
        let raw = std::fs::read(
            store
                .object_path(ArtifactKind::Continuation, &capability)
                .expect("object path"),
        )
        .expect("ciphertext should be readable");
        assert!(!String::from_utf8_lossy(&raw).contains("prompt"));
    }

    #[test]
    fn identity_reuse_with_different_plaintext_is_rejected() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store =
            F1r3driveContinuationStore::open_for_test(directory.path(), [1u8; 32], [2u8; 32])
                .expect("test store should open");
        store
            .put(ArtifactKind::Command, "run", "command", &json!(1))
            .expect("first write should succeed");
        assert!(
            store
                .put(ArtifactKind::Command, "run", "command", &json!(2))
                .is_err()
        );
    }

    #[test]
    fn content_and_capability_tampering_are_detected() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store =
            F1r3driveContinuationStore::open_for_test(directory.path(), [3u8; 32], [4u8; 32])
                .expect("test store should open");
        let pointer = store
            .put(
                ArtifactKind::Observation,
                "run",
                "effect",
                &json!({"ok":true}),
            )
            .expect("write should succeed");
        let mut wrong_capability = pointer.clone();
        wrong_capability.capability_hash = "00".repeat(32);
        assert!(
            store
                .get::<Value>(&wrong_capability, "run", "effect")
                .is_err()
        );

        let mut wrong_hash = pointer;
        wrong_hash.content_hash = "ff".repeat(32);
        assert!(store.get::<Value>(&wrong_hash, "run", "effect").is_err());
    }

    use proptest::prelude::*;

    /// A bounded JSON value. Numbers are restricted to `i64` so serialization is
    /// exact (floats would let NaN and precision loss break round-trip equality).
    fn arb_json() -> impl Strategy<Value = Value> {
        let leaf = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::from),
            any::<i64>().prop_map(Value::from),
            any::<String>().prop_map(Value::from),
        ];
        leaf.prop_recursive(3, 32, 8, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
                prop::collection::vec(("[a-z]{1,8}", inner), 0..6)
                    .prop_map(|entries| Value::Object(entries.into_iter().collect())),
            ]
        })
    }

    /// A run or object identifier accepted by `validate_identity` (non-empty,
    /// printable, no NUL or control characters).
    fn arb_identity() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9_-]{1,32}".prop_map(String::from)
    }

    proptest! {
        /// put ∘ get is the identity for any JSON value under any identity — every
        /// value the store accepts decrypts back to exactly itself.
        #[test]
        fn encrypted_objects_round_trip_for_any_json(
            value in arb_json(),
            run_id in arb_identity(),
            object_id in arb_identity(),
        ) {
            let directory = tempfile::tempdir().expect("temporary directory");
            let store = F1r3driveContinuationStore::open_for_test(directory.path(), [7u8; 32], [9u8; 32])
                .expect("test store opens");
            let pointer = store
                .put(ArtifactKind::Continuation, &run_id, &object_id, &value)
                .expect("write succeeds");
            let restored: Value = store.get(&pointer, &run_id, &object_id).expect("read succeeds");
            prop_assert_eq!(restored, value);
        }

        /// Writing different bytes to an identity that already holds a value is
        /// always rejected (the store is content-addressed and immutable).
        #[test]
        fn reusing_an_identity_with_different_bytes_is_rejected(
            first in arb_json(),
            second in arb_json(),
            run_id in arb_identity(),
            object_id in arb_identity(),
        ) {
            prop_assume!(first != second);
            let directory = tempfile::tempdir().expect("temporary directory");
            let store = F1r3driveContinuationStore::open_for_test(directory.path(), [1u8; 32], [2u8; 32])
                .expect("test store opens");
            store
                .put(ArtifactKind::Command, &run_id, &object_id, &first)
                .expect("first write succeeds");
            prop_assert!(
                store
                    .put(ArtifactKind::Command, &run_id, &object_id, &second)
                    .is_err()
            );
        }

        /// Without the capability key you cannot even address the object: a reader
        /// with a different capability key rejects the pointer on the capability
        /// hash, before any decryption.
        #[test]
        fn a_different_capability_key_cannot_address_the_object(
            value in arb_json(),
            run_id in arb_identity(),
            object_id in arb_identity(),
        ) {
            let directory = tempfile::tempdir().expect("temporary directory");
            let writer = F1r3driveContinuationStore::open_for_test(directory.path(), [3u8; 32], [4u8; 32])
                .expect("writer opens");
            let reader = F1r3driveContinuationStore::open_for_test(directory.path(), [3u8; 32], [5u8; 32])
                .expect("reader opens");
            let pointer = writer
                .put(ArtifactKind::Output, &run_id, &object_id, &value)
                .expect("write succeeds");
            prop_assert!(reader.get::<Value>(&pointer, &run_id, &object_id).is_err());
        }

        /// Without the encryption key you cannot read the object: a reader sharing
        /// the capability key (so the pointer resolves) but holding a different
        /// encryption key fails AEAD decryption.
        #[test]
        fn a_different_encryption_key_cannot_decrypt_the_object(
            value in arb_json(),
            run_id in arb_identity(),
            object_id in arb_identity(),
        ) {
            let directory = tempfile::tempdir().expect("temporary directory");
            let writer = F1r3driveContinuationStore::open_for_test(directory.path(), [3u8; 32], [8u8; 32])
                .expect("writer opens");
            let reader = F1r3driveContinuationStore::open_for_test(directory.path(), [6u8; 32], [8u8; 32])
                .expect("reader opens");
            let pointer = writer
                .put(ArtifactKind::Output, &run_id, &object_id, &value)
                .expect("write succeeds");
            prop_assert!(reader.get::<Value>(&pointer, &run_id, &object_id).is_err());
        }

        /// Any single-byte corruption of the stored object is detected on read.
        #[test]
        fn any_single_byte_of_tampering_is_detected(
            value in arb_json(),
            run_id in arb_identity(),
            object_id in arb_identity(),
            byte_index in any::<usize>(),
            xor in 1u8..=255,
        ) {
            let directory = tempfile::tempdir().expect("temporary directory");
            let store = F1r3driveContinuationStore::open_for_test(directory.path(), [7u8; 32], [9u8; 32])
                .expect("test store opens");
            let pointer = store
                .put(ArtifactKind::Observation, &run_id, &object_id, &value)
                .expect("write succeeds");
            let capability = store.derive_capability(ArtifactKind::Observation, &run_id, &object_id);
            let path = store
                .object_path(ArtifactKind::Observation, &capability)
                .expect("object path");
            let mut bytes = std::fs::read(&path).expect("read ciphertext");
            // The stored object is always at least MAGIC + nonce bytes, so it is non-empty.
            let index = byte_index % bytes.len();
            bytes[index] ^= xor;
            std::fs::write(&path, &bytes).expect("write tampered ciphertext");
            prop_assert!(store.get::<Value>(&pointer, &run_id, &object_id).is_err());
        }
    }
}
