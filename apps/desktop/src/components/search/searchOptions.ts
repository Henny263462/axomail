/**
 * Option lists and field/op helpers for the advanced-search builder, kept out of
 * the dialog SFC so it stays lean and under the file-size gate. Each option
 * pairs a backend enum value with an i18n key the dialog renders with `t()`.
 */
import type {
  SearchConditionOp,
  SearchField,
  SearchScopeKind,
  SearchSort,
} from "../../types/search";

/** One `<option>` in the builder: a backend value and its label key. */
export interface Option<T> {
  value: T;
  labelKey: string;
}

/** The searchable fields, in dialog order. */
export const FIELDS: Option<SearchField>[] = [
  { value: "from", labelKey: "search.field.from" },
  { value: "to", labelKey: "search.field.to" },
  { value: "cc", labelKey: "search.field.cc" },
  { value: "subject", labelKey: "search.field.subject" },
  { value: "body", labelKey: "search.field.body" },
  { value: "date", labelKey: "search.field.date" },
  { value: "tag", labelKey: "search.field.tag" },
  { value: "has_attachment", labelKey: "search.field.hasAttachment" },
  { value: "folder", labelKey: "search.field.folder" },
  { value: "account", labelKey: "search.field.account" },
  { value: "read", labelKey: "search.field.read" },
  { value: "starred", labelKey: "search.field.starred" },
];

/** Operators offered on a text field. */
export const TEXT_OPS: Option<SearchConditionOp>[] = [
  { value: "contains", labelKey: "search.op.contains" },
  { value: "not_contains", labelKey: "search.op.notContains" },
  { value: "is", labelKey: "search.op.is" },
  { value: "is_not", labelKey: "search.op.isNot" },
  { value: "starts_with", labelKey: "search.op.startsWith" },
  { value: "ends_with", labelKey: "search.op.endsWith" },
  { value: "matches_regex", labelKey: "search.op.matchesRegex" },
];

/** Operators offered on the date field. */
export const NUMBER_OPS: Option<SearchConditionOp>[] = [
  { value: "greater_than", labelKey: "search.op.greaterThan" },
  { value: "less_than", labelKey: "search.op.lessThan" },
  { value: "is", labelKey: "search.op.is" },
  { value: "is_not", labelKey: "search.op.isNot" },
];

/** Operators offered on a boolean field (has-attachment / read / starred). */
export const BOOL_OPS: Option<SearchConditionOp>[] = [
  { value: "is", labelKey: "search.op.is" },
  { value: "is_not", labelKey: "search.op.isNot" },
];

/** The scope kinds. */
export const SCOPES: Option<SearchScopeKind>[] = [
  { value: "all_folders", labelKey: "search.scope.allFolders" },
  { value: "folder", labelKey: "search.scope.folder" },
  { value: "folder_recursive", labelKey: "search.scope.folderRecursive" },
];

/** The sort orders. */
export const SORTS: Option<SearchSort>[] = [
  { value: "date_desc", labelKey: "search.sort.dateDesc" },
  { value: "date_asc", labelKey: "search.sort.dateAsc" },
  { value: "subject_asc", labelKey: "search.sort.subjectAsc" },
  { value: "subject_desc", labelKey: "search.sort.subjectDesc" },
];

/** The input kind a field's value uses. */
export type FieldKind = "text" | "date" | "bool";

/** Which value-input kind a field wants. */
export function fieldKind(field: SearchField): FieldKind {
  if (field === "date") return "date";
  if (field === "has_attachment" || field === "read" || field === "starred") {
    return "bool";
  }
  return "text";
}

/** The operator set valid for a field. */
export function opsFor(field: SearchField): Option<SearchConditionOp>[] {
  const kind = fieldKind(field);
  if (kind === "date") return NUMBER_OPS;
  if (kind === "bool") return BOOL_OPS;
  return TEXT_OPS;
}

/** A default operator that is valid for `field`. */
export function defaultOpFor(field: SearchField): SearchConditionOp {
  return opsFor(field)[0]?.value ?? "is";
}
