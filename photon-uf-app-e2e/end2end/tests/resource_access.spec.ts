import { test, expect, seedAuth, waitForHydrated, type Page } from "./fixtures";

async function open(page: Page, path: string) {
  await page.goto(path, { waitUntil: "domcontentloaded" });
  await waitForHydrated(page);
}

const topicPath = (name: string) => `/photon/topics/${encodeURIComponent(name)}`;
const subPath = (id: string) => `/photon/subscriptions/${encodeURIComponent(id)}`;
const eventPath = (id: string) => `/photon/events/${encodeURIComponent(id)}`;

/** Orbital components put their test id on an injected `<style>` too; skip that copy. */
const byTestId = (page: Page, id: string) =>
  page.locator(`[data-testid="${id}"]:not(style)`);

test.describe("e2e.access", () => {
  test("e2e.access.admin_owns_every_resource", async ({ page }) => {
    const seeded = await seedAuth(page, "admin");
    await open(page, "/photon/topics");
    const topic = page.getByTestId(`topic-${seeded.fixtures.topic_name}`);
    await expect(topic.getByTestId("photon-your-access")).toHaveText("Owner", {
      timeout: 60_000,
    });

    // Every subscription row shows its health, not "No access".
    await open(page, "/photon/subscriptions");
    await expect(byTestId(page, "photon-health-badge").first()).toBeVisible({
      timeout: 60_000,
    });
    await expect(page.getByTestId("photon-subscriptions-no-access")).toHaveCount(0);
  });

  test("e2e.access.access_tab_shows_default_owner", async ({ page }) => {
    const seeded = await seedAuth(page, "admin");
    for (const path of [
      topicPath(seeded.fixtures.topic_name),
      subPath(seeded.fixtures.subscription_id),
    ]) {
      await open(page, path);
      await expect(page.getByTestId("photon-access-panel")).toBeHidden({ timeout: 60_000 });
      await page.getByRole("tab", { name: "Access" }).click();
      await expect(byTestId(page, "access-card")).toBeVisible({ timeout: 60_000 });
      await expect(byTestId(page, "access-card-error")).toHaveCount(0);
      await expect(byTestId(page, "access-card-rows")).toContainText("Photon admins", {
        timeout: 60_000,
      });
      // Switching back keeps the overview mounted: no skeleton, no reload.
      await page.getByRole("tab", { name: "Overview" }).click();
      await expect(page.getByTestId("photon-overview-panel")).toBeVisible();
      await expect(page.getByTestId("access-denied")).toHaveCount(0);
    }
  });

  test("e2e.access.viewer_inspects_events", async ({ page }) => {
    const seeded = await seedAuth(page, "viewer");
    await open(page, "/photon/topics");
    const topic = page.getByTestId(`topic-${seeded.fixtures.topic_name}`);
    await expect(topic.getByTestId("photon-your-access")).toHaveText("Can view", {
      timeout: 60_000,
    });

    await open(page, topicPath(seeded.fixtures.topic_name));
    await expect(page.getByText("Events (24h):").first()).toBeVisible({ timeout: 60_000 });
    await expect(page.getByTestId("access-denied")).toHaveCount(0);

    await open(page, eventPath(seeded.fixtures.event_id));
    await expect(page.getByText("Payload", { exact: true })).toBeVisible({ timeout: 60_000 });
    await expect(page.getByTestId("access-denied")).toHaveCount(0);

    await open(page, subPath(seeded.fixtures.subscription_id));
    await expect(page.getByTestId("photon-subscription-meta")).toBeVisible({
      timeout: 60_000,
    });
    await expect(page.getByTestId("access-denied")).toHaveCount(0);
  });

  test("e2e.access.outsider_denied_inspect", async ({ page }) => {
    const seeded = await seedAuth(page, "outsider");
    await open(page, "/photon/topics");
    const topic = page.getByTestId(`topic-${seeded.fixtures.topic_name}`);
    await expect(topic.getByTestId("photon-your-access")).toHaveText("No access", {
      timeout: 60_000,
    });
    await expect(topic.getByText("Schema:")).toHaveCount(0);

    await open(page, topicPath(seeded.fixtures.topic_name));
    const denied = page.getByTestId("access-denied");
    await expect(denied).toBeVisible({ timeout: 60_000 });
    await expect(denied).toHaveAttribute("data-action", "View");
    await expect(page.getByTestId("access-denied-message")).toContainText(
      `You need View on ${seeded.fixtures.topic_name}`,
    );
    await expect(page.getByText("Events (24h):")).toHaveCount(0);

    await open(page, subPath(seeded.fixtures.subscription_id));
    await expect(page.getByTestId("access-denied").first()).toBeVisible({ timeout: 60_000 });
    await expect(page.getByTestId("photon-subscription-meta")).toHaveCount(0);

    await open(page, eventPath(seeded.fixtures.event_id));
    await expect(page.getByTestId("access-denied")).toBeVisible({ timeout: 60_000 });
    // The denied message says "payload" too, so match the card heading exactly.
    await expect(page.getByText("Payload", { exact: true })).toHaveCount(0);
  });

  test("e2e.access.outsider_requests_access", async ({ page }) => {
    const seeded = await seedAuth(page, "outsider");
    await open(page, subPath(seeded.fixtures.subscription_id));
    const denied = page
      .getByTestId("access-denied")
      .filter({ hasText: seeded.fixtures.subscription_id });
    await expect(denied).toBeVisible({ timeout: 60_000 });
    await denied.getByTestId("access-denied-request").click();
    const fields = page.getByTestId("request-access-fields");
    await expect(fields).toBeVisible();
    await fields.getByRole("textbox").last().fill("On call for this subscription this week");
    await page.getByTestId("request-access-send").click();
    await expect(denied.getByTestId("access-denied-sent")).toBeVisible({ timeout: 60_000 });
  });
});
