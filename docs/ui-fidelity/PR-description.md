The Quota renderer now follows the approved wireframe across the overview, account
details, settings, and Provider → Connect → Verify wizard. It restores the required
controls, search and ordering behavior, semantic quota labels, viewport counts, and the
complete Fit transition. Repeated Add requests start fresh wizards, and native window
transitions are serialized through persistence and publication. Management cards show
verified account identities; Hide account labels replaces those identities and workspaces
while preserving public allowance labels in Details.

## Known gaps

The [fidelity reference](README.md#known-gaps) owns the limitations and approval context.

- Confirm-before-adding: the owner's R13 deferral to the connection follow-up.
- Try narrow view, Move with keys, and Launch at login: retained host capability limits.
- Timed history retention: the owner's R18 fallback for unsupported durations.
- Quiet hours and native notification preview: retained notification capability limits.
- A distinct hidden-window refresh schedule: retained polling contract.
