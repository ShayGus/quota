// The parity cases: what to open in the wireframe and in the app, and which
// shared elements to compare. `ignore` lists properties that differ only by
// data (which account is shown, its values) rather than by design, and
// `ignoreAt` does the same for one selector.

const click = (selector) => `(() => {
  const node = document.querySelector(${JSON.stringify(selector)});
  node?.click();
  return node !== null;
})()`;

const POPOVER = [
  ".app-header",
  ".app-brand",
  ".app-brand .logo",
  ".app-brand h1",
  ".app-brand p",
  ".app-actions .icon-btn",
  ".toolbar",
  ".filter-set",
  ".filter-set button",
  ".filter-set button.selected",
  ".filter-set .count",
  ".layout-set .icon-btn",
  ".layout-set .icon-btn.active",
  ".overview-label",
  ".cards",
  ".provider-card",
  ".provider-head",
  ".provider-name",
  ".provider-meta",
  ".badge",
  ".badge .icon",
  ".quota-grid",
  ".quota-button",
  ".quota-label",
  ".ring",
  ".ring-track",
  ".ring-arc",
  ".ring-value",
  ".ring-value span",
  ".ring-caption",
  ".reset-label",
  ".reset-label strong",
  ".card-foot",
  ".card-foot button",
  ".card-foot .icon",
  ".app-footer",
  ".footer-state",
  ".footer-state .icon",
  ".app-footer .text-btn",
  ".app-footer .text-btn .icon",
];

const DETAIL = [
  ".back-row",
  ".back-button",
  ".back-button .icon",
  ".back-row .eyebrow",
  ".detail-body",
  ".detail-identity",
  ".detail-identity .provider-name",
  ".detail-identity .provider-meta",
  ".tabs",
  ".tabs button",
  ".tabs button.selected",
  ".detail-summary",
  ".detail-summary .ring",
  ".detail-summary .ring-value",
  ".detail-summary .ring-value span",
  ".detail-summary .ring-caption",
  ".detail-time .eyebrow",
  ".detail-time strong",
  ".detail-time p",
  ".detail-list",
  ".detail-list > div",
  ".detail-list dt",
  ".detail-list dd",
  ".detail-list dd .muted",
  ".section-title",
  ".other-limit",
  ".other-limit span",
  ".detail-body .note",
  ".detail-bottom",
  ".detail-bottom .text-btn",
];

const WIZARD = [
  ".back-row",
  ".back-button",
  ".back-row .badge",
  ".wizard",
  ".step-line",
  ".step-line .step",
  ".step-line .step.selected",
  ".step-line .step-rule",
  ".wizard h2",
  ".provider-pick",
  ".provider-pick .provider-icon",
  ".provider-pick .provider-name",
  ".provider-pick .provider-meta",
  ".provider-pick > .icon",
  ".wizard .note",
];

const CONNECT = [
  ".wizard h2",
  ".connection-box",
  ".connection-box .provider-icon",
  ".connection-box .provider-name",
  ".connection-box .provider-meta",
  ".connection-box h3",
  ".connection-box p",
  ".wizard .note",
  ".wizard-action",
  ".wizard-action .button",
  ".wizard-action .button.primary",
  ".wizard-action .button.primary .icon",
];

const SETTINGS_FRAME = [
  ".settings-head",
  ".settings-head h2",
  ".settings-head p",
  ".settings-head .icon-btn",
  ".settings-nav",
  ".settings-nav button",
  ".settings-nav button.selected",
  ".settings-nav button .icon",
  ".settings-nav .nav-foot",
  ".settings-content",
  ".settings-title-row",
  ".settings-content h3",
  ".settings-intro",
];

