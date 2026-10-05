/**
 * The pictures at the top of the README: the overview with a few accounts, in
 * both themes and both layouts. The other user-guide pictures are taken by the
 * tests of the surface they show. `bun run docs:screenshots` copies them into
 * `docs/images/`.
 */
import { expect, test } from "./harness";
import { defaultPreferences, heroAccounts, scenario } from "./scenarios";

/** A Codex account running low, a Claude account with room left, an OpenRouter balance. */
const HERO_ACCOUNTS = heroAccounts();

for (const theme of ["dark", "light"] as const) {
  for (const layout of ["ring", "bar"] as const) {
    test(`README overview, ${layout}s, ${theme}`, async ({ open }) => {
      const host = await open(
        scenario("overview", {
          accounts: HERO_ACCOUNTS,
          preferences: defaultPreferences({ theme, indicator_style: layout }),
        }),
      );
      await expect(host.page.getByRole("article")).toHaveCount(3);
      await host.screenshotFull(`hero-${layout}-${theme}`);
    });
  }
}
