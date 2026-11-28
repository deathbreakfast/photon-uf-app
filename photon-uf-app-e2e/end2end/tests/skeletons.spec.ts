import type { Page } from "@playwright/test";
import { test, expect, seedAuth, waitForHydrated } from "./fixtures";

/**
 * Hold every request to a server fn until `release()` is called, so the page's
 * loading state stays on screen long enough to assert on. Server fn URLs are
 * `/api/<fn name>` plus an optional hash suffix.
 */
async function holdServerFn(page: Page, url: RegExp) {
  let release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route(url, async (route) => {
    await gate;
    await route.continue();
  });
  return release;
}

/** Load the dashboard and mark the window, so a full reload would be detectable. */
async function openDashboard(page: Page) {
  await page.goto("/photon", { waitUntil: "domcontentloaded" });
  await waitForHydrated(page);
  const dash = page.getByTestId("photon-dashboard");
  await expect(dash).toBeVisible({ timeout: 60_000 });
  await page.evaluate(() => {
    (window as unknown as { __photonShell?: boolean }).__photonShell = true;
  });
  return dash;
}

async function expectShellKept(page: Page) {
  expect(
    await page.evaluate(
      () => (window as unknown as { __photonShell?: boolean }).__photonShell === true,
    ),
  ).toBe(true);
}

test.describe("e2e.skeletons", () => {
  test("e2e.skeletons.subscriptions_list", async ({ page }) => {
    const seeded = await seedAuth(page, "admin");
    const dash = await openDashboard(page);
    const release = await holdServerFn(page, /\/api\/get_subscriptions[^/?]*(\?|$)/);

    // Second "View All" is Active Subscriptions.
    await dash.getByRole("button", { name: /View All/ }).nth(1).click();
    const subs = page.getByTestId("photon-subscriptions");
    await expect(subs).toBeVisible({ timeout: 30_000 });
    await expect(subs.getByTestId("photon-list-skeleton")).toBeVisible();

    release();
    await expect(subs.getByTestId("photon-list-skeleton")).toHaveCount(0, { timeout: 30_000 });
    await expect(subs.getByText(seeded.fixtures.topic_name).first()).toBeVisible();
    await expectShellKept(page);
  });

  test("e2e.skeletons.subscription_detail", async ({ page }) => {
    const seeded = await seedAuth(page, "admin");
    const subId = seeded.fixtures.subscription_id;
    const dash = await openDashboard(page);
    const release = await holdServerFn(page, /\/api\/get_subscription(?!s)[^/?]*(\?|$)/);

    await dash
      .locator(`a[href$="/subscriptions/${encodeURIComponent(subId)}"]`)
      .first()
      .click();
    const detail = page.getByTestId("photon-subscription-detail");
    await expect(detail).toBeVisible({ timeout: 30_000 });

    // Header and labels render at once; only the values are placeholders.
    const metaSkeleton = detail.getByTestId("photon-subscription-meta-skeleton");
    await expect(metaSkeleton).toBeVisible();
    await expect(metaSkeleton.getByText("Details")).toBeVisible();
    await expect(metaSkeleton.getByText("Checkpoint lag")).toBeVisible();
    // The events card header renders while the subscription loads. (Its table skeleton
    // isn't asserted: after client navigation the events table can resolve without a
    // client fetch, so there's nothing to hold. e2e.skeletons.events_table covers it.)
    const events = detail.getByTestId("photon-subscription-events");
    await expect(events.getByText("Recent events on this topic")).toBeVisible();
    // Until the subscription loads, the title is the id from the URL.
    await expect(detail.getByText(subId, { exact: true }).first()).toBeVisible();

    release();
    const meta = detail.getByTestId("photon-subscription-meta");
    await expect(meta).toBeVisible({ timeout: 30_000 });
    await expect(meta.getByText(subId)).toBeVisible();
    await expect(meta.getByRole("link", { name: seeded.fixtures.topic_name })).toBeVisible();
    await expectShellKept(page);
  });

  test("e2e.skeletons.events_table", async ({ page }) => {
    const seeded = await seedAuth(page, "admin");
    const dash = await openDashboard(page);
    // Hold every server fn so the events page stays on its table skeleton.
    const release = await holdServerFn(page, /\/api\//);

    // First "View All" is Recent Events.
    await dash.getByRole("button", { name: /View All/ }).first().click();
    const events = page.getByTestId("photon-events");
    await expect(events).toBeVisible({ timeout: 30_000 });
    const skeleton = events.getByTestId("photon-table-skeleton");
    await expect(skeleton).toBeVisible();
    for (const header of ["Event", "Topic", "Key", "Seq", "Published"]) {
      await expect(skeleton.getByText(header, { exact: true })).toBeVisible();
    }

    release();
    await expect(skeleton).toHaveCount(0, { timeout: 30_000 });
    await expect(
      events.getByTestId("photon-events-table").getByText(seeded.fixtures.topic_name, { exact: true }).first(),
    ).toBeVisible();
    await expectShellKept(page);
  });
});
