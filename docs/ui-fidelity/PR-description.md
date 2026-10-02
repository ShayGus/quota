The Quota renderer now follows the approved wireframe across the overview, account
details, settings, and Provider → Connect → Verify wizard. It restores the required
controls, search and ordering behavior, semantic quota labels, viewport counts, and the
complete Fit transition. Repeated Add requests start fresh wizards, and native window
transitions are serialized through persistence and publication. Management cards show
verified account identities; Hide account labels replaces those identities and workspaces
while preserving public allowance labels in Details.

## Known gaps

- Identity confirmation currently happens **after the host saves the account**, rather
  than before adding it to the overview. The owner approved deferring
  confirm-before-adding to a follow-up task after
  [Quota PR 4](https://github.com/ShayGus/quota/pull/4) merges. This PR does not extend
  the host protocol. The wizard shows saved provider accounts' Account, Workspace, and
  Quota reading from snapshots; the attempt event does not identify the account it saved.
- Try narrow view, Move with keys, and Launch at login remain visible and disabled with
  precise capability notices. The host lacks narrow-width and keyboard movement commands,
  login registration, and a confirmed login-enabled flag.
- Local history retention is the approved selector, but only **Disabled** and **Keep
  indefinitely** are selectable. The host has no retention sweep, so the mockup's 7 days
  and 30 days periods are shown disabled with a precise notice. The current state is named
  by an extra option the mockup does not list, so the control never claims a period the
  host does not apply. This follows the owner's R18 decision to preserve default retention
  behavior and disable unsupported durations when a small host extension is insufficient.
- Quiet hours use UTC, and notification preview is confined to the renderer because the
  host has no preview command. These retain the existing host capability boundary
  described in [README.md](README.md); the independent threshold settings are implemented.
- Background refresh updates active and background intervals together for fixed and
  boundary-aware polling. The current supervisor uses the active interval for ordinary
  reads and has no distinct hidden-window schedule. This retains the existing polling
  contract described in [README.md](README.md).
