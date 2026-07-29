# MOB-03 virtual security review

Status: conditional approval for emulator/simulator development only  
Reviewed: 2026-07-30  
Physical-device release approval: not granted

## Boundary

- Flutter receives status facts and typed intents only. It never receives a
  signer seed, vault key, recovery key, native path or raw database handle.
- Android creates 192 random bytes, encrypts them with an Android Keystore
  AES-GCM wrapping key, and stores the envelope and install marker only under
  `noBackupFilesDir`.
- iOS stores the same bounded material as a
  `kSecAttrAccessibleWhenUnlockedThisDeviceOnly` Keychain item and keeps the
  paired marker below an Application Support directory marked
  `NSURLIsExcludedFromBackupKey`.
- Rust copies the temporary material into zeroizing memory, binds the exact
  epoch, instance nonce, vault key and three public signer domains to
  `bootstrap.redb`, and drops the key session on lock/background.

## Identity and signer review

- Transport Node, feed-event and Actor-root seeds occupy independent random
  domains. Equal private or public domains are rejected.
- Rust exposes typed signing operations with distinct domain-separation
  contexts. No generic sign endpoint exists in Dart or Pigeon.
- The encrypted private vault reuses `ku-core::foundation::PrivateVault`,
  `RedbVerifiedBackend`, XChaCha20-Poly1305 and zeroizing `VaultKey`; it does
  not introduce a new hand-written vault cipher.

## Install binding and restore behavior

- A stored authority opens only when profile version, installation epoch,
  instance nonce, keyed binding digest and all three public identities match.
- Authority with a missing marker, half-created marker/envelope pair, absent
  platform key or mismatched material fails closed. It is not reset
  automatically.
- Clean install retires an orphaned platform key before generating new
  material. On iOS this explicitly handles a Keychain item that may survive
  uninstall.
- The Android emulator injection drill removes only the install marker while
  retaining authority bytes and observes a redacted rejection with no runtime
  snapshot.

## Archive and recovery foundation

- Portable data uses `OBARV001`, a versioned encrypted archive with an
  encrypted manifest, random nonce prefix, independent 1 MiB AEAD chunks,
  authenticated indices, declared length, BLAKE3 payload digest and no
  plaintext-private export path.
- File archive/restore runs in bounded memory, fsyncs the staging file and
  atomically renames only after the complete digest verifies, so multi-gigabyte
  datasets are not loaded into RAM and a corrupt partial never becomes active.
- The archive key is derived from a separate 32-byte recovery key. It is not
  the installation wrapping key or a human password.
- Wrong key, corruption, truncation, trailing bytes, manifest/payload
  mismatch, unsupported version and zero recovery key all fail closed.
- File picking, recovery-key verification UX and staged activation remain
  later presentation/workflow slices; this review does not claim that recovery
  UI has shipped.

## Backup and log review

- Android sets `allowBackup=false` and also supplies explicit cloud-backup,
  device-transfer and legacy full-backup exclusions for every mutable data
  domain.
- iOS marks the whole mutable Application Support root excluded from backup and
  uses this-device-only Keychain accessibility.
- Security history accepts bounded uppercase event/scope codes only. It stores
  no content, key, raw private identifier or path and retains at most 512
  records.
- Android runtime logs contain bounded booleans/generation/status only.
  Security failures log one fixed redacted message.

## Residual release gates

- Inspect real Android cloud/device-transfer output and real iOS backup output.
- Exercise biometric/credential cancellation, lockout, reboot-before-unlock and
  protected-data-unavailable states on physical devices.
- Exercise iOS uninstall/reinstall with a surviving Keychain item.
- Complete recovery/export UI, staged restore and destructive reset review.
- Obtain an external cryptographic review before public recovery packages are
  treated as a stable format.
