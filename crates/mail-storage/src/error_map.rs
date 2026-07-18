//! Translation of `sqlx` failures into [`MailError`].
//!
//! Error messages carry only the failing operation's name plus a stable
//! category derived from the error kind — never SQL text, bound values,
//! or credential references. This keeps diagnostics useful while ensuring
//! no user data or secret can leak through an error string (ADR-0003).

use axomail_core::MailError;

/// Maps a `sqlx` error into [`MailError::Database`], tagged with the
/// name of the operation that produced it.
pub(crate) fn map_sqlx(operation: &str, error: sqlx::Error) -> MailError {
    MailError::Database {
        message: format!("{operation}: {}", category(&error)),
    }
}

/// Maps a migration failure into [`MailError::Database`].
///
/// The full error is logged (it contains only schema/version detail, no
/// user data) while the returned message stays generic.
pub(crate) fn map_migrate(error: sqlx::migrate::MigrateError) -> MailError {
    tracing::error!(error = ?error, "database migration failed");
    MailError::Database {
        message: "applying database migrations failed".to_owned(),
    }
}

/// Stable, value-free category for a `sqlx` error.
///
/// Only the most stable variants are named explicitly; everything else
/// collapses to a generic label so no foreign error text is ever
/// interpolated into the message.
fn category(error: &sqlx::Error) -> &'static str {
    match error {
        sqlx::Error::RowNotFound => "row not found",
        sqlx::Error::PoolTimedOut => "connection pool timed out",
        sqlx::Error::PoolClosed => "connection pool closed",
        sqlx::Error::Database(db) => database_category(db.as_ref()),
        _ => "database operation failed",
    }
}

/// Category for a driver-level database error, based on its constraint
/// kind rather than the driver's message.
fn database_category(error: &dyn sqlx::error::DatabaseError) -> &'static str {
    match error.kind() {
        sqlx::error::ErrorKind::UniqueViolation => "unique constraint violated",
        sqlx::error::ErrorKind::ForeignKeyViolation => "foreign-key constraint violated",
        sqlx::error::ErrorKind::NotNullViolation => "not-null constraint violated",
        sqlx::error::ErrorKind::CheckViolation => "check constraint violated",
        _ => "constraint violated",
    }
}