const SETTINGS = {
  general: [
    ...SETTINGS_FRAME,
    ".setting-row",
    ".setting-row .setting-label",
    ".setting-row p",
    ".setting-row select",
    ".settings-content > .note",
  ],
  accounts: [
    ...SETTINGS_FRAME,
    ".settings-title-row .button.primary",
    ".account-manage-card",
    ".account-manage-head",
    ".account-manage-head .provider-name",
    ".account-manage-head .switch",
    ".account-manage-actions",
    ".account-manage-actions .text-btn",
    ".account-manage-actions .danger",
    ".account-manage-actions .icon-btn",
  ],
  appearance: [
    ...SETTINGS_FRAME,
    ".theme-options",
    ".theme-option",
    ".theme-thumbnail",
    ".theme-thumbnail span",
    ".theme-option b",
    ".setting-row .tabs",
    ".setting-row .tabs button",
    ".setting-row .tabs button.selected",
  ],
  notifications: [
    ...SETTINGS_FRAME,
    ".setting-group-label",
    ".thresholds",
    ".thresholds label",
    ".thresholds input",
  ],
  privacy: [
    ...SETTINGS_FRAME,
    ".privacy-card",
    ".privacy-card .icon",
    ".privacy-card strong",
    ".privacy-card p",
    ".setting-row select",
    ".setting-row .button.danger",
  ],
  diagnostics: [
    ...SETTINGS_FRAME,
    ".detail-list",
    ".detail-list > div",
    ".detail-list dt",
    ".detail-list dd",
    ".section-title",
    ".diagnostic-log",
  ],
};

/**
 * Run before the cases: a floating overview keeps whatever width it was last
 * resized to, so it is put back to the popover's 440 pixels first.
 */
export const START = [
  {
    tool: "manage_window",
    args: { action: "set_size", window_label: "overview", width: 440, height: 600 },
  },
  // A freshly launched app is still settling its first layout.
  { wait: 3000 },
];

/** Every case. The app is expected to start on the overview, dark theme. */
export const CASES = [
  {
    name: "overview",
    selectors: POPOVER,
    wireframe: { query: "theme=dark" },
    app: { setup: [], teardown: [] },
    scope: ".popover",
    ignore: ["text", "w"],
    // The list holds as many cards as there are accounts.
    ignoreAt: { ".cards": ["h"] },
  },
  {
    name: "detail",
    selectors: DETAIL,
    wireframe: {
      query: "theme=dark",
      clicks: ["[data-action=window-detail][data-account=claude]"],
    },
    app: { setup: [click(".quota-button")], teardown: [click(".back-button")] },
    scope: ".popover",
    // The sample's Claude is at 18% (amber); a live account's colour follows its value.
    ignore: ["text", "w", "color", "background-color", "border-top-color"],
  },
  {
    name: "wizard-provider",
    selectors: WIZARD,
    wireframe: { query: "theme=dark", clicks: [".app-footer [data-action=add-account]"] },
    app: { setup: [click(".app-footer .text-btn")], teardown: [click(".back-button")] },
    scope: ".popover",
    ignore: ["text", "w"],
    // The chevron is pushed right by an automatic margin, which grows with the
    // width ignored above: the native window draws the wireframe's 1px border.
    ignoreAt: { ".provider-pick > .icon": ["margin"] },
  },
  {
    name: "wizard-connect",
    selectors: CONNECT,
    wireframe: {
      query: "theme=dark",
      clicks: [".app-footer [data-action=add-account]", "[data-provider=claude]"],
    },
    app: {
      setup: [
        click(".app-footer .text-btn"),
        `(() => { const b = [...document.querySelectorAll(".provider-pick")].find((n) => n.textContent.includes("Claude")); b?.click(); return b !== undefined; })()`,
      ],
      teardown: [click(".back-button")],
    },
    scope: ".popover",
    // The intro sentence is product wording, not the prototype's, so its height differs.
    ignore: ["text", "w", "h"],
  },
  ...["general", "accounts", "appearance", "notifications", "privacy", "diagnostics"].map(
    (section) => ({
      name: `settings-${section}`,
      selectors: SETTINGS[section],
      wireframe: {
        query: "theme=dark",
        clicks: ["[data-action=settings]", `[data-tab=${section}]`],
      },
      app: {
        // The settings window has no DOM tools by design, so its route is shown in
        // the overview webview at the settings window's size, then put back.
        setup: [
          {
            tool: "manage_window",
            args: {
              action: "set_size",
              window_label: "overview",
              width: 780,
              height: 600,
            },
          },
          `(() => { location.hash = "#/settings/${section}/parity"; location.reload(); return true; })()`,
          { wait: 3500 },
        ],
        teardown: [
          `(() => { location.hash = ""; location.reload(); return true; })()`,
          { wait: 3500 },
          {
            tool: "manage_window",
            args: {
              action: "set_size",
              window_label: "overview",
              width: 440,
              height: 400,
            },
          },
        ],
      },
      scope: ".settings-window",
      ignore: ["text", "w", "h", "margin"],
    }),
  ),
];
