//! Account repository: create, read, list, and delete configured
//! accounts. Server configuration is persisted as plain columns; only
//! non-secret credential references are stored.

use std::num::NonZeroU16;

use axomail_core::{
    Account, AccountAuth, AccountId, ConnectionSecurity, CredentialRef, EmailAddress, HttpsUrl,
    IncomingProtocol, IncomingServerConfig, MailError, OAuthClientId, OAuthConfig, OAuthScopes,
    OutgoingServerConfig, ServerEndpoint,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error_map::map_sqlx;

/// Columns selected when reading an account, in [`AccountRow`] order.
const ACCOUNT_COLUMNS: &str = "id, display_name, email, \
    incoming_protocol, incoming_host, incoming_port, incoming_security, \
    incoming_username, incoming_credential_ref, \
    outgoing_host, outgoing_port, outgoing_security, \
    outgoing_username, outgoing_credential_ref, \
    auth_mechanism, oauth_client_id, oauth_auth_url, oauth_token_url, oauth_scopes";

/// Account data to insert, before an identifier has been assigned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewAccount {
    /// Name shown in the account list.
    pub display_name: String,
    /// Primary address used as the default sender.
    pub email: EmailAddress,
    /// Authentication mechanism (password or OAuth2); non-secret only.
    #[serde(default = "AccountAuth::default_password")]
    pub auth: AccountAuth,
    /// Receiving configuration.
    pub incoming: IncomingServerConfig,
    /// Sending configuration.
    pub outgoing: OutgoingServerConfig,
}

