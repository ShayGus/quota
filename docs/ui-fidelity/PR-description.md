The Quota renderer now follows the approved wireframe across the overview, account
details, settings, and Provider → Connect → Verify wizard. It restores the required
controls, search and ordering behavior, semantic quota labels, viewport counts, and the
complete Fit transition. Repeated Add requests start fresh wizards, and native window
transitions are serialized through persistence and publication.

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
