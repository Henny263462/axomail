# 0015. OpenPGP stack: rPGP (pure-Rust RustCrypto), keychain-guarded keys

Date: 2026-07-20
Status: accepted

## Context

Wave 5 Track 5.1 (cluster S3) brings OpenPGP to axomail for Thunderbird
parity: verify a signature badge, decrypt inline/PGP-MIME messages, and sign
and encrypt on send, with per-recipient key management. The app has no crypto
hooks for this today — encrypted parts fall through as opaque attachments.

An OpenPGP implementation is a large, security-critical dependency, so — as
with the OAuth HTTP client (ADR-0009) and the attachment file dialogs
(ADR-0010) — the crate is chosen and added **once** in the Wave 5 Stage 0
sequential step, ahead of the track fan-out, and vetted here. Track 5.1 then
writes the PGP module in `crates/mail-security` (where key access and the
keychain already live) and the migration `0021` for key metadata. No PGP code
is written in Stage 0; only the dependency and this record.

Project rules apply: MIT-compatible licenses only (`deny.toml`), a minimal
dependency count with versions verified against crates.io at adoption
(ADR-0002/0006), **no secrets in the database** (ADR-0001), redacted errors
across the IPC boundary (ADR-0003), one crypto stack rather than several
(ADR-0006), and the four-layer sandboxed-iframe HTML render (ADR-0007) must
stay intact. The load-bearing constraint is the license policy: the historically
dominant OpenPGP library (Sequoia) defaults to a GPL/LGPL C crypto backend
(Nettle), which the allow-list forbids.

## Decision

### Library: `pgp` (rPGP) 0.20.0, pure-Rust, `default-features = false`

```toml
pgp = { version = "0.20.0", default-features = false }
```

rPGP is a pure-Rust OpenPGP implementation built entirely on the RustCrypto
crates (`rsa`, `sha2`, `aes-gcm`, `ed25519-dalek`, `curve25519-dalek`, `dsa`,
`k256`/`p256`/`p384`/`p521`, `blake2`, `sha3`, …) plus the pure-Rust `zlib-rs`
and `flate2`. It has **no** Nettle/GPG/OpenSSL backend at all — there is no
GPL/LGPL crypto to select or exclude, which is precisely why it, and not
Sequoia, fits the license policy. `pgp` 0.20.0 is `MIT OR Apache-2.0`
(verified against crates.io on 2026-07-20; `rust-version` 1.88, below the
workspace 1.95).

**`default-features = false`** drops rPGP's single default feature, `bzip2`,
which pulls the C `bzip2` / `bzip2-sys` binding. That is non-pure-Rust and adds
a build-time C toolchain requirement on every platform, only to support
BZIP2-compressed PGP packets — which are rare and optional. ZIP and ZLIB
compression remain available through the pure-Rust `flate2` / `zlib-rs` that
rPGP still pulls, so interoperability is unaffected in practice. This yields
the minimal pure-Rust feature set for sign/verify/encrypt/decrypt. The other
optional features stay off: `asm` (platform assembly), `draft-pqc`
(post-quantum drafts), `large-rsa`, `wasm`/`web-time`/`getrandom` (browser
targets — the desktop app supplies its own RNG), and
`malformed-artifact-compat`.

### ASCII armor: existing `base64`, rPGP's bundled `crc24`

PGP ASCII armor is Radix-64: standard base64 (RFC 4648) plus a CRC-24
checksum. `base64` 0.22.1 is already a first-class, cargo-deny-vetted workspace
dependency (ADR-0009) and covers the base64 half; rPGP bundles `crc24` (present
in the resolved graph) for the checksum and performs armor encode/decode
internally. **No new armor dependency is required.**

### Keys: secret material in the OS keychain, never the database

Per ADR-0001, secret key material never enters SQLite or logs. The Track 5.1
migration `0021` stores only **non-secret** key metadata (key id / fingerprint,
user ids, algorithm, created/expiry, a trust/validity marker, and a keychain
reference), mirroring how OAuth tokens are handled (ADR-0009): the armored
secret key is serialized and stored in the OS keychain under the account's
existing credential-reference scheme, and the payload type redacts it in its
`Debug` impl. Public keys and the trust model may live in the database; private
keys do not. Passphrase entry, when a key is passphrase-protected, is handled
in the backend and the passphrase is never sent to the webview.

### Rendering and IPC boundaries are unchanged

Decrypted plaintext and verification results are produced in the Rust backend
and cross the IPC boundary as sanitized DTOs; decrypted HTML still renders
through the ADR-0007 sandboxed iframe. Errors are redacted (ADR-0003): a failed
decrypt or a bad signature returns a classified `MailError`, never raw crypto
detail. No CSP change and no new OS capability are needed — OpenPGP is
computation over bytes the app already has, not new network or filesystem
reach.

