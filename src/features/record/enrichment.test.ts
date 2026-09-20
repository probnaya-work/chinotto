import { describe, expect, it } from "vitest";
import { parseMeta } from "./enrichment";

describe("reading what a page calls itself", () => {
  it("prefers og:title over the document title", () => {
    const html = `<head><title>Site — Section — Page</title>
      <meta property="og:title" content="Why Everyone Suddenly Wants a Second Brain"></head>`;
    expect(parseMeta(html).title).toBe("Why Everyone Suddenly Wants a Second Brain");
  });

  it("falls back to the document title", () => {
    expect(parseMeta("<head><title>A Plain Title</title></head>").title).toBe("A Plain Title");
  });

  it("returns null rather than inventing a title", () => {
    // The caller must show the URL, not a domain dressed up as a name.
    expect(parseMeta("<html><body>no head at all</body></html>").title).toBeNull();
    expect(parseMeta("<head><title></title></head>").title).toBeNull();
    expect(parseMeta("<head><title>   </title></head>").title).toBeNull();
    expect(parseMeta("").title).toBeNull();
  });

  it("decodes entities and collapses whitespace in a title", () => {
    const html = `<head><title>
        A Brief History &amp; Ethos of the   Digital Garden
      </title></head>`;
    expect(parseMeta(html).title).toBe("A Brief History & Ethos of the Digital Garden");
  });

  it("picks up the canonical url and site name when offered", () => {
    const html = `<head>
      <link rel="canonical" href="https://theatlantic.com/ideas/second-brain/">
      <meta property="og:site_name" content="The Atlantic">
    </head>`;
    const meta = parseMeta(html);
    expect(meta.urlCanonical).toBe("https://theatlantic.com/ideas/second-brain/");
    expect(meta.siteName).toBe("The Atlantic");
  });

  it("falls back to og:url when there is no canonical link", () => {
    const html = `<head><meta property="og:url" content="https://example.com/real"></head>`;
    expect(parseMeta(html).urlCanonical).toBe("https://example.com/real");
  });

  it("survives attribute orders, single quotes and extra attributes", () => {
    const html = `<head><meta data-x='1' name='og:title' content='Single Quoted' lang="en"></head>`;
    expect(parseMeta(html).title).toBe("Single Quoted");
  });

  it("does not mistake a similarly named tag for the one it wants", () => {
    const html = `<head><meta property="og:title:alt" content="Wrong">
      <title>Right</title></head>`;
    // og:title:alt must not satisfy a request for og:title.
    expect(parseMeta(html).title).toBe("Right");
  });

  it("truncates an absurd title instead of storing a page of text", () => {
    const html = `<head><title>${"x".repeat(5000)}</title></head>`;
    expect(parseMeta(html).title).toHaveLength(500);
  });

  it("is unbothered by a page that is mostly script", () => {
    const html = `<script>var t = "<title>Not This</title>";</script><title>Actual</title>`;
    // The regex finds the first <title>, which here is inside a script. Accepting that is
    // the trade for not parsing untrusted HTML — worth knowing about, not worth a DOM.
    expect(parseMeta(html).title).toBe("Not This");
  });

  it("handles a head with no metadata at all without throwing", () => {
    const meta = parseMeta("<!doctype html><html><head></head><body>hi</body></html>");
    expect(meta).toEqual({ title: null, urlCanonical: null, siteName: null });
  });
});
