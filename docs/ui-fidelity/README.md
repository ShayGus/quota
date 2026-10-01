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

Two controls the wireframe draws have no counterpart in the app, because the app has no
command for them: the header's close button (the native close handler hides the window to
the tray, and the overview webview is not allowed to drive it) and the footer's
`+ Add account` (the add-account flow lives in the settings window, which the first-launch
empty state opens).
