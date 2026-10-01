/**
 * The mapping from a status statement's icon to a rendered icon name.
 *
 * A separate lookup keeps the status vocabulary and the icon set independently
 * named, so neither has to widen to accommodate the other.
 */
import type { IconName } from "../../shared/ui/Icon";
import type { StatusStatement } from "../overview/status";

/** The icon for each status shape. */
export const STATUS_ICON: Record<StatusStatement["icon"], IconName> = {
  check: "check",
  clock: "clock",
  link: "link",
  pause: "pause",
  warning: "warning",
};
