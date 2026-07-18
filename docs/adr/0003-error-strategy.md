# 0003. Error strategy

Date: 2026-07-18
Status: accepted

## Context

The project brief requires a central typed error hierarchy that
distinguishes at least: network, DNS, TLS, authentication, protocol,
rate-limit, timeout, parsing, database, keychain, filesystem,
unsupported-capability, temporary, permanent, and user-configuration
errors — with user-friendly messages, technical detail preserved for
diagnostics, and no secrets ever emitted.

## Decision

- One central enum, `MailError` in `axomail-core`, built with
  `thiserror` 2. Variants carry structured context fields, not
  pre-rendered strings from foreign errors.
- Classification is a **method**, not extra variants:
  `MailError::class()` maps every variant to
  `ErrorClass::{Temporary, Permanent, UserConfiguration}`. Retry logic
  in `mail-sync` keys exclusively off this classification.
- Secrets are excluded by construction: variants have no fields that
  could carry credentials, and adapters must redact protocol data
  before building `message` fields. `Authentication` deliberately has
  no payload at all.
- Cancellation is modeled as `MailError::Cancelled` and classified
  `Permanent` so retry engines never resurrect cancelled work.
- TLS errors are classified `Permanent`: certificate problems must
  surface to the user unchanged, never be retried into silence, and
  there is no "accept insecure certificates" pathway.
- `unwrap()`/`expect()` remain banned in production paths (enforced as
  clippy `unwrap_used`/`expect_used` warnings promoted to errors in CI);
  test modules opt out locally.
- The enum is `#[non_exhaustive]`; downstream matches need a wildcard
  arm, which lets later work packages add variants (e.g. rate-limit
  subtypes) without breaking crates.

## Consequences

- The UI can render friendly messages per variant and per class while a
  diagnostics view shows the `Debug` form.
- Mapping library errors (sqlx, keyring, TLS stacks) into `MailError`
  happens at each adapter boundary; conversion helpers land with those
  adapters, keeping `axomail-core` dependency-free beyond serde and
  thiserror.
- DNS "name does not exist" answers should be mapped by adapters to
  `Configuration`, not `Dns`, so users see a "check the server name"
  hint; this is documented on the variant.
