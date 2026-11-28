import { test, expect, seedAuth, waitForHydrated, type Page } from "./fixtures";

/** Orbital components put their test id on an injected `<style>` too; skip that copy. */
const byTestId = (page: Page, id: string) => page.locator(`[data-testid="${id}"]:not(style)`);
const eventPath = (id: string) => `/photon/events/${encodeURIComponent(id)}`;

async function openEvent(page: Page, id: string) {
  await page.goto(eventPath(id), { waitUntil: "domcontentloaded" });
  await waitForHydrated(page);
  await expect(byTestId(page, "photon-event-detail")).toBeVisible({ timeout: 60_000 });
}

/** POST to the owners-only reveal log server fn as the seeded session. */
async function revealLog(page: Page, topic: string) {
  return page.request.post("/api/photon_reveal_log", {
    form: { topic_name: topic },
  });
}

test.describe("e2e.event_page", () => {
  test("e2e.event_page.delivery_rows", async ({ page }) => {
    const { fixtures } = await seedAuth(page, "admin");
    await openEvent(page, fixtures.order_event_id);
    await expect(byTestId(page, "photon-event-title")).toContainText(`Event ${fixtures.order_event_id}`);
    const handled = byTestId(page, "photon-delivery-send-confirmation");
    await expect(handled).toHaveAttribute("data-state", "Handled", { timeout: 60_000 });
    const waiting = byTestId(page, "photon-delivery-warehouse");
    await expect(waiting).toHaveAttribute("data-state", "Waiting");
    await expect(waiting).toContainText("1st in line");
    const failed = byTestId(page, "photon-delivery-fraud-check");
    await expect(failed).toHaveAttribute("data-state", "Failed");
    await expect(failed).toContainText("1 attempt");
    await expect(failed).toContainText("fraud service unavailable");
    const meta = byTestId(page, "photon-event-meta");
    await expect(meta.getByRole("link", { name: fixtures.orders_topic })).toBeVisible();
    await expect(meta).toContainText("A person");
  });

  test("e2e.event_page.personal_fields_masked_in_server_html", async ({ page }) => {
    const { fixtures } = await seedAuth(page, "viewer");
    // The document the server sent, before any hydration or reveal.
    const res = await page.goto(eventPath(fixtures.order_event_id), { waitUntil: "domcontentloaded" });
    expect(res, "navigation response").toBeTruthy();
    const html = await res!.text();
    expect(html).toContain("photon-event-payload");
    expect(html).not.toContain("customer1@example.com");
    expect(html).not.toContain('"ines"');
    await waitForHydrated(page);
    const email = byTestId(page, "photon-payload-field-customer_email");
    await expect(email.getByText("Personal")).toBeVisible({ timeout: 60_000 });
    await expect(email).not.toContainText("@example.com");
  });

  test("e2e.event_page.reveal_with_a_reason_is_recorded", async ({ page }) => {
    const { fixtures } = await seedAuth(page, "viewer");
    await openEvent(page, fixtures.order_event_id);
    // Viewers can't run subscriptions, so no Redeliver buttons.
    await expect(byTestId(page, "photon-event-deliveries")).toBeVisible({ timeout: 60_000 });
    await expect(page.getByRole("button", { name: "Redeliver…" })).toHaveCount(0);

    const details = `Ticket ${Date.now()}: customer says no confirmation email`;
    await byTestId(page, "photon-reveal-open").click();
    const dialog = byTestId(page, "photon-reveal-dialog");
    await expect(dialog).toBeVisible();
    await dialog.locator("select").selectOption("support");
    await dialog.locator("textarea").fill(details);
    await byTestId(page, "photon-reveal-submit").click();

    const banner = byTestId(page, "photon-reveal-banner");
    await expect(banner).toContainText("Personal fields shown to you until you leave this page", {
      timeout: 60_000,
    });
    await expect(banner).toContainText(details);
    await expect(byTestId(page, "photon-payload-field-customer_email")).toContainText(
      "customer1@example.com",
    );
    await expect(byTestId(page, "photon-revealed-publisher")).toHaveText("ines");

    // The log is for the topic's owners: the viewer can't read it, the admin can.
    expect((await revealLog(page, fixtures.orders_topic)).ok()).toBeFalsy();
    await seedAuth(page, "admin");
    const log = await revealLog(page, fixtures.orders_topic);
    expect(log.ok()).toBeTruthy();
    const rows = (await log.json()) as Array<{ event_id: string; revealed_by: string; details: string; reason: string }>;
    const row = rows.find((r) => r.details === details);
    expect(row, JSON.stringify(rows)).toBeTruthy();
    expect(row?.event_id).toBe(fixtures.order_event_id);
    expect(row?.revealed_by).toBe("viewer");
    expect(row?.reason).toBe("Support");
  });

  test("e2e.event_page.redeliver_runs_one_handler_once", async ({ page }) => {
    const before = (await seedAuth(page, "admin")).fixtures;
    await openEvent(page, before.order_event_id);
    await byTestId(page, "photon-delivery-send-confirmation")
      .getByRole("button", { name: "Redeliver…" })
      .click();
    const dialog = byTestId(page, "photon-redeliver-dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText("no other subscription does");
    await expect(dialog).toContainText("acts on this event twice");
    await byTestId(page, "photon-redeliver-submit").click();
    await expect(byTestId(page, "photon-redeliver-notice")).toContainText(
      "Sent the event to send-confirmation again.",
      { timeout: 60_000 },
    );
    const after = (await seedAuth(page, "admin")).fixtures;
    expect(after.order_confirmations).toBe(before.order_confirmations + 1);
  });

  test("e2e.event_page.expired_keeps_metadata", async ({ page }) => {
    const { fixtures } = await seedAuth(page, "admin");
    await openEvent(page, fixtures.expired_event_id);
    const notice = byTestId(page, "photon-event-expired");
    await expect(notice).toContainText("The payload has expired", { timeout: 60_000 });
    await expect(notice).toContainText(`ask the owners of ${fixtures.orders_topic}`);
    await expect(byTestId(page, "photon-event-meta")).toContainText("ord-expired");
    await expect(byTestId(page, "photon-event-payload")).toHaveCount(0);
  });

  test("e2e.event_page.not_found_has_a_way_back", async ({ page }) => {
    await seedAuth(page, "admin");
    await openEvent(page, "ev-mistyped-0000");
    const missing = byTestId(page, "photon-event-not-found");
    await expect(missing).toContainText("No event with this id", { timeout: 60_000 });
    await missing.getByRole("button", { name: "Search events" }).click();
    await expect(page).toHaveURL(/\/photon\/events$/);
  });

  test("e2e.event_page.phone_width", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    const { fixtures } = await seedAuth(page, "admin");
    await openEvent(page, fixtures.order_event_id);
    await expect(byTestId(page, "photon-event-meta")).toBeVisible({ timeout: 60_000 });
    await expect(byTestId(page, "photon-event-payload")).toBeVisible();
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow).toBeLessThanOrEqual(1);
  });
});
