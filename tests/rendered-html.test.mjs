import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("builds the Alpha Echo product shell", async () => {
  const [html, source] = await Promise.all([
    readFile(new URL("../dist/index.html", import.meta.url), "utf8"),
    readFile(new URL("../app/intelligence-desk.tsx", import.meta.url), "utf8"),
  ]);

  assert.match(html, /<title>Alpha Echo · 跨市场交易情报<\/title>/i);
  assert.match(html, /<div id="root"><\/div>/);
  assert.match(source, /今日决策台/);
  assert.match(source, /持仓工作台/);
  assert.match(source, /市场情报/);
  assert.match(source, /报告记录/);
  assert.doesNotMatch(html, /codex-preview|cloudflare|wrangler|vinext/i);
});

test("states the research and execution safety boundaries", async () => {
  const source = await readFile(
    new URL("../app/intelligence-desk.tsx", import.meta.url),
    "utf8",
  );
  assert.match(source, /只读研究/);
  assert.match(source, /不执行交易/);
  assert.match(source, /历史报告不可重新生成/);
});

test("keeps each tab focused on essential controls instead of decorative heroes", async () => {
  const source = await readFile(
    new URL("../app/intelligence-desk.tsx", import.meta.url),
    "utf8",
  );

  assert.doesNotMatch(source, /decision-strip/);
  assert.doesNotMatch(source, /portfolio-headline/);
  assert.doesNotMatch(source, /手工生成与自动任务共享完整生命周期/);
  assert.match(source, /briefing-toolbar/);
  assert.match(source, /研究执行器未配置/);
});

test("exposes the trade-driven portfolio workflow in the Web UI", async () => {
  const source = await readFile(
    new URL("../app/intelligence-desk.tsx", import.meta.url),
    "utf8",
  );

  assert.match(source, /记一笔买入或卖出/);
  assert.match(source, /总持仓、平均成本和已实现盈亏由系统计算/);
  assert.match(source, /确认买入/);
  assert.match(source, /确认卖出/);
  assert.match(source, /按账户分组的当前持仓/);
  assert.match(source, /账户市值/);
  assert.match(source, /最近成交/);
  assert.match(source, /撤销错误成交/);
  assert.match(source, /恢复成交前持仓/);
  assert.match(source, /编辑逻辑/);
  assert.match(source, /逻辑补全队列/);
  assert.match(source, /连续补全持仓逻辑/);
  assert.match(source, /为什么继续持有？/);
  assert.match(source, /什么情况说明判断错了？/);
  assert.match(source, /计划持有多久？/);
  assert.match(source, /确认并启用，然后处理下一项/);
  assert.match(source, /模型已根据经验证诊断起草/);
  assert.match(source, /采用研究草稿/);
  assert.match(source, /确认并启用持仓逻辑/);
  assert.match(source, /intent\.status === "active" && intent\.instrument_ids\.includes/);
  assert.match(source, /账户与标的设置/);
  assert.match(source, /<option value="RMB">RMB<\/option><option value="HKD">HKD<\/option><option value="USD">USD<\/option>/);
  assert.match(source, /market === "hk"\) return "HKD"/);
  assert.match(source, /function EquityMarketOptions/);
  assert.match(source, /记账币种/);
  assert.match(source, /useState\(\{ name: "", market: "a_share", broker: "" \}\)/);
  assert.doesNotMatch(source, /name: "", market: "a_share", currency:/);
  assert.doesNotMatch(source, /CSV 导入/);
  assert.doesNotMatch(source, /创建新版本/);
  assert.match(source, /仓位切片/);
});

test("records a trade without asking the user to edit a position snapshot", async () => {
  const source = await readFile(
    new URL("../app/intelligence-desk.tsx", import.meta.url),
    "utf8",
  );

  assert.match(source, /股票代码不存在时会自动建立/);
  assert.match(source, /成交价/);
  assert.match(source, /现价将由行情源自动更新/);
  assert.match(source, /api\/trades/);
  assert.match(source, /api\/market-data\/refresh/);
  assert.match(source, /刷新现价/);
  assert.match(source, /market_price_as_of/);
  assert.doesNotMatch(source, /补充当前价/);
  assert.ok(source.indexOf("<TradeComposer") < source.indexOf("account-position-groups"));
});

