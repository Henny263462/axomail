//! Core domain layer of axomail.
//!
//! Will hold typed identifiers, account and message models, the
//! `IncomingMailProvider`/`OutgoingMailProvider` traits, and the central
//! `MailError` hierarchy (work package 1b). Contains no protocol, storage,
//! or UI code.
#![forbid(unsafe_code)]
