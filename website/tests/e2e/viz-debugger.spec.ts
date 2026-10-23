// The Visualizer and the debugger: prepared steps and pauses, how they move, and how they keep
// replaying after edits and breakpoint changes.
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

  test("an edit keeps the steps playing, with a notice until restored", async ({ page }) => {
    const root = await live(page, "visualizer");
    await typeAtEnd(page, root, "\n(+ 1 2)");
    await expect(root.locator("[data-notice]")).toContainText("Edited · replaying the original bst.rkt");
    await root.getByRole("button", { name: "Next step" }).click();
    await expect(root.getByText("Step 69 of 92", { exact: true })).toBeVisible();
    await root.locator("[data-restore]").click();
    await expect(root.locator("[data-notice]")).toHaveCount(0);
    await expect(root.locator(".cm-content")).not.toContainText("(+ 1 2)");
  });

  test("Recenter undoes zoom and pan", async ({ page }) => {
    const root = await live(page, "visualizer");
    const pct = root.getByText(/^\d+%$/);
    const world = root.getByRole("img").locator("> div");
    const transform = () => world.evaluate((e) => getComputedStyle(e).transform);
    const fitted = await pct.textContent();
    const at = await transform();
    await root.getByRole("button", { name: "Zoom in" }).click();
    await expect(pct).not.toHaveText(fitted!);
    const box = (await root.getByRole("img").boundingBox())!;
    await page.mouse.move(box.x + box.width / 2, box.y + box.height - 10);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 - 60, box.y + box.height - 40, { steps: 5 });
    await page.mouse.up();
    await expect.poll(transform).not.toBe(at);
    await root.getByRole("button", { name: "Recenter" }).click();
    await expect(pct).toHaveText(fitted!);
    await expect.poll(transform).toBe(at);
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

  test("other breakpoints don't block: continue still stops at the prepared line-20 pauses", async ({ page }) => {
    const root = await live(page, "debugger");
    await root.getByRole("button", { name: "Continue" }).focus();
    await root.locator(".cm-lineNumbers .cm-gutterElement", { hasText: /^22$/ }).dispatchEvent("mousedown");
    await expect(root.locator('[title="Breakpoint on line 22"]')).toHaveCount(1);
    await expect(root.locator("[data-notice]")).toHaveCount(0);
    await root.getByRole("button", { name: "Continue" }).click();
    await expect(root.locator("[data-debug-state]")).toHaveText("Paused");
    await expect(root.getByRole("list", { name: "Call stack" }).locator("li")).toHaveCount(4);
  });

  test("an edit keeps the pauses, with a notice until restored", async ({ page }) => {
    const root = await live(page, "debugger");
    await typeAtEnd(page, root, "\n(+ 1 2)");
    await expect(root.locator("[data-notice]")).toContainText("Edited · replaying the original bst.rkt");
    await expect(root.getByRole("list", { name: "Variables" }).locator("li").first()).toHaveText("t = '()");
    await root.getByRole("button", { name: "Step out" }).click();
    await expect(root.getByRole("list", { name: "Call stack" }).locator("li")).toHaveCount(3);
    await root.locator("[data-restore]").click();
    await expect(root.locator("[data-notice]")).toHaveCount(0);
    await expect(root.locator(".cm-content")).not.toContainText("(+ 1 2)");
  });
});
