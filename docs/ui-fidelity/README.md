# UI fidelity pairs

These recorded captures compare real Quota screens with the approved wireframe
(`quota-owner-test-1/wireframe.html`, Quota wireframe v0.3) in the same state.

| Wireframe                      | App                                    | State                                 |
| ------------------------------ | -------------------------------------- | ------------------------------------- |
| `wireframe-overview-dark.png`  | `app-overview-dark.png`                | All accounts, dark, 810 px window     |
| `wireframe-overview-light.png` | `app-overview-light.png`               | All accounts, light, 810 px window    |
| —                              | `app-details-light.png`                | Account details                       |
| —                              | `app-settings-general-light.png`       | Settings, General section             |
| —                              | `app-settings-accounts-light.png`      | Settings, Accounts section            |
| —                              | `app-settings-notifications-light.png` | Settings, Notifications section       |
| —                              | `app-settings-notifications-dark.png`  | The same section in dark              |
| —                              | `app-settings-privacy-light.png`       | Settings, Privacy section             |
| —                              | `app-wizard-connect-light.png`         | Provider → Connect → Verify, step one |

The wireframe images are the wireframe's own 810 px popover rendered at the desktop window
width, with the prototype's guide above it. The app images are the real renderer at the
same width, driven by one deterministic snapshot with the ten sample accounts the
wireframe draws. The settings images are the settings window at its own 780 px width. The
Accounts capture predates the verified-identity row restoration and remains historical
evidence rather than a capture of the submitted head.

The approved controls remain visible: mode and topmost remain separate, hide uses the
authorized native close handler that keeps the overview alive, and every Add account entry
opens Provider → Connect → Verify in the settings window. Details retain Settings,
Provider usage page, and Manage accounts. The overview explains ordering, clears hidden
searches, and counts fully visible rows over the filtered rows.

Every settings destination carries a fresh route id, so Manage accounts from the overview
and the Accounts section from the settings nav both open the account management list
rather than reusing a retained panel left on one account's details.

The connection wizard uses `begin_connection`, `confirm_connection`, `cancel_connection`,
and the existing attempt events; it never simulates verification. Each explicit Add
account request opens a fresh connection route, including when settings was hidden on a
completed attempt. A successful attempt verifies the provider identity and one quota
reading and reports them as a pending candidate instead of saving anything, so the Verify
step shows Account, Workspace, and Quota reading and asks the person to confirm the
identity before adding this subscription to their overview. Nothing is written and no
monitoring starts until Add account; Cancel discards the candidate, and a restart or an
abandoned attempt leaves no account behind. The Hide account labels preference hides a
pending identity the same way it hides a saved one. Provider sign-in and connection
adapter implementation belong to the connection task.

## Known gaps

Notifications now carry three independent alert switches, so 20%, 10%, and 0% are separate
choices and deselecting one never writes a null percentage. Quiet hours are UTC.
Notification preview stays inside the renderer because no host preview command exists.

Local history retention is the approved selector. This build has no retention sweep, so
only **Disabled** and **Keep indefinitely** are real choices; the mockup's 7 days and 30
days periods are shown but refused, and the current state is named honestly with an option
the mockup does not list. This follows the owner's R18 decision to preserve default
retention behavior and disable unsupported durations when a small host extension is
insufficient. Adding a sweep is persistence work, not a visual change.

General includes Window width, Move with keys, Launch at login, Pause monitoring, and
Background refresh. Use wide view runs the existing Fit command. Try narrow view and Move
with keys are disabled: the host has no narrow-width or keyboard movement commands. Launch
at login is disabled: the host has no login registration command or confirmed enabled
flag. These controls state the exact missing capabilities and have no substitute effects
on position or startup behavior. The separate Reset position control remains available.

Pause monitoring and the overview Resume action share `set_monitoring_state`. Background
refresh saves each provider's existing polling strategy via `set_polling_preferences`,
preserving floors, adaptive bounds, boundary verification, backoff, and read budgets.
Fixed and boundary-aware schedules update the active and background intervals together
because the current supervisor uses the active interval for all ordinary reads; there is
no distinct hidden-window schedule yet.

Fit from overview or settings uses one native transition: floating chrome and confirmed
mode, fitted geometry, and an overview reset that clears filter/search and returns from
details. Mode selection, Fit, Reset, and topmost publication hold the shared window
controller lock through their complete transitions, including persistence. Viewport counts
also observe the overview content so opening or closing ordering help updates the fully
visible row count.
