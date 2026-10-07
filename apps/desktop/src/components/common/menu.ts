import type { IconName } from "./icons";

export interface MenuItem {
  id: string;
  label: string;
  icon?: IconName;
  disabled?: boolean;
  danger?: boolean;
  separatorBefore?: boolean;
}
