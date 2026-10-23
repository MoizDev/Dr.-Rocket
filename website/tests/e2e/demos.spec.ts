// The stepper, rich source and language demos: prepared results, how they move, and how they
// keep replaying after edits.
import { expect, test, type Locator, type Page } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/");
  await page.evaluate(() => document.querySelectorAll("[data-reveal]").forEach((el) => el.classList.add("is-in")));
});

const live = async (page: Page, id: string) => {
  const root = page.locator(`[data-demo="${id}"]`);
  await root.scrollIntoViewIfNeeded();
  await expect(root.locator(".cm-content").first()).toBeAttached();
  return root;
};
const typeAtEnd = async (page: Page, root: Locator, text: string) => {
  await root.locator(".cm-content").first().focus();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type(text);
};
const stepNo = async (root: Locator) => Number(/Step (\d+) of/.exec((await root.getByText(/^Step \d+ of \d+$/).textContent()) ?? "")?.[1]);

test("stepper steps, restarts, and keeps stepping after an edit", async ({ page }) => {
  const root = await live(page, "stepper");
  const rest = await stepNo(root);
  expect(rest).toBeGreaterThan(1);
  await root.getByRole("button", { name: "Next step" }).click();
  await expect.poll(() => stepNo(root)).toBe(rest + 1);
  await root.getByRole("button", { name: "Restart" }).click();
  await expect.poll(() => stepNo(root)).toBe(1);
  await typeAtEnd(page, root, "\n(define z 3)");
  await root.getByRole("button", { name: "Next step" }).click();
  await expect.poll(() => stepNo(root)).toBe(2);
  await root.getByRole("button", { name: /Restore/ }).click();
  await expect(root.locator(".cm-content")).not.toContainText("(define z 3)");
  await expect(root.getByText("reduction")).toBeVisible();
});

test("rich source runs to prepared images", async ({ page }) => {
  const root = await live(page, "rich-source");
  await root.getByRole("button", { name: "Run poster.rkt" }).click();
  await expect(root.getByRole("status").filter({ hasText: "No tests" })).toBeVisible();
  expect(await root.getByRole("img").count()).toBeGreaterThan(0);
});

test("languages picks a level and confirms it with OK", async ({ page }) => {
  const root = page.locator('[data-demo="languages"]');
  await root.scrollIntoViewIfNeeded();
  const opts = root.getByRole("option");
  const target = opts.nth(1);
  const name = (await target.textContent())!.trim();
  await target.click();
  await expect(target).toHaveAttribute("aria-selected", "true");
  await root.getByRole("button", { name: "OK" }).click();
  await expect(root.locator("[data-confirmed]")).toContainText(name.split("\n")[0].slice(0, 8));
});
