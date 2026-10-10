/**
 * The account order the person chooses: the least remaining first (the
 * default), their own arrangement, or by provider.
 *
 * An arrangement lists what it knows first and new accounts after; a group's
 * keys stay together and move as one; readings never move an arranged
 * account; and the widget follows the same order.
 */
import { describe, expect, it } from "vitest";

import { widgetAccounts } from "../src/features/widget/model";
import { withAccountSort } from "../src/features/settings/preferences";
import type { AccountSnapshot } from "../src/generated/bindings";
import {
  arrangedOrder,
  canonicalOrder,
  moveInOrder,
  placeAccounts,
} from "../src/shared/state/order";
import { account, NOW, percent, preferences, window as quotaWindow } from "./fixtures";

const WORK = { id: "work", name: "Work", spend_shown: true, key_shown: true };

/** An account with `left` percent of its weekly allowance left. */
function left(id: string, ordinal: number, remaining: number): AccountSnapshot {
  return account(
    id,
    "codex",
    ordinal,
    [quotaWindow(`${id}-w`, "weekly", percent(remaining))],
    {
      rank: remaining,
    },
  );
}

function key(id: string, ordinal: number): AccountSnapshot {
  return { ...account(id, "openrouter", ordinal, [], { rank: 50 }), group: WORK };
}

const ids = (accounts: readonly { readonly account: AccountSnapshot }[]): string[] =>
  accounts.map((entry) => entry.account.account_id);

describe("the person's own order", () => {
  it("lists the arranged accounts first and a new one after them", () => {
    const accounts = [left("a", 1, 50), left("b", 2, 50), left("new", 3, 50)];
    expect(arrangedOrder(accounts, ["b", "gone", "a"])).toEqual(["b", "a", "new"]);
  });

  it("keeps a group's keys together where its first key falls", () => {
    const accounts = [key("k1", 1), left("a", 2, 50), key("k2", 3)];
    expect(arrangedOrder(accounts, ["k1", "a", "k2"])).toEqual(["k1", "k2", "a"]);
  });

  it("never moves an account for its reading, and puts accounts that are off last", () => {
    const low = left("low", 2, 5);
    const off = { ...left("off", 3, 50), monitoring_enabled: false };
    const accounts = [left("full", 1, 90), low, off];
    const arranged = preferences({
      account_sort: "manual",
      account_order: ["off", "full", "low"],
    });
    expect(ids(placeAccounts(accounts, arranged))).toEqual(["full", "low", "off"]);
    // The default still puts the least remaining first.
    expect(ids(placeAccounts(accounts))).toEqual(["low", "full", "off"]);
  });

  it("moves an account past its neighbour, and a group past it as one", () => {
    const accounts = [left("a", 1, 50), key("k1", 2), key("k2", 3), left("b", 4, 50)];
    const order = ["a", "k1", "k2", "b"];
    expect(moveInOrder(accounts, order, "b", -1)).toEqual(["a", "b", "k1", "k2"]);
    expect(moveInOrder(accounts, order, "a", 1)).toEqual(["k1", "k2", "a", "b"]);
    // At the group's top edge the whole group moves up.
    expect(moveInOrder(accounts, order, "k1", -1)).toEqual(["k1", "k2", "a", "b"]);
  });

  it("moves a key among its group's keys", () => {
    const accounts = [key("k1", 1), key("k2", 2), key("k3", 3)];
    expect(moveInOrder(accounts, ["k1", "k2", "k3"], "k3", -1)).toEqual([
      "k1",
      "k3",
      "k2",
    ]);
  });

  it("cannot move past either end", () => {
    const accounts = [left("a", 1, 50), left("b", 2, 50)];
    expect(moveInOrder(accounts, ["a", "b"], "a", -1)).toBeNull();
    expect(moveInOrder(accounts, ["a", "b"], "b", 1)).toBeNull();
  });

  it("starts from the order on screen the first time it is chosen", () => {
    const accounts = [left("full", 1, 90), left("low", 2, 5)];
    const next = withAccountSort(preferences(), "manual", accounts);
    expect(next.account_order).toEqual(canonicalOrder(accounts, preferences()));
    expect(next.account_order).toEqual(["low", "full"]);
  });

  it("is the widget's order too", () => {
    const accounts = [left("full", 1, 90), left("low", 2, 5)];
    const arranged = preferences({
      account_sort: "manual",
      account_order: ["full", "low"],
    });
    expect(widgetAccounts(accounts, arranged, NOW).map((tile) => tile.id)).toEqual([
      "full",
      "low",
    ]);
  });
});

describe("by provider", () => {
  it("lists accounts by provider name, then in the order they were added", () => {
    const accounts = [
      account("o", "openrouter", 1, [], { rank: 50 }),
      account("c2", "codex", 3, [], { rank: 10 }),
      account("c1", "codex", 2, [], { rank: 90 }),
    ];
    const byProvider = preferences({ account_sort: "provider" });
    expect(ids(placeAccounts(accounts, byProvider))).toEqual(["c1", "c2", "o"]);
  });
});
