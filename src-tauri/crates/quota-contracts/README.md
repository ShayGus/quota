# quota-contracts

The Rust-owned IPC surface for the Quota desktop host.

This crate contains transport definitions only. It has no business services and no
credential types. Its application-model dependency is `quota-domain`; its transport
dependencies are declared in `Cargo.toml`.

## What lives here

- `refs` — tagged identifier wrappers. A `ConnectionRef` is not assignable to an
  `AccountRef` parameter, so the wrong entity type is a compile error rather than a
  runtime mistake.
- `commands` — request and result shapes for every application command. There is no
  generic `set_state`, no JSON patch, and no raw SQL or URL argument.
- `errors` — the `CommandError` union. Recovery switches on the variant, never on the
  message text.
- `events` — payload types for the typed backend-to-renderer events.
- `preferences` — the aggregate the renderer receives, assembled from the store and
  `SQLite` owners.

## One schema, not two

Domain types cross the boundary through their own `Serde` and `Type` derives in
`quota-domain`. Nothing here restates a domain struct field by field, so a change to the
model cannot leave a stale copy behind. Only shapes that genuinely differ in transport —
tagged references, error unions, command arguments, event envelopes — are declared.