/// The mutable configuration of an existing account.
///
/// Only the fields an account edit is allowed to change are present: the
/// display name and each server's endpoint and transport security. The
/// incoming protocol, login names, primary address, and credential
/// references are immutable through this path — switching protocol or
/// re-keying credentials is deliberately out of scope, so they cannot be
/// touched here. Endpoints are [`ServerEndpoint`]s, so host and port are
/// already validated by the core layer before they reach the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountUpdate {
    /// New name shown in the account list.
    pub display_name: String,
    /// New receiving endpoint (host and port).
    pub incoming_endpoint: ServerEndpoint,
    /// New receiving transport security.
    pub incoming_security: ConnectionSecurity,
    /// New sending endpoint (host and port).
    pub outgoing_endpoint: ServerEndpoint,
    /// New sending transport security.
    pub outgoing_security: ConnectionSecurity,
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

        let auth = AuthColumns::from_auth(&new.auth);
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO accounts (
                display_name, email,
                incoming_protocol, incoming_host, incoming_port, incoming_security,
                incoming_username, incoming_credential_ref,
                outgoing_host, outgoing_port, outgoing_security,
                outgoing_username, outgoing_credential_ref,
                auth_mechanism, oauth_client_id, oauth_auth_url, oauth_token_url, oauth_scopes
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        .bind(auth.mechanism)
        .bind(auth.client_id)
        .bind(auth.auth_url)
        .bind(auth.token_url)
        .bind(auth.scopes)
        .fetch_one(self.pool)
        .await
        .map_err(|error| map_sqlx("insert_account", error))?;

        Ok(Account {
            id: AccountId::new(id),
            display_name: new.display_name.clone(),
            email: new.email.clone(),
            auth: new.auth.clone(),
            incoming: new.incoming.clone(),
            outgoing: new.outgoing.clone(),
        })
    }

    /// Applies an [`AccountUpdate`] to the account with `id` and returns the
    /// updated [`Account`].
    ///
    /// Only the display name and the two servers' host/port/security columns
    /// are written; the protocol, usernames, credential references, and email
    /// are left exactly as stored. The immutable columns are not part of the
    /// `SET` list, so this can never re-key credentials or switch protocol.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if no account has the given `id`,
    /// and [`MailError::Database`] on any query failure or if the updated row
    /// cannot be decoded back into a valid [`Account`].
    pub async fn update(
        &self,
        id: AccountId,
        update: &AccountUpdate,
    ) -> Result<Account, MailError> {
        let incoming_port = i64::from(update.incoming_endpoint.port().get());
        let outgoing_port = i64::from(update.outgoing_endpoint.port().get());

        let row = sqlx::query_as::<_, AccountRow>(sqlx::AssertSqlSafe(format!(
            "UPDATE accounts SET \
                display_name = ?, \
                incoming_host = ?, incoming_port = ?, incoming_security = ?, \
                outgoing_host = ?, outgoing_port = ?, outgoing_security = ? \
             WHERE id = ? RETURNING {ACCOUNT_COLUMNS}"
        )))
        .bind(update.display_name.as_str())
        .bind(update.incoming_endpoint.host())
        .bind(incoming_port)
        .bind(security_str(update.incoming_security))
        .bind(update.outgoing_endpoint.host())
        .bind(outgoing_port)
        .bind(security_str(update.outgoing_security))
        .bind(id.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("update_account", error))?;

        row.map(AccountRow::into_account)
            .transpose()?
            .ok_or_else(|| MailError::Configuration {
                message: "cannot update an account that does not exist".to_owned(),
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
    auth_mechanism: String,
    oauth_client_id: Option<String>,
    oauth_auth_url: Option<String>,
    oauth_token_url: Option<String>,
    oauth_scopes: Option<String>,
}

impl AccountRow {
    /// Converts a stored row into a validated [`Account`].
    fn into_account(self) -> Result<Account, MailError> {
        // Parse auth first: it only borrows `self`, so it must run before the
        // field moves below.
        let auth = self.parse_auth()?;
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
            auth,
            incoming,
            outgoing,
        })
    }

    /// Reconstructs the [`AccountAuth`] from the stored auth columns.
    fn parse_auth(&self) -> Result<AccountAuth, MailError> {
        match self.auth_mechanism.as_str() {
            "password" => Ok(AccountAuth::Password),
            "oauth2" => {
                let client_id = OAuthClientId::parse(oauth_field(
                    self.oauth_client_id.as_deref(),
                    "oauth_client_id",
                )?)?;
                let auth_url = HttpsUrl::parse(oauth_field(
                    self.oauth_auth_url.as_deref(),
                    "oauth_auth_url",
                )?)?;
                let token_url = HttpsUrl::parse(oauth_field(
                    self.oauth_token_url.as_deref(),
                    "oauth_token_url",
                )?)?;
                let scopes = OAuthScopes::new(
                    oauth_field(self.oauth_scopes.as_deref(), "oauth_scopes")?
                        .split_whitespace()
                        .map(str::to_owned)
                        .collect(),
                )?;
                Ok(AccountAuth::OAuth2(OAuthConfig {
                    client_id,
                    auth_url,
                    token_url,
                    scopes,
                }))
            }
            _ => Err(MailError::Database {
                message: "read_account: unknown auth mechanism".to_owned(),
            }),
        }
    }
}

/// The five auth columns to bind on insert, derived from an [`AccountAuth`].
struct AuthColumns {
    mechanism: &'static str,
    client_id: Option<String>,
    auth_url: Option<String>,
    token_url: Option<String>,
    scopes: Option<String>,
}

impl AuthColumns {
    /// Splits an [`AccountAuth`] into its persisted column values. Password
    /// accounts leave every OAuth column `NULL`.
    fn from_auth(auth: &AccountAuth) -> Self {
        match auth {
            AccountAuth::Password => Self {
                mechanism: "password",
                client_id: None,
                auth_url: None,
                token_url: None,
                scopes: None,
            },
            AccountAuth::OAuth2(config) => Self {
                mechanism: "oauth2",
                client_id: Some(config.client_id.as_str().to_owned()),
                auth_url: Some(config.auth_url.as_str().to_owned()),
                token_url: Some(config.token_url.as_str().to_owned()),
                scopes: Some(config.scopes.as_space_delimited()),
            },
        }
    }
}

/// Returns a required OAuth column value, mapping a missing one to a database
/// integrity error (an `oauth2` row must carry every OAuth field).
fn oauth_field<'a>(value: Option<&'a str>, column: &str) -> Result<&'a str, MailError> {
    value.ok_or_else(|| MailError::Database {
        message: format!("read_account: oauth2 account is missing {column}"),
    })
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
