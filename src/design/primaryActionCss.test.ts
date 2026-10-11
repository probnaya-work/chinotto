import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(path.join(process.cwd(), "src/design/tokens.css"), "utf8");

describe("filled primary action CSS", () => {
  it("keeps dark text on the bright button while hovered", () => {
    expect(css).toMatch(
      /\.chinotto-verb\.chinotto-verb--filled:hover:not\(:disabled\)\s*\{[^}]*background:\s*var\(--agency-hover\)\s*!important;[^}]*color:\s*var\(--surface\)\s*!important;/s,
    );
  });

  it("keeps the normal verb underline as a visible hover response", () => {
    const genericHover = css.match(/\.chinotto-verb:hover:not\(:disabled\)\s*\{([^}]*)\}/s)?.[1];
    const filledHover = css.match(
      /\.chinotto-verb\.chinotto-verb--filled:hover:not\(:disabled\)\s*\{([^}]*)\}/s,
    )?.[1];

    expect(genericHover).toContain("text-decoration: underline");
    expect(filledHover).not.toContain("text-decoration: none");
  });
});
