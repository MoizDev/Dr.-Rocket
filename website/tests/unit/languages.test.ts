import { describe, expect, it } from "vitest";
import { LEVELS, GROUPS, defaultSettings, printingStyles, showsSharingOption, stepperLevel, TEACHPACKS } from "@/lib/demos/data/languages";

describe("language levels (copied from src/racket/lang.rs)", () => {
  it("names and descriptions", () => {
    expect(LEVELS.map((l) => [l.id, l.name, l.summary])).toMatchSnapshot();
  });
  it("every level is in exactly one group", () => {
    const ids = GROUPS.flatMap((g) => g.languages);
    expect(new Set(ids).size).toBe(ids.length);
    expect([...ids].sort()).toEqual(LEVELS.map((l) => l.id).sort());
  });
  it("Beginning Student has no quasiquote output", () => {
    expect(printingStyles("beginner")).toEqual(["constructor", "write", "print"]);
    expect(printingStyles("intermediate")).toContain("quasiquote");
  });
  it("DeinProgramm defaults to case-insensitive", () => {
    for (const id of ["sdp-beginner", "sdp", "sdp-advanced"] as const) expect(defaultSettings(id).caseSensitive).toBe(false);
    expect(defaultSettings("beginner").caseSensitive).toBe(true);
  });
  it("show sharing: hidden for Beginner levels, on by default only for Advanced", () => {
    expect(showsSharingOption("beginner")).toBe(false);
    expect(showsSharingOption("beginner-abbr")).toBe(false);
    expect(showsSharingOption("intermediate")).toBe(true);
    for (const l of LEVELS) expect(defaultSettings(l.id).showSharing).toBe(l.id === "advanced");
  });
  it("stepper card only where the stepper works", () => {
    const yes = LEVELS.filter((l) => stepperLevel(l.id) !== null).map((l) => l.id);
    expect(yes).toEqual(["beginner", "beginner-abbr", "intermediate", "intermediate-lambda", "sdp-beginner", "sdp"]);
  });
  it("has the teachpacks", () => expect(TEACHPACKS.length).toBe(12));
});
