import { test, expect, seedAuth, waitForHydrated, type Page } from "./fixtures";

async function open(page: Page, path: string) {
  await page.goto(path, { waitUntil: "domcontentloaded" });
  await waitForHydrated(page);
}

/** Worst first, as the list sorts. */
const RANK = ["failing", "stalled", "paused", "catching-up", "not-started", "healthy"];

async function healthsInOrder(page: Page): Promise<string[]> {
  const table = page.getByTestId("photon-subscriptions-table");
  const badges = table.locator('[data-testid="photon-health-badge"]:not(style)');
  return badges.evaluateAll((els) => els.map((el) => el.getAttribute("data-health") ?? ""));
}

test.describe("e2e.subs_health", () => {
  test("e2e.subs_health.badges_worst_first", async ({ page }) => {
    await seedAuth(page, "admin");
    await open(page, "/photon/subscriptions");
    const table = page.getByTestId("photon-subscriptions-table");
    await expect(table).toBeVisible({ timeout: 60_000 });

    // The stalled fixture turns Stalled two seconds after boot; reload until it has.
    await expect
      .poll(
        async () => {
          await page.reload({ waitUntil: "domcontentloaded" });
          await waitForHydrated(page);
          return healthsInOrder(page);
        },
        { timeout: 60_000 },
      )
      .toEqual(expect.arrayContaining(["failing", "stalled", "paused", "healthy"]));

    const healths = await healthsInOrder(page);
    const ranks = healths.map((h) => RANK.indexOf(h));
    expect(ranks).toEqual([...ranks].sort((a, b) => a - b));
    expect(healths[0]).toBe("failing");

    const failingRow = table.locator("tr", { hasText: "e2e-failing" });
    await expect(failingRow).toContainText("Failing");
    await expect(failingRow).toContainText(/failed in 24 h/);
    const stalledRow = table.locator("tr", { hasText: "e2e-stalled" });
    await expect(stalledRow).toContainText("Stalled");
    await expect(stalledRow).toContainText(/\d+ waiting, oldest/);
    await expect(stalledRow).toContainText("Never");
    await expect(table.locator("tr", { hasText: "e2e-ops-handler" })).toContainText("Up to date");
  });

  test("e2e.subs_health.filter_and_empty_state", async ({ page }) => {
    await seedAuth(page, "admin");
    await open(page, "/photon/subscriptions");
    const table = page.getByTestId("photon-subscriptions-table");
    await expect(table).toBeVisible({ timeout: 60_000 });

    await table.locator('[data-testid="photon-health-filter"] select').selectOption("failing");
    await expect(page).toHaveURL(/\?health=failing/);
    await expect
      .poll(async () => healthsInOrder(page), { timeout: 30_000 })
      .toEqual(["failing"]);
    await expect(table.getByText("e2e-ops-handler")).toHaveCount(0);

    // The filter survives a reload from the URL, and its empty state names it.
    await open(page, "/photon/subscriptions?health=not-started");
    await expect(page.getByTestId("photon-subscriptions-empty")).toContainText(
      "No subscriptions are Not started",
      { timeout: 60_000 },
    );
    await page
      .getByTestId("photon-subscriptions-table")
      .locator('[data-testid="photon-health-filter"] select')
      .selectOption("");
    await expect(page).not.toHaveURL(/health=/);
    await expect(page.getByTestId("photon-subscriptions-table").getByText("e2e-ops-handler")).toBeVisible();
  });

  test("e2e.subs_health.search_name_and_topic", async ({ page }) => {
    await seedAuth(page, "admin");
    await open(page, "/photon/subscriptions");
    const table = page.getByTestId("photon-subscriptions-table");
    await expect(table).toBeVisible({ timeout: 60_000 });
    await table.getByRole("searchbox").or(table.getByRole("textbox")).first().fill("test.photon.e2e.fail");
    await expect(table.getByText("e2e-failing")).toBeVisible();
    await expect(table.getByText("e2e-ops-handler")).toHaveCount(0);
  });

  test("e2e.subs_health.narrow_list", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await seedAuth(page, "admin");
    await open(page, "/photon/subscriptions");
    const table = page.getByTestId("photon-subscriptions-table");
    await expect(table).toBeVisible({ timeout: 60_000 });
    // Below Md the table becomes a list: name, then the health word and waiting text.
    await expect(table.getByText("e2e-failing").first()).toBeInViewport();
    await expect(table.getByText("Failing", { exact: true }).last()).toBeInViewport();
    await expect(table.getByText(/failed in 24 h/).first()).toBeInViewport();
  });
});