### S/MIME: verify-only if feasible, else deferred

S/MIME (CMS over X.509) is a separate stack from OpenPGP and is **out of scope**
for the rPGP dependency. Track 5.1 will attempt verify-only S/MIME if it can be
done within the existing RustCrypto/`rustls` primitives already in the tree;
full S/MIME (encrypt/sign, certificate trust chains) is a documented non-goal
for this push (see the plan's "Explicitly deferred"). This ADR does not add an
S/MIME dependency.

### Alternatives considered

- **Sequoia (`sequoia-openpgp`) with the Nettle backend** — the most mature and
  widely-audited Rust OpenPGP library, but its default `crypto-nettle` backend
  links the **LGPL** Nettle C library, which is not on the `deny.toml`
  allow-list, and it adds a second (C) crypto stack against ADR-0006. Rejected
  on license and single-stack grounds.
- **Sequoia with its RustCrypto backend (`crypto-rust`)** — removes the Nettle
  license problem, but that backend is explicitly documented by Sequoia as not
  production-hardened, and Sequoia pulls a heavier dependency and build surface
  than rPGP. It would also inherit the same RustCrypto `rsa` advisory below.
  Rejected in favour of the library whose *primary* backend is the pure-Rust
  one.
- **Shelling out to the GnuPG (`gpg`) binary** — avoids a Rust crypto dependency
  but requires an external binary to be installed, spawns a subprocess with the
  user's secret keyring outside our keychain model (breaking ADR-0001), and is
  hard to sandbox. Rejected.
- **Hand-rolling OpenPGP packet parsing on the raw RustCrypto crates** — the
  RustCrypto primitives are already in the tree, but OpenPGP framing, key
  formats, and MIME integration are exactly the large, error-prone surface a
  vetted library exists to provide. Rejected.

## Consequences

- axomail can implement OpenPGP sign/verify/encrypt/decrypt in Track 5.1 with a
  pure-Rust, MIT/Apache stack and no second crypto backend. Gmail/M365 and every
  other account type benefit equally; PGP is protocol-agnostic.
- **Large dependency surface.** rPGP transitively adds ~85 crates — the full
  RustCrypto suite (ciphers, hashes, curves, `rsa`, `dsa`), `num-bigint-dig`,
  `nom`, the `bitvec` family, `snafu`, and the pure-Rust `zlib-rs`. All resolve
  within the `deny.toml` allow-list (MIT/Apache/BSD/ISC/Zlib/Unicode), confirmed
  by `cargo deny check licenses` → **licenses ok**. The dep is declared on
  `crates/mail-security` now (Stage 0) and compiles even though unused, so the
  build already validates it; `cargo deny check` gates any license regression on
  every change.
- **Accepted advisory — RUSTSEC-2023-0071 (`rsa` Marvin timing sidechannel).**
  The RustCrypto `rsa` 0.9 crate (pulled by rPGP for RSA OpenPGP keys) carries
  a known non-constant-time vulnerability with **no fixed release available**
  (the constant-time rewrite is unreleased, RustCrypto/RSA#626). RSA is
  mandatory in RFC 4880 and rPGP exposes no feature to disable it, so **every**
  pure-Rust OpenPGP stack shares this dependency — it is not avoidable by
  choosing a different library. The Marvin attack requires a *network-observable
  RSA private-key timing oracle* (e.g. a TLS/decryption server processing
  attacker-chosen ciphertexts at scale). axomail performs RSA private-key
  operations — decrypting received mail and signing outgoing mail — **locally on
  the user's own machine**, never as a remote decryption/signing service, which
  is exactly the advisory's own "local use on a non-compromised computer is
  fine" workaround. The residual risk is therefore low. It is accepted with an
  explicit, documented `ignore` entry in `deny.toml` (distinct from the
  Tauri "unmaintained" notices, which are not vulnerabilities) so
  `cargo deny check` passes. **Re-check trigger:** when a constant-time `rsa`
  release lands, drop the ignore and upgrade. This is the single reason the
  advisory gate has an accepted vulnerability, and it is surfaced for the
  wave's adversarial review.
- **BZIP2-compressed PGP messages are not decompressed** (no `bzip2` feature).
  ZIP/ZLIB and uncompressed messages work; a BZIP2 packet — uncommon in
  practice — will surface as an unsupported-compression error rather than being
  silently mis-read. Re-enabling it would reintroduce a C build dependency and
  should be a deliberate, separately-reviewed change.
- If rPGP ever changed its primary backend or license, or if the `rsa`
  situation were judged unacceptable, Track 5.1 (S3) is descopable without
  affecting the rest of Wave 5; this record would then document the block.
