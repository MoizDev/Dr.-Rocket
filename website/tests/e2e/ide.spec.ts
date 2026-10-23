// The IDE demos: the hero's Run and REPL, the terminals' prepared commands, project switching
// and quick open, and the theme cards' mirror. Nothing here runs Racket.
import { expect, test, type Page } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/");
  await page.evaluate(() => document.querySelectorAll("[data-reveal]").forEach((el) => el.classList.add("is-in")));
});

const live = async (page: Page, id: string, editors = 1) => {
  const root = page.locator(`[data-demo="${id}"]`);
  await root.scrollIntoViewIfNeeded();
  await expect(root.locator(".cm-content")).toHaveCount(editors);
  return root;
};

const typeAtEnd = async (page: Page, root: ReturnType<Page["locator"]>, text: string) => {
  await root.locator(".cm-content").first().focus();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type(text);
};

test.describe("hero ide", () => {
  test("REPL answers from the table, Run replays, edit withdraws", async ({ page }) => {
    const root = await live(page, "hero-ide");
    const log = root.getByRole("log", { name: "Interactions" });
    await expect(log).toContainText("All 4 tests passed");
    const input = root.getByLabel("Evaluate a Racket expression");
    await input.fill("(letter 64)");
    await input.press("Enter");
    await expect(log).toContainText('"C"');
    await root.getByRole("button", { name: "Run" }).first().click();
    await expect(log).toContainText("79.75");

    await typeAtEnd(page, root, "\n(define x 1)");
    await root.getByRole("button", { name: "Run" }).first().click();
    await expect(log).toContainText("doesn't run code");
    await root.getByRole("button", { name: /Restore/ }).first().click();
    await expect(root.locator(".cm-content")).not.toContainText("(define x 1)");
  });
});

test.describe("hero terminal", () => {
  test("prepared commands, help and unknown commands", async ({ page }) => {
    const root = page.locator('[data-demo="hero-terminal"]');
    await root.scrollIntoViewIfNeeded();
    const input = root.getByRole("textbox", { name: /Terminal command in/ });
    const out = root.getByRole("log", { name: "Terminal output" });
    await input.fill("pwd");
    await input.press("Enter");
    await expect(out).toContainText("/Users/student/");
    await input.fill("help");
    await input.press("Enter");
    await expect(out).toContainText("Prepared commands");
    await input.fill("make coffee");
    await input.press("Enter");
    await expect(out).toContainText("only knows a few prepared commands");
    await input.fill("clear");
    await input.press("Enter");
    await expect(out).not.toContainText("Prepared commands");
  });
});

test.describe("projects", () => {
  test("switches projects and opens a file with quick open", async ({ page }) => {
    const root = await live(page, "projects");
    await root.getByRole("tab", { name: "math239" }).first().click();
    await expect(root.getByRole("tree", { name: "math239 files" })).toBeVisible();
    // The sidebar's search button can sit in the card's cropped edge, so use the shortcut.
    await root.getByRole("tree", { name: "math239 files" }).locator("[role=treeitem]").first().focus();
    await page.keyboard.press("ControlOrMeta+p");
    const search = root.getByLabel("Search files by name");
    await search.fill("degree");
    await expect(root.getByRole("option").first()).toContainText("degree");
    await search.press("Enter");
    await expect(root.locator(".cm-content")).toContainText("degree");
    const term = root.getByRole("textbox", { name: /Terminal command in/ });
    await term.fill("racket graphs/degree.rkt");
    await term.press("Enter");
    await expect(root.getByRole("log", { name: "Terminal output" })).toContainText("(c . 3)");
  });
});

test.describe("themes", () => {
  test("the dark card mirrors edits to the light card", async ({ page }) => {
    const light = await live(page, "theme-light");
    const dark = page.locator('[data-demo="theme-dark"]');
    await typeAtEnd(page, light, "\n(define mirrored 1)");
    await expect(dark).toContainText("(define mirrored 1)");
    await light.getByRole("button", { name: "Run" }).first().click();
    await light.getByRole("button", { name: /Restore/ }).first().click();
    await expect(dark).not.toContainText("(define mirrored 1)");
  });
});
