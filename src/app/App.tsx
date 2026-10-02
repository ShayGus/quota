/**
 * Quota's application root.
 *
 * The renderer owns presentation state only: the current view. Every account
 * value comes from the snapshot store, and every mutation is a typed command
 * (spec 7.8.3).
 */
import { useLayoutEffect, useRef, useState, type JSX } from "react";

import { AccountDetail } from "../features/accounts/AccountDetail";
import { Overview } from "../features/overview/Overview";
import type { OverviewFilter } from "../features/overview/OverviewToolbar";
import { Settings, type SettingsActions } from "../features/settings/Settings";
import type { AccountId } from "../generated/bindings";
import { launch } from "../shared/ipc/report";
import { useRendererState } from "../shared/state/useRendererState";
import { useNow } from "../shared/ui/useNow";
import { actions } from "./actions";
import { Icon } from "../shared/ui/Icon";
import { AppHeader } from "./AppHeader";
import { AppBoundary, FeatureBoundary } from "./ErrorBoundary";
import { useSnapshotSubscription } from "./useSnapshotSubscription";
import { useTheme } from "./useTheme";
import { displayName } from "../shared/format/alias";

/**
 * Which surface the overview window is showing.
 *
 * Settings has no entry here: it belongs to its own window, whose capability is
 * the only one that may save preferences. The overview asks the host to show it.
 */
type View =
  { readonly name: "overview" } | { readonly name: "detail"; readonly id: AccountId };

/** The settings actions, wired to the typed commands. */
const settingsActions: SettingsActions = {
  savePreferences: (next) => {
    launch(actions.savePreferences(next));
  },
  setAlwaysOnTop: (alwaysOnTop) => {
    launch(actions.setAlwaysOnTop(alwaysOnTop));
  },
  setOverviewMode: (mode) => {
    launch(actions.setOverviewMode(mode));
  },
  fitToAccounts: () => {
    launch(actions.fitToAccounts());
  },
  resetPosition: () => {
    launch(actions.resetPosition());
  },
  setAccountEnabled: (accountId, enabled) => {
    launch(actions.setAccountEnabled(accountId, enabled));
  },
  renameAccount: (accountId, nickname) => {
    launch(actions.renameAccount(accountId, nickname));
  },
  disconnectAccount: (accountId) => {
    launch(actions.disconnectAccount(accountId));
  },
  openUsagePage: (accountId) => {
    launch(actions.openUsagePage(accountId));
  },
  beginConnection: async (request) => {
    const accepted = await actions.beginConnection(request);
    return accepted === null ? null : { id: accepted.attempt_id };
  },
  cancelConnection: (attempt) => actions.cancelConnection(attempt),
