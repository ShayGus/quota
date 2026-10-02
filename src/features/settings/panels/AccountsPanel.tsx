/**
 * Account management settings.
 *
 * Separate identities, including several subscriptions from one provider. The
 * provider mark is text, so a provider label can never inject markup (AC-37).
 * Disconnecting one account leaves its same-provider siblings untouched
 * (spec 4.6, AC-40).
 */
import { useState, type JSX } from "react";

      {accounts.length === 0 ? (
        <p className="note">
          No connected accounts. Add your first account to start monitoring.
        </p>
      ) : null}
