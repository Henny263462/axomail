/**
 * Advanced-search and saved-search DTOs, mirroring the Rust `dto::saved_search`
 * types. Enum values are the backend's snake_case strings.
 *
 * The match type and the comparison operator are reused verbatim from the filter
 * model ({@link ./filters}), so the two subsystems share one condition language;
 * the search adds the folder/account/read/starred *fields* and the scope/sort a
 * filter has no use for. Search results reuse `SearchResultDto` from `./mail`.
 */
import type { FilterConditionOp, FilterMatchType } from "./filters";

/** Which part of a message an advanced-search condition tests. */
export type SearchField =
  | "from"
  | "to"
  | "cc"
  | "subject"
  | "body"
  | "date"
  | "tag"
  | "has_attachment"
  | "folder"
  | "account"
  | "read"
  | "starred";

/** The comparison operator (reused from the filter model). */
export type SearchConditionOp = FilterConditionOp;

/** How a search's conditions combine (reused from the filter model). */
export type SearchMatchType = FilterMatchType;

/** The structural scope kind. */
export type SearchScopeKind = "all_folders" | "folder" | "folder_recursive";

/** How the matched results are ordered. */
export type SearchSort =
  | "date_desc"
  | "date_asc"
  | "subject_asc"
  | "subject_desc";

/** One advanced-search condition. */
export interface SearchCondition {
  field: SearchField;
  op: SearchConditionOp;
  value: string;
}

/** The structural scope: which folders are candidates at all. */
export interface SearchScope {
  kind: SearchScopeKind;
  /** The target folder for the folder / folder-recursive kinds; null otherwise. */
  mailboxId: number | null;
}

/** A complete advanced-search query. */
export interface SearchQuery {
  matchType: SearchMatchType;
  conditions: SearchCondition[];
  scope: SearchScope;
  sort: SearchSort;
}

/** A persisted saved search (a sidebar virtual folder). */
export interface SavedSearchDto {
  id: number;
  name: string;
  query: SearchQuery;
}

/** The create/update payload for a saved search. */
export interface SavedSearchInput {
  name: string;
  query: SearchQuery;
}
