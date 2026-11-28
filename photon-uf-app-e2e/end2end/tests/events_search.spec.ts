import { test, expect, seedAuth, waitForHydrated, type Page } from "./fixtures";

/** Orbital components put their test id on an injected `<style>` too; skip that copy. */
const byTestId = (page: Page, id: string) => page.locator(`[data-testid="${id}"]:not(style)`);

async function openEvents(page: Page, query = "") {
  await page.goto(`/photon/events${query}`, { waitUntil: "domcontentloaded" });
  await waitForHydrated(page);
  await expect(byTestId(page, "photon-events")).toBeVisible({ timeout: 60_000 });
}

const searchBox = (page: Page) => page.getByPlaceholder("Event id or key");
const eventLinks = (page: Page) => byTestId(page, "photon-events-table").locator('a[href^="/photon/events/"]');
const caption = (page: Page) => byTestId(page, "photon-events-caption");

function utcDay(offsetDays: number) {
  const d = new Date(Date.now() + offsetDays * 86_400_000);
  return d.toISOString().slice(0, 10);
}

test.describe("e2e.events_search", () => {
  test("e2e.events_search.key_finds_its_events", async ({ page }) => {
    const { fixtures } = await seedAuth(page, "admin");
    await openEvents(page);
    await searchBox(page).fill(fixtures.search_key);
    await searchBox(page).press("Enter");
    await expect(page).toHaveURL(new RegExp(`[?&]q=${fixtures.search_key}`));
    await expect(caption(page)).toHaveText("3 events · newest first", { timeout: 60_000 });
    await expect(eventLinks(page)).toHaveCount(3);
  });

  test("e2e.events_search.exact_id_opens_the_event", async ({ page }) => {
    const { fixtures } = await seedAuth(page, "admin");
    // Outside the range on purpose: an exact id opens whatever the filters say.
    await openEvents(page, "?range=1h&topic=" + encodeURIComponent(fixtures.topic_name));
    await searchBox(page).fill(fixtures.order_event_id);
    await searchBox(page).press("Enter");
    await expect(page).toHaveURL(new RegExp(`/photon/events/${fixtures.order_event_id}$`), {
      timeout: 60_000,
    });
    await expect(byTestId(page, "photon-event-meta")).toBeVisible({ timeout: 60_000 });
  });

  test("e2e.events_search.custom_range_narrows", async ({ page }) => {
    const { fixtures } = await seedAuth(page, "admin");
    const topic = encodeURIComponent(fixtures.orders_topic);
    const yesterday = utcDay(-1);
    await openEvents(page, `?topic=${topic}&range=custom&from=${yesterday}&to=${yesterday}`);
    await expect(byTestId(page, "photon-event-custom-range")).toBeVisible();
    await expect(byTestId(page, "photon-events-empty")).toContainText("No events in this range", {
      timeout: 60_000,
    });
    await openEvents(page, `?topic=${topic}&range=custom&from=${yesterday}&to=${utcDay(1)}`);
    await expect(caption(page)).toHaveText(`${fixtures.orders_total} events · newest first`, {
      timeout: 60_000,
    });
  });

  test("e2e.events_search.pages_with_a_total", async ({ page }) => {
    const { fixtures } = await seedAuth(page, "admin");
    await openEvents(page, `?topic=${encodeURIComponent(fixtures.orders_topic)}`);
    await expect(caption(page)).toHaveText(`${fixtures.orders_total} events · newest first`, {
      timeout: 60_000,
    });
    await expect(eventLinks(page)).toHaveCount(25);
    await byTestId(page, "photon-events-pagination").getByRole("button", { name: "Next page" }).click();
    await expect(page).toHaveURL(/[?&]page=2/);
    await expect(eventLinks(page)).toHaveCount(fixtures.orders_total - 25, { timeout: 60_000 });
  });

  test("e2e.events_search.filters_keep_focus_while_loading", async ({ page }) => {
    const { fixtures } = await seedAuth(page, "admin");
    await openEvents(page);
    await expect(caption(page)).toBeVisible({ timeout: 60_000 });
    await page.evaluate(() => {
      const bar = document.querySelector('[data-testid="photon-event-filters"]:not(style)');
      (bar as unknown as { __kept?: boolean }).__kept = true;
    });
    let release: () => void = () => {};
    const held = new Promise<void>((resolve) => (release = resolve));
    await page.route(/get_events_page/, async (route) => {
      await held;
      await route.continue();
    });
    await searchBox(page).fill(fixtures.search_key);
    await searchBox(page).press("Enter");
    await expect(byTestId(page, "photon-table-skeleton")).toBeVisible({ timeout: 30_000 });
    await expect(searchBox(page)).toBeFocused();
    await expect(searchBox(page)).toHaveValue(fixtures.search_key);
    release();
    await expect(caption(page)).toHaveText("3 events · newest first", { timeout: 60_000 });
    await expect(searchBox(page)).toBeFocused();
    const kept = await page.evaluate(() => {
      const bar = document.querySelector('[data-testid="photon-event-filters"]:not(style)');
      return (bar as unknown as { __kept?: boolean } | null)?.__kept === true;
    });
    expect(kept, "the toolbar must not remount while results load").toBe(true);
  });

  test("e2e.events_search.no_match_offers_the_last_7_days", async ({ page }) => {
    await seedAuth(page, "admin");
    await openEvents(page, "?q=no-such-key");
    const empty = byTestId(page, "photon-events-empty");
    await expect(empty).toContainText("No event or key matches no-such-key in the last 24 hours", {
      timeout: 60_000,
    });
    await empty.getByRole("button", { name: "Search the last 7 days" }).click();
    await expect(page).toHaveURL(/[?&]range=7d/);
    await expect(empty).toContainText("No event or key matches no-such-key in the last 7 days", {
      timeout: 60_000,
    });
  });

  test("e2e.events_search.phone_width", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await seedAuth(page, "admin");
    await openEvents(page);
    await expect(searchBox(page)).toBeVisible();
    await expect(caption(page)).toBeVisible({ timeout: 60_000 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow).toBeLessThanOrEqual(1);
  });
});
