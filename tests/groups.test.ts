/**
 * Account groups: the keys of one provider account, shown together.
 *
 * A group takes the place of its first key and lists its keys in order; the
 * account's total is read from the key that read the balance last, and each
 * key is its own spend limit; the total states only the periods some key
 * reports.
 */
import { describe, expect, it } from "vitest";

import { overviewItems } from "../src/features/overview/Overview";
import { widgetAccounts } from "../src/features/widget/model";
import type { AccountSnapshot, GroupSnapshot } from "../src/generated/bindings";
import {
  accountView,
  groupLabel,
  groupReader,
  groupSpendLine,
  isGroupable,
  keyLimitOf,
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

  it("splits a key into the account's total and the key's own limit", () => {
    const member = key("a", 1, "2026-10-01T11:00:00.000Z");
    expect(accountView(member).windows.map((window) => window.metric_role)).toEqual([
      "prepaid_balance",
    ]);
    expect(keyLimitOf(member)?.metric_role).toBe("extra_spend_cap");
  });

  it("reads the total from the key that read the balance last", () => {
    const older = key("a", 1, "2026-10-01T10:00:00.000Z");
    const newer = key("b", 2, "2026-10-01T11:00:00.000Z");
    const noBalance = { ...key("c", 3, "2026-10-01T12:00:00.000Z"), balance: null };
    expect(groupReader([older, newer, noBalance])?.account_id).toBe("b");
    expect(groupReader([noBalance])?.account_id).toBe("c");
  });

  it("states only the spend periods some key reports", () => {
    expect(groupSpendLine(group())).toBe("Keys spent $1.50 today · $20.10 this month");
    expect(groupSpendLine(group({ key_spend: null }))).toBeNull();
  });

  it("names a group Group 1 while account names are hidden", () => {
    const hidden = preferences({
      privacy: { ...preferences().privacy, alias_mode: "stable_aliases" },
    });
    expect(groupLabel(hidden, [group()], group())).toBe("Group 1");
    expect(groupLabel(preferences(), [group()], group())).toBe("Work");
  });

  it("draws the group's total as one widget tile, then a ring tile for each key", () => {
    const older = key("a", 1, "2026-10-01T10:00:00.000Z");
    const newer = key("b", 2, "2026-10-01T11:00:00.000Z");
    const tiles = widgetAccounts([older, newer], preferences(), NOW, [group()]);
    expect(tiles.map((tile) => tile.id)).toEqual(["group:group-work", "a", "b"]);
    expect(tiles.map((tile) => tile.name)).toEqual([
      "OpenRouter · Work",
      "Work · a",
      "Work · b",
    ]);
    // The total is the account's balance alone; each key is its limit alone.
    expect(tiles[0]?.rows.map((entry) => entry.tag)).toEqual(["Credit"]);
    expect(tiles[1]?.rows.map((entry) => entry.tag)).toEqual(["Key"]);
    expect(tiles[1]?.rings).toHaveLength(1);
    expect(tiles[1]?.headline.value).toBe("$11.60");
  });
});
