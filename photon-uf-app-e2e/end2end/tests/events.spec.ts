import type { Page } from "@playwright/test";
import { test, expect, seedAuth, waitForHydrated } from "./fixtures";

test.describe("e2e.events", () => {
  test("e2e.events.index_happy", async ({ page }) => {
    const seeded = await seedAuth(page, "admin");
    await page.goto("/photon/events", { waitUntil: "domcontentloaded" });
    await waitForHydrated(page);
    const events = page.getByTestId("photon-events");
    await expect(events).toBeVisible({ timeout: 60_000 });
    // Topic also appears as a hidden <option>; assert the table cell instead.
    await expect(
      events.getByRole("cell", { name: seeded.fixtures.topic_name }),
    ).toBeVisible({ timeout: 60_000 });
  });

  test("e2e.events.detail_happy", async ({ page }) => {
    const seeded = await seedAuth(page, "admin");
    await page.goto(
      `/photon/events/${encodeURIComponent(seeded.fixtures.event_id)}`,
      { waitUntil: "domcontentloaded" },
    );
    await waitForHydrated(page);
    await expect(page.getByTestId("photon-event-detail")).toBeVisible({ timeout: 60_000 });
    await expect(
      page.getByTestId("photon-event-detail").getByText(seeded.fixtures.topic_name).first(),
    ).toBeVisible();
  });

  test("e2e.events.not_found_sad", async ({ page }) => {
    await seedAuth(page, "admin");
    await page.goto("/photon/events/__photon_e2e_no_such_event__", {
      waitUntil: "domcontentloaded",
    });
    await waitForHydrated(page);
    await expect(page.getByTestId("photon-event-detail")).toBeVisible({ timeout: 60_000 });
    await expect(page.getByText("Event not found")).toBeVisible({ timeout: 60_000 });
  });
});

// Pages reached by client navigation must call their own server functions instead of
// reusing resource values the server rendered for the dashboard.
test.describe("e2e.events.client_nav", () => {
  async function openDashboard(page: Page) {
    const seeded = await seedAuth(page, "admin");
    await page.goto("/photon", { waitUntil: "domcontentloaded" });
    await waitForHydrated(page);
    await expect(page.getByTestId("photon-dashboard")).toBeVisible({ timeout: 60_000 });
    return seeded;
  }

  test("e2e.events.client_nav_index_fetches_events", async ({ page }) => {
    await openDashboard(page);
    const getEvents = page.waitForRequest((req) => /get_events/i.test(req.url()), {
      timeout: 20_000,
    });
    await page.getByTestId("photon-dashboard").getByRole("button", { name: /View All/ }).first().click();
    await expect(page).toHaveURL(/\/photon\/events$/);
    await getEvents;
    await expect(page.getByTestId("photon-events")).toBeVisible({ timeout: 60_000 });
  });

  test("e2e.events.client_nav_subscription_fetches_events", async ({ page }) => {
    const seeded = await openDashboard(page);
    const href = `/photon/subscriptions/${encodeURIComponent(seeded.fixtures.subscription_id)}`;
    const getEvents = page.waitForRequest((req) => /get_waiting_events/i.test(req.url()), {
      timeout: 20_000,
    });
    await page.getByTestId("photon-dashboard").locator(`a[href="${href}"]`).first().click();
    await expect(page.getByTestId("photon-subscription-detail")).toBeVisible({ timeout: 60_000 });
    await getEvents;
  });
});
