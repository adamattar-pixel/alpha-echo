import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import ts from "typescript";
import { setLanguage } from "../app/i18n.mjs";

// Render the actual components without a browser, network or generated fixtures.
const [source, stylesheet] = await Promise.all([
  readFile(new URL("../app/intelligence-desk.tsx", import.meta.url), "utf8"),
  readFile(new URL("../app/globals.css", import.meta.url), "utf8"),
]);
const js = ts.transpileModule(source.replaceAll("import.meta.env", "({})"), {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX },
}).outputText
  .replaceAll('"react"', JSON.stringify(import.meta.resolve("react")))
  .replaceAll('"react/jsx-runtime"', JSON.stringify(import.meta.resolve("react/jsx-runtime")))
  .replaceAll('"./i18n.mjs"', JSON.stringify(new URL("../app/i18n.mjs", import.meta.url).href));
const { Overview, PositionDiagnosisCard, companyNewsForPosition } = await import("data:text/javascript;base64," + Buffer.from(js).toString("base64"));

const position = {
  instrument_id: "stock-1", symbol: "ACME", instrument_name: "Acme Industries", market: "us",
  owner_id: "account-1", owner_name: "Long-term account", position_snapshot_item_id: "holding-1",
};
function evidence(index, overrides = {}) {
  return {
    id: "news-" + index, related_instrument_id: position.instrument_id, coverage_scope: "company",
    title: "Company update " + index, publisher: "Issuer IR", published_at: "2026-10-0" + index + "T08:00:00Z",
    url: "https://example.com/news/" + index, excerpt: "Verified source context.", ...overrides,
  };
}
function diagnosis(overrides = {}) {
  return {
    id: "diagnosis-1", finding_type: "equity_position_diagnosis", market: "us",
    affected_object_type: "position_snapshot_item", affected_object_id: "holding-1",
    fact_summary: "Verified diagnosis for Acme.", impact_direction: "unchanged", evidence: [evidence(1)],
    diagnostic_dimensions: {
      intent_assessment: "matched", company_context: "Current company operating status.", future_expectation: "Evidence-based future expectations.",
      macro_context: "Hidden macro detail", industry_context: "Hidden industry detail", market_pricing: "Hidden pricing detail", expectation_gap: "Hidden gap detail",
      upside_catalysts: ["A verified upside catalyst."], downside_pressures: ["A verified downside pressure."], core_risks: ["A verified core risk."],
    }, ...overrides,
  };
}
const render = (component, props) => renderToStaticMarkup(createElement(component, props));

test("overview visually pins macro/cross-market above portfolio advice", () => {
  setLanguage("en");
  const findings = [
    { id: "portfolio", market: "portfolio", fact_summary: "Portfolio conclusion.", inference: null, conditional_action: null, invalidation_conditions: [] },
    { id: "macro", market: "macro", fact_summary: "Macro assessment.", inference: null },
    { id: "cross", market: "cross", fact_summary: "Cross-market transmission.", inference: null },
    diagnosis(),
  ];
  const html = render(Overview, {
    data: { findings }, briefings: [], selectedDate: "2026-10-03", selectedReportId: "report-1", disabled: false,
    onDateChange() {}, onReportChange() {}, async onCreate() {},
    detail: { run: { id: "report-1", as_of: "2026-10-03T08:00:00Z", input_snapshot: { positions: [{
      ...position, account_id: "account-1", quantity: "1", currency: "USD",
    }] } } },
  });
  assert.ok(html.indexOf("decision-summary") < html.indexOf("portfolio-conclusion"));
  assert.ok(html.indexOf("portfolio-conclusion") < html.indexOf("report-account-groups"));
  assert.ok(html.includes(">Advice<"));
  assert.match(stylesheet, /\.decision-desk \.decision-summary \{ order: 1; \}/);
  assert.match(stylesheet, /\.decision-desk \.portfolio-conclusion \{ order: 2; \}/);
  assert.match(stylesheet, /\.decision-desk \.report-account-groups \{ order: 3; \}/);
  assert.ok(html.includes("Cross-market transmission."));
  assert.ok(html.includes("Long-term account"));
  assert.ok(html.includes("Verified diagnosis for Acme."));
});

test("holding cards expose only the seven requested analysis sections", () => {
  setLanguage("en");
  const html = render(PositionDiagnosisCard, { position, finding: diagnosis() });
  for (const label of ["Diagnosis", "Current company status", "Company and product news", "Future expectations", "Upside catalysts", "Downside pressures", "Core risks"]) {
    assert.ok(html.includes("<h4>" + label + "</h4>"), label);
  }
  for (const hidden of ["Hidden macro detail", "Hidden industry detail", "Hidden pricing detail", "Hidden gap detail", "report-position-facts", "diagnostic-dimensions"]) {
    assert.ok(!html.includes(hidden), hidden);
  }
  assert.ok(html.includes("Current company operating status."));
  assert.ok(html.includes("A verified core risk."));
});

test("company feed is newest first, deduplicated and bound to the exact stock", () => {
  const finding = diagnosis({ evidence: [
    evidence(1), evidence(2), evidence(3), evidence(4), evidence(5), evidence(6),
    evidence(6, { id: "duplicate" }), evidence(7, { related_instrument_id: "other-stock" }),
    evidence(7, { coverage_scope: "industry" }), evidence(7, { published_at: null }),
  ] });
  const before = JSON.stringify(finding);
  assert.deepEqual(companyNewsForPosition(position, finding).map((row) => row.id), ["news-6", "news-5", "news-4", "news-3", "news-2", "news-1"]);
  assert.equal(JSON.stringify(finding), before);
  setLanguage("en");
  const html = render(PositionDiagnosisCard, { position, finding });
  assert.ok(html.indexOf("Company update 6") < html.indexOf("Company update 5"));
  assert.ok(html.includes("Issuer IR"));
  assert.match(html, /dateTime="2026-10-06T08:00:00Z"/i);
  assert.ok(html.includes("Show 1 more updates"));
  assert.ok(!html.includes("Company update 7"));
  assert.ok(html.includes('href="https://example.com/news/1"'));
});

test("missing research stays explicit and Chinese labels still work", () => {
  setLanguage("en");
  const missing = render(PositionDiagnosisCard, { position, finding: null });
  assert.ok(missing.includes("This holding has no validated individual diagnosis"));
  assert.ok(!missing.includes("Verified diagnosis for Acme."));
  setLanguage("zh");
  const chinese = render(PositionDiagnosisCard, { position, finding: diagnosis() });
  assert.ok(chinese.includes("当前公司状态"));
  assert.ok(chinese.includes("公司与产品动态"));
});
