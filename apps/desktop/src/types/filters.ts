/**
 * Message-filter (rules engine) DTOs, mirroring the Rust `dto::filter` types.
 * Enum values are the backend's snake_case strings.
 */

/** How a filter's conditions combine. */
export type FilterMatchType = "all" | "any";

/** The message part a filter condition tests. */
export type FilterConditionField =
  | "from"
  | "to"
  | "cc"
  | "subject"
  | "body"
  | "has_attachment"
  | "size"
  | "date"
  | "tag";

/** How a filter condition compares its field. */
export type FilterConditionOp =
  | "contains"
  | "not_contains"
  | "is"
  | "is_not"
  | "starts_with"
  | "ends_with"
  | "matches_regex"
  | "greater_than"
  | "less_than";

/** What a filter action does when its filter matches. */
export type FilterActionKind =
  | "move"
  | "copy"
  | "tag"
  | "star"
  | "mark_read"
  | "mark_unread"
  | "mark_junk"
  | "delete_to_trash"
  | "forward"
  | "set_priority"
  | "stop";

/** One filter condition. */
export interface FilterCondition {
  field: FilterConditionField;
  op: FilterConditionOp;
  value: string;
}

/** One filter action. */
export interface FilterAction {
  kind: FilterActionKind;
  param: string;
}

/** A persisted message filter with its conditions and actions. */
export interface FilterDto {
  id: number;
  accountId: number | null;
  name: string;
  enabled: boolean;
  matchType: FilterMatchType;
  sortOrder: number;
  stopOnMatch: boolean;
  conditions: FilterCondition[];
  actions: FilterAction[];
}

/** The create/update payload for a filter. */
export interface FilterInput {
  accountId: number | null;
  name: string;
  enabled: boolean;
  matchType: FilterMatchType;
  stopOnMatch: boolean;
  conditions: FilterCondition[];
  actions: FilterAction[];
}

/** The result of running filters over a folder. */
export interface FilterRunOutcome {
  matched: number;
  actionsApplied: number;
  forwardsQueued: number;
}

/** An account's run-on-arrival filtering setting. */
export interface AccountFilterSettings {
  arrivalEnabled: boolean;
}
