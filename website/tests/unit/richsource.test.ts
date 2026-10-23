import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { createElement } from "react";
import { answer, poster, ROCKET, size, MISS, RUN_OUTPUT, unquote } from "@/lib/demos/data/richsource-repl";
import RImage from "@/components/demos/rich-source/RImage";

describe("rich-source REPL answers", () => {
  it("prepared inputs, spacing-insensitive", () => {
    expect(answer("rocket")).toEqual(ROCKET);
    expect(answer("( beside  rocket\nrocket rocket )")).toEqual(RUN_OUTPUT[1]);
    expect(answer("(above rocket rocket)")?.kind).toBe("above");
  });
  it("poster works for any string literal, escapes included", () => {
    expect(answer('(poster "Hello")')).toEqual(poster("Hello"));
    expect(answer('(poster "a \\"q\\" b")')).toEqual(poster('a "q" b'));
    expect(unquote('"x\\\\y"')).toBe("x\\y");
  });
  it("misses return null", () => {
    expect(answer('(poster "open')).toBeNull();
    expect(answer("(poster 5)")).toBeNull();
    expect(answer("(+ 1 2)")).toBeNull();
    expect(MISS).toContain('(poster "Hello")');
  });
  it("sizes", () => {
    expect(size(ROCKET)).toEqual({ w: 140, h: 140 });
    expect(size(RUN_OUTPUT[1])).toEqual({ w: 420, h: 140 });
    expect(size(poster("Hi")).h).toBe(174);
  });
  it("renders SVG, never <img>, with text escaped", () => {
    const html = renderToStaticMarkup(createElement(RImage, { img: poster("<script>x</script>"), uid: "t" }));
    expect(html).toContain("<svg");
    expect(html).not.toContain("<img");
    expect(html).not.toContain("<script>");
    expect(html).toContain("&lt;script&gt;");
  });
});
