//! Folder-management command inputs.
//!
//! These carry the ids and the user-typed leaf name for the server-first
//! folder CRUD commands. Ids are validated by lookup in the command layer
//! (an unknown id yields a clean error), so no `checked_id` guard is needed on
//! the way in — that guard protects ids on their way *out* to JavaScript.

use serde::Deserialize;

/// Input for `create_folder`: a new folder under `parent_id` (or at the root
/// when `null`), named with the leaf `name`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFolderInput {
    /// Account the folder belongs to.
    pub account_id: i64,
    /// Parent folder id, or `null` to create a top-level folder.
    pub parent_id: Option<i64>,
    /// New leaf name (must not contain the server delimiter).
    pub name: String,
}

/// Input for `rename_folder`: give folder `folder_id` the new leaf `name`,
/// keeping it under the same parent.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameFolderInput {
    /// Folder to rename.
    pub folder_id: i64,
    /// New leaf name (must not contain the server delimiter).
    pub name: String,
}

/// Input for `set_folder_subscribed`: subscribe or unsubscribe a folder.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetFolderSubscribedInput {
    /// Folder to (un)subscribe.
    pub folder_id: i64,
    /// Desired subscription state.
    pub subscribed: bool,
}
