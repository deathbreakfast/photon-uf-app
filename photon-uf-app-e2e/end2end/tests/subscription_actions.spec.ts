import {
  test,
  expect,
  seedAuth,
  waitForHydrated,
  publishEvents,
  type Page,
} from "./fixtures";

async function open(page: Page, path: string) {
  await page.goto(path, { waitUntil: "domcontentloaded" });
  await waitForHydrated(page);
}

const subPath = (id: string) => `/photon/subscriptions/${encodeURIComponent(id)}`;

/** Orbital components put their test id on an injected `<style>` too; skip that copy. */
const byTestId = (page: Page, id: string) =>
  page.locator(`[data-testid="${id}"]:not(style)`);

const ACTIONS = ["replay", "pause", "resume", "more"];

/** Reload until the page's health badge reads `health` (the stalled fixture takes 2 s). */
async function waitForHealth(page: Page, health: string) {
  await expect
    .poll(
      async () => {
        await page.reload({ waitUntil: "domcontentloaded" });
        await waitForHydrated(page);
        return byTestId(page, "photon-health-badge").first().getAttribute("data-health");
      },
      { timeout: 60_000 },
    )
    .toBe(health);
}

test.describe("e2e.sub_actions", () => {
  test("e2e.sub_actions.stalled_what_to_do_and_waiting", async ({ page }) => {
    const seeded = await seedAuth(page, "admin");
    await open(page, subPath(seeded.fixtures.stalled_subscription_id));
    await waitForHealth(page, "stalled");

    const todo = page.getByTestId("photon-what-to-do");
    await expect(todo).toContainText("Check that the");
    await expect(todo).toContainText("Ask them first if the handler is theirs.");
    await expect(todo.getByTestId("photon-what-to-do-pause")).toBeVisible();
    await expect(todo.getByTestId("photon-what-to-do-replay")).toBeVisible();

    const health = page.getByTestId("photon-subscription-health");
    for (const label of ["Events waiting", "Oldest has waited", "Last handled", "Failed in 24 h"]) {
      await expect(health.getByText(label, { exact: true })).toBeVisible();
    }

    const waiting = page.getByTestId("photon-waiting-events");
    const rows = waiting.locator("tbody tr");
    await expect(rows.first()).toBeVisible({ timeout: 30_000 });
    const seqs = await rows.evaluateAll((trs) =>
      trs.map((tr) => Number(tr.querySelectorAll("td")[2]?.textContent?.trim())),
    );
    expect(seqs.length).toBeGreaterThan(0);
    expect(seqs).toEqual([...seqs].sort((a, b) => a - b));
  });

  test("e2e.sub_actions.failing_last_error_and_retry", async ({ page }) => {
    const seeded = await seedAuth(page, "admin");
    await open(page, subPath(seeded.fixtures.failing_subscription_id));
    await expect(byTestId(page, "photon-health-badge").first()).toHaveAttribute(
      "data-health",
      "failing",
      { timeout: 60_000 },
    );
    await expect(byTestId(page, "photon-subscription-last-error")).toContainText(
      "SMTP relay refused the connection",
    );
    const todo = page.getByTestId("photon-what-to-do");
    await expect(todo).toContainText("Press Retry now");
    await todo.getByTestId("photon-what-to-do-retry").click();
    await expect(byTestId(page, "photon-action-notice")).toContainText("Retry started", {
      timeout: 30_000,
    });
  });

  test("e2e.sub_actions.pause_then_resume", async ({ page }) => {
    const seeded = await seedAuth(page, "admin");
    await open(page, subPath(seeded.fixtures.subscription_id));
    await page.getByTestId("photon-sub-action-pause").click({ timeout: 60_000 });
    const dialog = byTestId(page, "photon-pause-dialog");
    await expect(dialog).toContainText("stops taking events");
    await expect(dialog).toContainText("keep running");
    await dialog.getByTestId("photon-pause-confirm").click();

    await expect(byTestId(page, "photon-action-notice")).toContainText("e2e-ops-handler paused", {
      timeout: 30_000,
    });
    await expect(byTestId(page, "photon-subscription-paused")).toContainText("Paused by admin");
    await expect(byTestId(page, "photon-health-badge").first()).toHaveAttribute("data-health", "paused");

    await page.getByTestId("photon-sub-action-resume").click();
    const resume = byTestId(page, "photon-pause-dialog");
    await expect(resume).toContainText("starts again from");
    await resume.getByTestId("photon-pause-confirm").click();
    await expect(byTestId(page, "photon-action-notice")).toContainText("e2e-ops-handler resumed", {
      timeout: 30_000,
    });
    await expect(byTestId(page, "photon-subscription-paused")).toHaveCount(0);
    await expect(page.getByTestId("photon-sub-action-pause")).toBeVisible();
  });

  async function replayWith(page: Page, pick: (dialog: ReturnType<typeof byTestId>) => Promise<void>) {
    await page.getByTestId("photon-sub-action-replay").click({ timeout: 60_000 });
    const dialog = byTestId(page, "photon-replay-dialog");
    await expect(dialog).toBeVisible();
    await pick(dialog);
    const preview = byTestId(page, "photon-replay-preview");
    await expect(preview).toContainText(/^\d+ events from .+ to now/, { timeout: 30_000 });
    const count = Number((await preview.textContent())?.match(/^(\d+) events/)?.[1]);
    expect(count).toBeGreaterThan(0);
    const confirm = dialog.getByTestId("photon-replay-confirm");
    await expect(confirm).toHaveText(`Replay ${count} events`);
    await confirm.click();
    await expect(byTestId(page, "photon-action-notice")).toContainText("Replay started", {
      timeout: 30_000,
    });
    await expect(byTestId(page, "photon-action-notice")).toContainText(`gets ${count} events again`);
  }

  test("e2e.sub_actions.replay_from_seq", async ({ page }) => {
    const seeded = await seedAuth(page, "admin");
    await publishEvents(page, "ops", 2);
    await open(page, subPath(seeded.fixtures.subscription_id));
    await replayWith(page, async (dialog) => {
      await dialog.getByText("From a sequence number").click();
      await dialog.getByTestId("photon-replay-seq").locator("input").fill("0");
      await expect(byTestId(page, "photon-replay-preview")).toContainText("from seq 1 to now", {
        timeout: 30_000,
      });
    });
  });

  test("e2e.sub_actions.replay_from_event_and_time", async ({ page }) => {
    const seeded = await seedAuth(page, "admin");
    await open(page, subPath(seeded.fixtures.subscription_id));
    await replayWith(page, async (dialog) => {
      await dialog.getByText("From an event id").click();
      await dialog.getByTestId("photon-replay-event").locator("input").fill(seeded.fixtures.event_id);
    });
    await replayWith(page, async (dialog) => {
      await dialog.getByText("From a time").click();
      await dialog.getByTestId("photon-replay-time").locator("input").fill("00:00");
    });
  });

  test("e2e.sub_actions.skip_with_typed_confirm", async ({ page }) => {
    const seeded = await seedAuth(page, "admin");
    await publishEvents(page, "backlog", 2);
    await open(page, subPath(seeded.fixtures.backlog_subscription_id));
    await page.getByTestId("photon-sub-action-more").click({ timeout: 60_000 });
    await byTestId(page, "photon-sub-action-skip").click();

    const dialog = byTestId(page, "photon-skip-dialog");
    await expect(dialog).toContainText("are never delivered to it");
    const confirm = dialog.getByTestId("photon-skip-confirm");
    const label = (await confirm.textContent()) ?? "";
    const count = Number(label.match(/Skip (\d+) events/)?.[1]);
    expect(count).toBeGreaterThan(0);
    await expect(confirm).toBeDisabled();
    const name = dialog.getByTestId("photon-skip-confirm-name").locator("input");
    await name.fill("e2e-backlo");
    await expect(confirm).toBeDisabled();
    await name.fill("e2e-backlog");
    await expect(confirm).toBeEnabled();
    await confirm.click();

    await expect(byTestId(page, "photon-action-notice")).toContainText(`Skipped ${count} events`, {
      timeout: 30_000,
    });
    await expect(page.getByTestId("photon-waiting-events-empty")).toContainText("Nothing waiting");
  });

  test("e2e.sub_actions.viewer_sees_no_actions", async ({ page }) => {
    const seeded = await seedAuth(page, "viewer");
    for (const id of [seeded.fixtures.subscription_id, seeded.fixtures.stalled_subscription_id]) {
      await open(page, subPath(id));
      await expect(byTestId(page, "photon-subscription-view-only")).toContainText(
        "ask an owner: Photon admins",
        { timeout: 60_000 },
      );
      for (const action of ACTIONS) {
        await expect(page.getByTestId(`photon-sub-action-${action}`)).toHaveCount(0);
      }
      await expect(page.getByTestId("photon-what-to-do")).toHaveCount(0);
      await expect(page.getByTestId("photon-subscription-health")).toBeVisible();
    }
  });

  test("e2e.sub_actions.narrow_page", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    const seeded = await seedAuth(page, "admin");
    await open(page, subPath(seeded.fixtures.subscription_id));
    // Header actions wrap under the name instead of running off screen.
    await expect(page.getByTestId("photon-sub-action-replay")).toBeInViewport({ timeout: 60_000 });
    await expect(page.getByTestId("photon-sub-action-pause")).toBeInViewport();
    await expect(page.getByTestId("photon-subscription-health")).toBeVisible();
  });
});
