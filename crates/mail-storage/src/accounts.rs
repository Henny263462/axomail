//! Account repository: create, read, list, and delete configured
//! accounts. Server configuration is persisted as plain columns; only
//! non-secret credential references are stored.

use std::num::NonZeroU16;

use axomail_core::{
    Account, AccountId, ConnectionSecurity, CredentialRef, EmailAddress, IncomingProtocol,
    IncomingServerConfig, MailError, OutgoingServerConfig, ServerEndpoint,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error_map::map_sqlx;

/// Columns selected when reading an account, in [`AccountRow`] order.
const ACCOUNT_COLUMNS: &str = "id, display_name, email, \
    incoming_protocol, incoming_host, incoming_port, incoming_security, \
    incoming_username, incoming_credential_ref, \
    outgoing_host, outgoing_port, outgoing_security, \
    outgoing_username, outgoing_credential_ref";

/// Account data to insert, before an identifier has been assigned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewAccount {
    /// Name shown in the account list.
    pub display_name: String,
    /// Primary address used as the default sender.
    pub email: EmailAddress,
    /// Receiving configuration.
    pub incoming: IncomingServerConfig,
    /// Sending configuration.
    pub outgoing: OutgoingServerConfig,
}

/// Repository over the `accounts` table.
#[derive(Debug)]
pub struct AccountRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> AccountRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Inserts an account and returns it with its assigned identifier.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn insert(&self, new: &NewAccount) -> Result<Account, MailError> {
        let incoming_port = i64::from(new.incoming.endpoint.port().get());
        let outgoing_port = i64::from(new.outgoing.endpoint.port().get());

        let id: i64 = sqlx::query_scalar(
            "INSERT INTO accounts (
                display_name, email,
                incoming_protocol, incoming_host, incoming_port, incoming_security,
                incoming_username, incoming_credential_ref,
                outgoing_host, outgoing_port, outgoing_security,
                outgoing_username, outgoing_credential_ref
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id",
        )
        .bind(new.display_name.as_str())
        .bind(new.email.as_str())
        .bind(protocol_str(new.incoming.protocol))
        .bind(new.incoming.endpoint.host())
        .bind(incoming_port)
        .bind(security_str(new.incoming.security))
        .bind(new.incoming.username.as_str())
        .bind(new.incoming.credential.as_str())
        .bind(new.outgoing.endpoint.host())
        .bind(outgoing_port)
        .bind(security_str(new.outgoing.security))
        .bind(new.outgoing.username.as_str())
        .bind(new.outgoing.credential.as_str())
        .fetch_one(self.pool)
        .await
        .map_err(|error| map_sqlx("insert_account", error))?;

        Ok(Account {
            id: AccountId::new(id),
            display_name: new.display_name.clone(),
            email: new.email.clone(),
            incoming: new.incoming.clone(),
            outgoing: new.outgoing.clone(),
        })
    }

    /// Fetches an account by id, or `None` if it does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure and if a
    /// stored row cannot be decoded into a valid [`Account`].
    pub async fn get(&self, id: AccountId) -> Result<Option<Account>, MailError> {
        let row = sqlx::query_as::<_, AccountRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE id = ?"
        )))
        .bind(id.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("get_account", error))?;

        row.map(AccountRow::into_account).transpose()
    }

    /// Lists all accounts ordered by identifier.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure and if a
    /// stored row cannot be decoded into a valid [`Account`].
    pub async fn list(&self) -> Result<Vec<Account>, MailError> {
        let rows = sqlx::query_as::<_, AccountRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {ACCOUNT_COLUMNS} FROM accounts ORDER BY id"
        )))
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("list_accounts", error))?;

        rows.into_iter().map(AccountRow::into_account).collect()
    }

    /// Deletes an account (cascading to its mailboxes and messages).
    ///
    /// Returns `true` if a row was removed.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn delete(&self, id: AccountId) -> Result<bool, MailError> {
        let result = sqlx::query("DELETE FROM accounts WHERE id = ?")
            .bind(id.value())
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("delete_account", error))?;
        Ok(result.rows_affected() > 0)
    }
}

/// Raw `accounts` row, mapped by column name.
#[derive(sqlx::FromRow)]
struct AccountRow {
    id: i64,
    display_name: String,
    email: String,
    incoming_protocol: String,
    incoming_host: String,
    incoming_port: i64,
    incoming_security: String,
    incoming_username: String,
    incoming_credential_ref: String,
    outgoing_host: String,
    outgoing_port: i64,
    outgoing_security: String,
    outgoing_username: String,
    outgoing_credential_ref: String,
}

impl AccountRow {
    /// Converts a stored row into a validated [`Account`].
    fn into_account(self) -> Result<Account, MailError> {
        let incoming = IncomingServerConfig {
            protocol: parse_protocol(&self.incoming_protocol)?,
            endpoint: ServerEndpoint::new(self.incoming_host, port(self.incoming_port)?)?,
            security: parse_security(&self.incoming_security)?,
            username: self.incoming_username,
            credential: CredentialRef::new(self.incoming_credential_ref),
        };
        let outgoing = OutgoingServerConfig {
            endpoint: ServerEndpoint::new(self.outgoing_host, port(self.outgoing_port)?)?,
            security: parse_security(&self.outgoing_security)?,
            username: self.outgoing_username,
            credential: CredentialRef::new(self.outgoing_credential_ref),
        };
        Ok(Account {
            id: AccountId::new(self.id),
            display_name: self.display_name,
            email: EmailAddress::parse(&self.email)?,
            incoming,
            outgoing,
        })
    }
}

/// Serialized form of an incoming protocol.
fn protocol_str(protocol: IncomingProtocol) -> &'static str {
    match protocol {
        IncomingProtocol::Imap => "imap",
        IncomingProtocol::Pop3 => "pop3",
    }
}

/// Serialized form of a connection-security mode.
fn security_str(security: ConnectionSecurity) -> &'static str {
    match security {
        ConnectionSecurity::Tls => "tls",
        ConnectionSecurity::StartTls => "starttls",
    }
}

/// Parses a stored protocol string.
fn parse_protocol(value: &str) -> Result<IncomingProtocol, MailError> {
    match value {
        "imap" => Ok(IncomingProtocol::Imap),
        "pop3" => Ok(IncomingProtocol::Pop3),
        _ => Err(MailError::Database {
            message: "read_account: unknown incoming protocol".to_owned(),
        }),
    }
}

/// Parses a stored connection-security string.
fn parse_security(value: &str) -> Result<ConnectionSecurity, MailError> {
    match value {
        "tls" => Ok(ConnectionSecurity::Tls),
        "starttls" => Ok(ConnectionSecurity::StartTls),
        _ => Err(MailError::Database {
            message: "read_account: unknown connection security".to_owned(),
        }),
    }
}

/// Converts a stored port back into a [`NonZeroU16`].
fn port(value: i64) -> Result<NonZeroU16, MailError> {
    u16::try_from(value)
        .ok()
        .and_then(NonZeroU16::new)
        .ok_or_else(|| MailError::Database {
            message: "read_account: stored port out of range".to_owned(),
        })
}
