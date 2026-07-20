//! Contact-group (mailing-list) methods on [`ContactRepo`]. A group names a set
//! of contacts; typing the group name into a recipient field expands to its
//! members' primary addresses.

use axomail_core::{EmailAddress, MailError};

use super::{Contact, ContactGroup, ContactGroupId, ContactId, ContactRepo, ContactRow};
use crate::error_map::map_sqlx;

impl ContactRepo<'_> {
    /// Creates a group. The name is unique case-insensitively.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] for a blank name and
    /// [`MailError::Database`] on any query failure (including a duplicate
    /// name).
    pub async fn create_group(&self, name: &str) -> Result<ContactGroup, MailError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(MailError::Configuration {
                message: "a group name must not be empty".to_owned(),
            });
        }
        let id: i64 =
            sqlx::query_scalar("INSERT INTO contact_groups (name) VALUES (?) RETURNING id")
                .bind(name)
                .fetch_one(self.pool)
                .await
                .map_err(|error| map_sqlx("create_group", error))?;
        Ok(ContactGroup {
            id: ContactGroupId::new(id),
            name: name.to_owned(),
            member_count: 0,
        })
    }

    /// Lists every group with its member count, in name order.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn list_groups(&self) -> Result<Vec<ContactGroup>, MailError> {
        let rows = sqlx::query_as::<_, GroupRow>(
            "SELECT g.id, g.name, COUNT(m.contact_id) AS member_count
             FROM contact_groups g
             LEFT JOIN contact_group_members m ON m.group_id = g.id
             GROUP BY g.id
             ORDER BY g.name COLLATE NOCASE, g.id",
        )
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("list_groups", error))?;
        Ok(rows.into_iter().map(GroupRow::into_group).collect())
    }

    /// Deletes a group, cascading to its memberships (never its contacts).
    /// Returns `true` if a row was removed.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn delete_group(&self, id: ContactGroupId) -> Result<bool, MailError> {
        let result = sqlx::query("DELETE FROM contact_groups WHERE id = ?")
            .bind(id.value())
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("delete_group", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Adds a contact to a group. Returns `true` if the membership was new (a
    /// duplicate is a silent no-op).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure, including a bad
    /// group or contact reference.
    pub async fn add_member(
        &self,
        group: ContactGroupId,
        contact: ContactId,
    ) -> Result<bool, MailError> {
        let result = sqlx::query(
            "INSERT INTO contact_group_members (group_id, contact_id) VALUES (?, ?)
             ON CONFLICT DO NOTHING",
        )
        .bind(group.value())
        .bind(contact.value())
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("add_group_member", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Removes a contact from a group. Returns `true` if a membership existed.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn remove_member(
        &self,
        group: ContactGroupId,
        contact: ContactId,
    ) -> Result<bool, MailError> {
        let result =
            sqlx::query("DELETE FROM contact_group_members WHERE group_id = ? AND contact_id = ?")
                .bind(group.value())
                .bind(contact.value())
                .execute(self.pool)
                .await
                .map_err(|error| map_sqlx("remove_group_member", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Returns a group's member contacts (with addresses), in name order.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn members_of(&self, group: ContactGroupId) -> Result<Vec<Contact>, MailError> {
        let rows = sqlx::query_as::<_, ContactRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM contacts c
             JOIN contact_group_members m ON m.contact_id = c.id
             WHERE m.group_id = ?
             ORDER BY c.display_name COLLATE NOCASE, c.id",
            super::columns_prefixed("c")
        )))
        .bind(group.value())
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("members_of_group", error))?;
        self.attach_emails(rows).await
    }

    /// Returns the ids of the groups a contact belongs to.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn groups_of(&self, contact: ContactId) -> Result<Vec<ContactGroupId>, MailError> {
        let ids: Vec<i64> =
            sqlx::query_scalar("SELECT group_id FROM contact_group_members WHERE contact_id = ?")
                .bind(contact.value())
                .fetch_all(self.pool)
                .await
                .map_err(|error| map_sqlx("groups_of_contact", error))?;
        Ok(ids.into_iter().map(ContactGroupId::new).collect())
    }

    /// Returns the primary addresses of a group's members (members without an
    /// address are skipped), for expanding a group into a recipient field.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn group_addresses(
        &self,
        group: ContactGroupId,
    ) -> Result<Vec<EmailAddress>, MailError> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT c.primary_email FROM contacts c
             JOIN contact_group_members m ON m.contact_id = c.id
             WHERE m.group_id = ? AND c.primary_email IS NOT NULL
             ORDER BY c.display_name COLLATE NOCASE, c.id",
        )
        .bind(group.value())
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("group_addresses", error))?;
        rows.iter()
            .map(|value| EmailAddress::parse(value))
            .collect()
    }

    /// Finds a group whose name matches `name` exactly (case-insensitively),
    /// for expanding a group typed into a recipient field.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn find_group_by_name(&self, name: &str) -> Result<Option<ContactGroup>, MailError> {
        let row = sqlx::query_as::<_, GroupRow>(
            "SELECT g.id, g.name, COUNT(m.contact_id) AS member_count
             FROM contact_groups g
             LEFT JOIN contact_group_members m ON m.group_id = g.id
             WHERE g.name = ?
             GROUP BY g.id",
        )
        .bind(name.trim())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("find_group_by_name", error))?;
        Ok(row.map(GroupRow::into_group))
    }
}

/// Raw `contact_groups` row joined with its member count.
#[derive(sqlx::FromRow)]
struct GroupRow {
    id: i64,
    name: String,
    member_count: i64,
}

impl GroupRow {
    fn into_group(self) -> ContactGroup {
        ContactGroup {
            id: ContactGroupId::new(self.id),
            name: self.name,
            member_count: self.member_count,
        }
    }
}
