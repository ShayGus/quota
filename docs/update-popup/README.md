# The update pop-up, beside the settings window

The pop-up is drawn by the same renderer as every other window, from the settings window's
header and the dialog's text and buttons, in both themes. These captures come from
`tests/ui/update.spec.ts`, which also asserts that the header, the title, the subtitle,
the close button, the primary button and the window background have the same computed
style in the pop-up and in the settings window, in light and in dark.

| Capture                      | What it shows                                                |
| ---------------------------- | ------------------------------------------------------------ |
| `offer-light.png`            | An update is available: both versions, Cancel and OK. Light. |
| `offer-dark.png`             | The same, dark.                                              |
| `installing-light.png`       | After OK: the install is running and nothing can be pressed. |
| `failed-light.png`           | The install failed: the message and one Close button.        |
| `compare-light-settings.png` | The settings window, Accounts, light, for the comparison.    |
| `compare-dark-settings.png`  | The settings window, Accounts, dark.                         |

CI uploads the same files, regenerated on every run, in the interface tests' screenshots.
