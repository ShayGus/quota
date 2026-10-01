# quota-domain

The pure domain model for Quota: validated identities, quota windows,
measurements, and the least-remaining-first ranking rule.

This crate has no Tauri, React, Tokio, network, or database dependency and
denies unsafe code. It is the only place where quota meaning is decided; every
other crate maps into these types rather than reinterpreting them.

## What lives here

- `ids` — opaque, validated identifier newtypes. Deserialization goes through
  the same constructor, so a decoded value cannot bypass the invariant.
- `percent` — a finite percentage that keeps its original evidence. Overspend
  above 100% used survives; only the visible arc is clamped.
- `quota` — units, scopes, measurements, windows, and structured validation
  issues. Exhausted, not entitled, unlimited, and unknown are distinct states.
- `ranking` — the least-remaining-first order, with an explicit reason whenever
  an account has no comparable value.
- `account`, `snapshot`, `polling` — connections, the published snapshot, and
  typed polling policies.

## Rules this crate enforces

- Independent windows and independent providers are never added or averaged.
- A count without a denominator stays in its native unit and never acquires an
  invented percentage.
- A stale reading, an expired boundary, or a missing window never becomes a
  current rank.
- Extra-spend caps and credit balances never take part in the ranking.