test("exposes traceable research and immutable briefing details", async () => {
  const source = await readFile(
    new URL("../app/intelligence-desk.tsx", import.meta.url),
    "utf8",
  );

  assert.match(source, /已验证研究/);
  assert.match(source, /明线、暗线与资金流/);
  assert.match(source, /Research Run/);
  assert.match(source, /source_finding_ids/);
  assert.match(source, /不可变报告/);
  assert.match(source, /来源快照 Adapter/);
  assert.match(source, /diagnostic_dimensions/);
  assert.match(source, /未来预期/);
  assert.match(source, /预期差/);
  assert.match(source, /缺少有效 Trade Intent；本诊断不判断逻辑加强或削弱/);
  assert.match(source, /缺少持仓逻辑/);
  assert.match(source, /上行催化剂/);
  assert.match(source, /下行压力/);
  assert.match(source, /核心风险/);
  assert.match(source, /公司与产品动态/);
  assert.match(source, /当前公司状态/);
  assert.match(source, /company-news-feed/);
  assert.match(source, /activeStockResearchMarkets/);
  assert.match(source, /宏观与持仓诊断/);
  assert.match(source, /宏观与跨市场判断/);
  assert.match(source, /组合结论、条件式建议与风险预警/);
  assert.match(source, /briefing_id=/);
  assert.doesNotMatch(source, /生成早报|生成晚报|生成新 Revision/);
});

test("shows live checkpoint progress for the dynamic stock research plan", async () => {
  const source = await readFile(
    new URL("../app/intelligence-desk.tsx", import.meta.url),
    "utf8",
  );

  assert.match(source, /completed_research_tasks/);
  assert.match(source, /current_research_tasks/);
  assert.match(source, /已完成/);
  assert.match(source, /Math\.max\(briefing\.total_research_tasks, 1\)/);
  assert.match(source, /t\("当前任务"\)\} \{current\} \/ \{total\}/);
  assert.match(source, /await refresh\(\)/);
  assert.doesNotMatch(source, /setInterval/);
  assert.match(source, /displayedDetail/);
  assert.match(source, /selectedDetailId/);
  assert.match(source, /正在合成最终报告/);
  assert.match(source, /function BriefingDocument/);
  assert.match(source, /查看结构化条目与研究引用/);
  assert.doesNotMatch(source, /<pre className="briefing-markdown"/);
});

test("lets the user pause automatic daily reports without disabling manual reports", async () => {
  const source = await readFile(
    new URL("../app/intelligence-desk.tsx", import.meta.url),
    "utf8",
  );

  assert.match(source, /暂停自动日报/);
  assert.match(source, /恢复自动日报/);
  assert.match(source, /手动生成报告不受影响/);
});

test("removes the retired Meme arena without removing stock workflows", async () => {
  const [source, routes, entrypoint, config, packageJson, migration, briefings, portfolio, settings] = await Promise.all([
    readFile(new URL("../app/intelligence-desk.tsx", import.meta.url), "utf8"),
    readFile(new URL("../backend/src/http.rs", import.meta.url), "utf8"),
    readFile(new URL("../backend/src/main.rs", import.meta.url), "utf8"),
    readFile(new URL("../backend/src/config.rs", import.meta.url), "utf8"),
    readFile(new URL("../package.json", import.meta.url), "utf8"),
    readFile(new URL("../backend/migrations/0026_remove_meme_arena.sql", import.meta.url), "utf8"),
    readFile(new URL("../backend/src/briefings.rs", import.meta.url), "utf8"),
    readFile(new URL("../backend/src/portfolio.rs", import.meta.url), "utf8"),
    readFile(new URL("../backend/src/intelligence.rs", import.meta.url), "utf8"),
  ]);
  assert.doesNotMatch(source, /MemeArena|memeReports|api\/meme|api\/wallets|api\/tracked-wallets|GMGN|Smart Money|value="meme"/);
  assert.doesNotMatch(routes, /meme::router|adapters::router/);
  assert.doesNotMatch(entrypoint, /meme::|config.meme/);
  assert.doesNotMatch(config, /MemeRunnerConfig|GMGN_/);
  assert.doesNotMatch(packageJson, /gmgn-cli/);
  assert.doesNotMatch(portfolio, /route\("\/api\/(tracked-wallets|wallets)/);
  assert.doesNotMatch(settings, /settings\/meme|gmgn_runner|meme_report_schedule/);
  assert.doesNotMatch(briefings, /FROM tracked_wallets|JOIN wallets/);
  assert.match(briefings, /WHERE ps.as_of <= \$1 AND ps.account_id IS NOT NULL/);
  assert.match(briefings, /WHERE i.market IN \('a_share', 'hk', 'us'\)/);
  assert.match(portfolio, /validate_new_instrument_market\(input.market\)\?/);
  assert.match(portfolio, /validate_stock_snapshot\(&input\)\?/);
  assert.doesNotMatch(portfolio, /JOIN wallets/);
  assert.match(briefings, /source_type <> 'wallet_portfolio'/);
  assert.match(briefings, /'us', 'cross', 'portfolio'/);
  for (const table of ["meme_token_blacklist", "meme_token_observations", "meme_report_runs"]) {
    assert.ok(migration.includes("DROP TABLE " + table));
  }
  assert.match(source, /function TradeComposer/);
  assert.match(source, /function SettingsView/);
  assert.match(source, /function BriefingsView/);
  assert.match(source, /className="language-toggle"/);
});
