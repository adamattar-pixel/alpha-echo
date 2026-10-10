import english from "./locales/en.json" with { type: "json" };

const storageKey = "alpha-echo.language";
let language = "zh";
try {
  if (typeof window !== "undefined" && window.localStorage.getItem(storageKey) === "en") language = "en";
} catch { /* Language switching still works when browser storage is unavailable. */ }
const listeners = new Set();

export function getLanguage() { return language; }
export function getLocale() { return language === "en" ? "en-US" : "zh-CN"; }
export function subscribeLanguage(listener) {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}
export function setLanguage(next) {
  if (next !== "en" && next !== "zh") return;
  if (next === language) return;
  language = next;
  try { window.localStorage.setItem(storageKey, next); } catch { /* Session preference remains available. */ }
  listeners.forEach((listener) => listener());
}

/** Translate interface strings only; report text and user-entered values stay intact. */
export function translate(key, values = {}, selectedLanguage = language) {
  if (typeof key !== "string") return "";
  const template = selectedLanguage === "en" ? english[key] ?? key : key;
  return template.replace(/\{(\d+)\}/g, (match, index) => String(values[index] ?? match));
}

export const t = translate;

const enumLabels = {
  strengthened: "逻辑加强", weakened: "逻辑削弱", invalidated: "已失效", unchanged: "不变",
  short: "短期", swing: "波段", medium: "中期", long: "长期", position: "持仓策略",
  passed: "通过验证", failed: "失败", pending: "等待中", valid: "通过验证",
  invalid: "未通过验证", partial: "部分完成", not_run: "未运行", valid_empty: "验证通过，无结果",
  emerging: "萌芽", forming: "形成中", accelerating: "加速", dominant: "主导",
  weakening: "走弱", fading: "退潮", mature: "成熟", early: "早期", seed: "萌芽",
  equity_position_diagnosis: "股票持仓诊断", macro_context: "宏观背景",
  market_context: "市场背景", portfolio_construction: "组合构建",
  regime: "宏观环境", event_calendar: "事件日历",
};
export function enumLabel(value) { return t(enumLabels[value] ?? value); }
const feedbackSources = new Map(Object.entries(english).map(([source, translation]) => [translation, source]));

// Feedback is stored in its source language so an already-visible message follows
// the toggle too. Interpolated feedback is limited to these application messages.
export function feedback(message) {
  for (const key of ["{0}已记录，总持仓与平均成本已自动更新。"]) {
    const parts = key.split(/\{\d+\}/);
    if (message.startsWith(parts[0]) && message.endsWith(parts[1])) {
      const value = message.slice(parts[0].length, message.length - parts[1].length);
      return t(key, { 0: key.startsWith("{0}") ? t(value) : value });
    }
  }
  return t(feedbackSources.get(message) ?? message);
}
