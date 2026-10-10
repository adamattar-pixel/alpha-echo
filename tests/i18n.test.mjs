import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";
import english from "../app/locales/en.json" with { type: "json" };
import { enumLabel, feedback, setLanguage, translate } from "../app/i18n.mjs";

test("translates labels and interpolated counts without changing unknown content", () => {
  assert.equal(translate("持仓工作台", {}, "en"), "Holdings");
  assert.equal(translate("持仓工作台", {}, "zh"), "持仓工作台");
  assert.equal(translate("已完成 {0} / {1}", { 0: 4, 1: 18 }, "en"), "Completed 4 / 18");
  assert.equal(translate("已完成 {0} / {1}", { 0: 0, 1: 18 }, "zh"), "已完成 0 / 18");
  assert.equal(translate("我的自定义持仓逻辑", {}, "en"), "我的自定义持仓逻辑");
});

test("visible feedback follows language changes, including counts and trade sides", () => {
  setLanguage("en");
  assert.equal(feedback("买入已记录，总持仓与平均成本已自动更新。"), "Buy recorded. Holdings and average cost updated automatically.");
  const savedMessage = translate("账户已建立，现在可以直接记买入。");
  setLanguage("zh");
  assert.equal(feedback(savedMessage), "账户已建立，现在可以直接记买入。");
});

test("persists the language and restores it in a fresh session", async () => {
  const saved = new Map();
  globalThis.window = { localStorage: { getItem: (key) => saved.get(key), setItem: (key, value) => saved.set(key, value) } };
  try {
    const first = await import("../app/i18n.mjs?test=persistence-first");
    let notifications = 0;
    const unsubscribe = first.subscribeLanguage(() => { notifications += 1; });
    first.setLanguage("en");
    first.setLanguage("en");
    assert.equal(notifications, 1);
    assert.equal(saved.get("alpha-echo.language"), "en");
    const reloaded = await import("../app/i18n.mjs?test=persistence-reload");
    assert.equal(reloaded.getLanguage(), "en");
    assert.equal(reloaded.getLocale(), "en-US");
    unsubscribe();
    first.setLanguage("zh");
    assert.equal(notifications, 1);
  } finally {
    delete globalThis.window;
  }
});

test("localizes known domain labels while retaining custom values", () => {
  setLanguage("en");
  assert.equal(enumLabel("strengthened"), "Thesis strengthened");
  assert.equal(enumLabel("medium"), "Medium term");
  assert.equal(enumLabel("passed"), "Validated");
  setLanguage("zh");
  assert.equal(enumLabel("strengthened"), "逻辑加强");
  assert.equal(enumLabel("medium"), "中期");
  assert.equal(enumLabel("dominant"), "主导");
  for (const [value, label] of Object.entries({ invalid: "未通过验证", partial: "部分完成", not_run: "未运行", valid_empty: "验证通过，无结果", regime: "宏观环境", event_calendar: "事件日历" })) {
    assert.equal(enumLabel(value), label);
  }
  assert.equal(enumLabel("my-custom-strategy"), "my-custom-strategy");
});

test("switching still works with blocked or corrupt browser storage", async () => {
  globalThis.window = { get localStorage() { throw new Error("Storage blocked"); } };
  try {
    const blocked = await import("../app/i18n.mjs?test=blocked-storage");
    assert.equal(blocked.getLanguage(), "zh");
    assert.doesNotThrow(() => blocked.setLanguage("en"));
    assert.equal(blocked.getLanguage(), "en");
    blocked.setLanguage("invalid");
    assert.equal(blocked.getLanguage(), "en");
    globalThis.window = { localStorage: { getItem: () => "invalid" } };
    const corrupt = await import("../app/i18n.mjs?test=corrupt-storage");
    assert.equal(corrupt.getLanguage(), "zh");
  } finally {
    delete globalThis.window;
  }
});

test("every literal interface translation has a nonempty English entry", async () => {
  const source = await readFile(new URL("../app/intelligence-desk.tsx", import.meta.url), "utf8");
  const tree = ts.createSourceFile("desk.tsx", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const missing = new Set();
  function visit(node) {
    if (ts.isCallExpression(node) && node.expression.getText(tree) === "t" && ts.isStringLiteral(node.arguments[0])) {
      const key = node.arguments[0].text;
      if (!english[key]?.trim()) missing.add(key);
    }
    ts.forEachChild(node, visit);
  }
  visit(tree);
  assert.deepEqual([...missing], []);
});
