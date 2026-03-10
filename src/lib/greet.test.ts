import { describe, it, expect } from "vitest";
import { greet } from "./greet";

describe("greet", () => {
  it("should return a greeting with the given name", () => {
    expect(greet("World")).toBe("Hello, World! Welcome to HAAL Installer.");
  });

  it("should handle empty name", () => {
    expect(greet("")).toBe("Hello, ! Welcome to HAAL Installer.");
  });
});
