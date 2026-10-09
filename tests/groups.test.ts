/**
 * Account groups: the keys of one provider account, shown together.
 *
 * A group takes the place of its first key and lists its keys in order; each
 * key drops the account balance the group shows once; the total states only
 * the periods some key reports.
 */
import { describe, expect, it } from "vitest";

import { overviewItems } from "../src/features/overview/Overview";
import { widgetAccounts } from "../src/features/widget/model";
import type { AccountSnapshot, GroupSnapshot } from "../src/generated/bindings";
import {
  groupBalanceLine,
  groupLabel,
  groupSpendLine,
  isGroupable,
  memberView,
} from "../src/shared/format/group";
import { placeAccounts } from "../src/shared/state/order";
import { account, keyLimit, NOW, preferences, prepaidBalance } from "./fixtures";

const WORK = { id: "group-work", name: "Work" };

function key(id: string, ordinal: number, readAt: string): AccountSnapshot {
  const { window, summary } = prepaidBalance();
  return {
    ...account(id, "openrouter", ordinal, [window, keyLimit()], {
      nickname: id,
      rank: 74.4,
      balance: {
        ...summary,
        key_spend: { today_minor: 100, week_minor: null, month_minor: 900 },
      },
    }),
    last_success_at: readAt,
    group: WORK,
  };
}

function group(overrides: Partial<GroupSnapshot> = {}): GroupSnapshot {
  return {
    id: WORK.id,
    provider_id: "openrouter",
    name: WORK.name,
    account_ids: ["a", "b"],
    balance: prepaidBalance().summary,
    key_spend: { today_minor: 150, week_minor: null, month_minor: 2010 },
    ...overrides,
  };
}

describe("account groups", () => {
  it("can group only the providers connected with API keys", () => {
    expect(isGroupable("openrouter")).toBe(true);
    expect(isGroupable("open_code_go")).toBe(true);
    expect(isGroupable("claude")).toBe(false);
  });

  it("puts a group where its first key falls and keeps every other row in place", () => {
    const alone = account("alone", "claude", 2, [], { rank: 90 });
    const rows = placeAccounts([
      key("a", 1, "2026-10-01T11:00:00.000Z"),
      alone,
      key("b", 3, "2026-10-01T11:00:00.000Z"),
    ]);
    const items = overviewItems(rows, [group()]);
    expect(items.map((item) => item.kind)).toEqual(["group", "account"]);
    const first = items[0];
    expect(first?.kind === "group" ? first.members.length : 0).toBe(2);
  });

  it("keeps a key whose group the snapshot does not list as a row of its own", () => {
    const rows = placeAccounts([key("a", 1, "2026-10-01T11:00:00.000Z")]);
    expect(overviewItems(rows, []).map((item) => item.kind)).toEqual(["account"]);
  });

  it("shows a key in its group without the account balance and with its limit", () => {
    const view = memberView(key("a", 1, "2026-10-01T11:00:00.000Z"));
    expect(view.windows.map((window) => window.metric_role)).toEqual(["extra_spend_cap"]);
    expect(view.show_key_limit).toBe(true);
  });

  it("states the balance and only the spend periods some key reports", () => {
    expect(groupBalanceLine(group())).toBe("$37.20 left");
    expect(groupSpendLine(group())).toBe("Keys spent $1.50 today · $20.10 this month");
    expect(groupBalanceLine(group({ balance: null }))).toBeNull();
    expect(groupSpendLine(group({ key_spend: null }))).toBeNull();
  });

  it("names a group Group 1 while account names are hidden", () => {
    const hidden = preferences({
      privacy: { ...preferences().privacy, alias_mode: "stable_aliases" },
    });
    expect(groupLabel(hidden, [group()], group())).toBe("Group 1");
    expect(groupLabel(preferences(), [group()], group())).toBe("Work");
  });

  it("draws one widget tile per group, from the key that read the balance last", () => {
    const older = key("a", 1, "2026-10-01T10:00:00.000Z");
    const newer = key("b", 2, "2026-10-01T11:00:00.000Z");
    const tiles = widgetAccounts([older, newer], preferences(), NOW, [group()]);
    expect(tiles).toHaveLength(1);
    expect(tiles[0]?.id).toBe("b");
    expect(tiles[0]?.name).toBe("OpenRouter · Work");
  });
});
