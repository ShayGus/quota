/**
 * One provider account with several keys: its total once, then each key.
 *
 * The header names the account and shows what belongs to the account as a
 * whole, the balance and what the keys spent together. Under it each key is
 * an ordinary account card, showing what is its own: its spend limit, status
 * and reconnect, never the account's balance a second time.
 */
import type { JSX, ReactNode } from "react";

import type { GroupSnapshot } from "../../generated/bindings";
import { groupBalanceLine, groupSpendLine } from "../../shared/format/group";
import { providerLabel } from "../../shared/format/provider";
import { ProviderMark } from "../../shared/ui/ProviderMark";

/** A group's header and, inside it, its keys' cards. */
export function GroupCard({
  group,
  name,
  keyCount,
  children,
}: {
  readonly group: GroupSnapshot;
  /** The group's name, or an alias under the privacy setting. */
  readonly name: string;
  /** How many keys the group has, shown or not. */
  readonly keyCount: number;
  /** The keys' cards. */
  readonly children: ReactNode;
}): JSX.Element {
  const provider = providerLabel(group.provider_id);
  const balance = groupBalanceLine(group);
  const spend = groupSpendLine(group);
  return (
    <section className="group-card" aria-label={`${provider} ${name} account`}>
      <header className="group-head">
        <ProviderMark providerId={group.provider_id} />
        <span className="provider-copy">
          <span className="provider-name">{name}</span>
          <span className="provider-meta">
            {provider} · {keyCount} {keyCount === 1 ? "key" : "keys"}
          </span>
        </span>
        {balance === null ? null : <strong className="group-balance">{balance}</strong>}
      </header>
      {spend === null ? null : <div className="group-spend">{spend}</div>}
      <div className="group-keys">{children}</div>
    </section>
  );
}
