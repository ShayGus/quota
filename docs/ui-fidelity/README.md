# UI fidelity pairs

These pairs compare the real Quota screens with the approved wireframe
(`quota-owner-test-1/wireframe.html`, Quota wireframe v0.3) in the same state.

| Wireframe                      | App                                             | State                                          |
| ------------------------------ | ----------------------------------------------- | ---------------------------------------------- |
| `wireframe-overview-dark.png`  | `app-overview-dark.png`                         | All accounts, dark, 810 px window              |
| `wireframe-overview-light.png` | `app-overview-light.png`                        | All accounts, light, 810 px window             |
| —                              | `app-overview-tray-width-light.png`             | The same list at the 440 px tray popover width |
| —                              | `app-details-light.png`, `app-details-dark.png` | Account details                                |
| —                              | `app-settings-light.png`                        | Settings, Window section                       |

The wireframe images are the wireframe's own 810 px popover rendered at the desktop window
width, with the prototype's guide above it. The app images are the real renderer at the
same width, driven by one deterministic snapshot with the ten sample accounts the
wireframe draws.

The approved controls are wired to the host: mode and topmost remain separate, hide uses
the authorized native close handler that keeps the overview alive, and every Add account
entry opens Provider → Connect → Verify in the settings window. Details retain Settings,
Provider usage page, and Manage accounts. The overview explains ordering, clears hidden
searches, and counts fully visible rows over the filtered rows.

The connection wizard uses `begin_connection`, `cancel_connection`, and the existing
attempt events; it never simulates verification. The host commits the verified account
before publishing `verified`, so there is no separate confirm-and-add command. Attempt
events contain connection state but no account ID or verified workspace, which prevents an
exact account identity confirmation in the wizard. The requested nickname is sent before
connection; the verified identity is available in Accounts after publication. Provider
sign-in and connection adapter implementation belong to the connection task.

Notifications use the existing policy. It has one critical threshold, so 10% and 0% are
mutually exclusive. Quiet hours are UTC. Notification preview stays inside the renderer
because no host preview command exists. The committed screenshots show the initial
fidelity implementation, before review fixes; they are historical evidence rather than
captures of the updated settings and wizard.

General includes Window width, Move with keys, Launch at login, Pause monitoring, and
Background refresh. The host exposes Fit and Reset but no narrow-width or keyboard
movement command: Use wide view fits all accounts; Try narrow view and Move with keys
reset the floating window for manual resizing or dragging. `launch_behavior` controls
quiet tray startup versus restoring the last mode; there is no login registration command
or stored enabled flag. The Launch at login switch saves that nearest startup preference
and states the limitation beside the control.

Pause monitoring and the overview Resume action share `set_monitoring_state`. Background
refresh saves each provider's existing polling strategy via `set_polling_preferences`,
preserving floors, adaptive bounds, boundary verification, backoff, and read budgets.
Fixed and boundary-aware schedules update the active and background intervals together
because the current supervisor uses the active interval for all ordinary reads; there is
no distinct hidden-window schedule yet.

Fit from overview or settings uses one native transition: floating chrome and confirmed
mode, fitted geometry, and an overview reset that clears filter/search and returns from
details. Viewport counts also observe the overview content so opening or closing ordering
help updates the fully visible row count.
