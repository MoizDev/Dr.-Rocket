// The Visualizer and the debugger: prepared steps and pauses, how they move, and how edits and
// breakpoint changes withdraw them.
import { expect, test, type Page } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/");
  await page.evaluate(() => document.querySelectorAll("[data-reveal]").forEach((el) => el.classList.add("is-in")));
});

const live = async (page: Page, id: string) => {
  const root = page.locator(`[data-demo="${id}"]`);
  await root.scrollIntoViewIfNeeded();
  await expect(root.locator(".cm-content")).toHaveCount(1);
  return root;
};

/** Types into a demo's editor without clicking (the back card may sit under another). */
const typeAtEnd = async (page: Page, root: ReturnType<Page["locator"]>, text: string) => {
  await root.locator(".cm-content").focus();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type(text);
};

test.describe("visualizer", () => {
  test("rests on step 68, steps both ways", async ({ page }) => {
    const root = await live(page, "visualizer");
    await expect(root.getByText("Step 68 of 92", { exact: true })).toBeVisible();
    await expect(root.getByText("tree-sum", { exact: true }).first()).toBeVisible();
    await root.getByRole("button", { name: "Next step" }).click();
    await expect(root.getByText("Step 69 of 92", { exact: true })).toBeVisible();
    await root.getByRole("button", { name: "Previous step" }).click();
    await root.getByRole("button", { name: "Previous step" }).click();
    await expect(root.getByText("Step 67 of 92", { exact: true })).toBeVisible();
    await root.getByRole("button", { name: "Last step" }).click();
    await expect(root.getByText("Step 92 of 92", { exact: true })).toBeVisible();
    const slider = root.getByRole("slider", { name: "Step" });
    await slider.focus();
    await page.keyboard.press("Home");
    await expect(slider).toHaveAttribute("aria-valuenow", "1");
  });

  test("an edit withdraws the steps until the file is restored", async ({ page }) => {
    const root = await live(page, "visualizer");
    await typeAtEnd(page, root, "\n(+ 1 2)");
    await expect(root.locator("[data-notice]")).toBeVisible();
    await expect(root.getByRole("button", { name: "Next step" })).toHaveAttribute("aria-disabled", "true");
    await root.locator("[data-restore]").click();
    await expect(root.locator("[data-notice]")).toHaveCount(0);
    await expect(root.getByText("Step 68 of 92", { exact: true })).toBeVisible();
  });

  test("reformatting keeps the steps", async ({ page }) => {
    const root = await live(page, "visualizer");
    await typeAtEnd(page, root, "\n\n");
    await expect(root.locator("[data-notice]")).toHaveCount(0);
    await expect(root.getByText("Step 68 of 92", { exact: true })).toBeVisible();
  });
});

test.describe("debugger", () => {
  test("rests paused at line 20 with t = '() and three recursive frames", async ({ page }) => {
    const root = await live(page, "debugger");
    await expect(root.locator("[data-debug-state]")).toHaveText("Paused");
    const vars = root.getByRole("list", { name: "Variables" });
    await expect(vars.locator("li").first()).toHaveText("t = '()");
    await expect(root.getByRole("list", { name: "Call stack" }).locator("li")).toHaveCount(4);
  });

  test("focus raises the back card; continue, steps and frames move through the pauses", async ({ page }) => {
    const root = await live(page, "debugger");
    const cont = root.getByRole("button", { name: "Continue" });
    await cont.focus();
    expect(await root.evaluate((e) => getComputedStyle(e).zIndex)).toBe("3");
    await page.keyboard.press("Enter");
    // The next entry at line 20 is (tree-sum (node-right t)) of node 1, with t = '() again.
    await expect(root.getByRole("list", { name: "Call stack" }).locator("li")).toHaveCount(4);
    await root.getByRole("button", { name: "Step out" }).click();
    await expect(root.getByRole("list", { name: "Call stack" }).locator("li")).toHaveCount(3);
    const frames = root.getByRole("list", { name: "Call stack" }).getByRole("button");
    await frames.nth(2).click();
    await expect(frames.nth(2)).toHaveAttribute("aria-current", "true");
    await expect(root.getByRole("list", { name: "Variables" }).locator("li").first()).toContainText("(make-node 5");
    // Expand a structure inline.
    await root.getByRole("list", { name: "Variables" }).getByRole("button", { name: "Expand node" }).first().click();
    await expect(root.getByText("left", { exact: true }).first()).toBeVisible();
  });

  test("without breakpoints, continue runs to the end", async ({ page }) => {
    const root = await live(page, "debugger");
    await root.getByRole("button", { name: "Continue" }).focus();
    // Remove the line-20 breakpoint from the gutter.
    const n20 = root.locator(".cm-lineNumbers .cm-gutterElement", { hasText: /^20$/ });
    await n20.dispatchEvent("mousedown");
    await root.getByRole("button", { name: "Continue" }).click();
    await expect(root.locator("[data-debug-state]")).toHaveText("Finished · result 30");
  });

  test("other breakpoints show the notice and Restore breakpoint", async ({ page }) => {
    const root = await live(page, "debugger");
    await root.getByRole("button", { name: "Continue" }).focus();
    await root.locator(".cm-lineNumbers .cm-gutterElement", { hasText: /^22$/ }).dispatchEvent("mousedown");
    await expect(root.locator("[data-notice]")).toContainText("prepared for a breakpoint on line 20");
    await root.getByRole("button", { name: "Restore breakpoint" }).click();
    await expect(root.locator("[data-notice]")).toHaveCount(0);
    await expect(root.locator("[data-debug-state]")).toHaveText("Paused");
  });

  test("an edit withdraws the pauses until restored", async ({ page }) => {
    const root = await live(page, "debugger");
    await typeAtEnd(page, root, "\n(+ 1 2)");
    await expect(root.locator("[data-notice]")).toBeVisible();
    await expect(root.getByRole("list", { name: "Variables" })).toHaveCount(0);
    await root.locator("[data-restore]").click();
    await expect(root.getByRole("list", { name: "Variables" }).locator("li").first()).toHaveText("t = '()");
  });
});
