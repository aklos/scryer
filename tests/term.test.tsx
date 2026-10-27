import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { Term } from "../src/entities/model/Term";
import { CLAIM_TERM, docsUrl, GLOSSARY } from "../src/entities/model/glossary";

describe("Term", () => {
  it("Shows a glossary word with its definition and the thing's own description on hover, and a link to its docs section", () => {
    const html = renderToStaticMarkup(<Term id="layer" detail="Pure rules, no I/O." />).replaceAll("&#x27;", "'");
    expect(html).toContain(">layer<");
    expect(html).toContain(GLOSSARY.layer.definition);
    expect(html).toContain("Pure rules, no I/O.");
    expect(html).toContain("Read more");
    expect(docsUrl("layer")).toBe("https://aklos.github.io/scryer/concepts/structure/#styles");
  });

  it("If a term has no docs section, shows its definition without a link", () => {
    const html = renderToStaticMarkup(<Term id="code" />);
    expect(html).toContain(GLOSSARY.code.definition);
    expect(html).not.toContain("Read more");
  });

  it("Names each claim state by the word every surface shows for it", () => {
    expect(Object.values(CLAIM_TERM).map((t) => GLOSSARY[t].label)).toEqual(["proven", "unverified", "failing", "todo"]);
  });
});
