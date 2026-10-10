import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import type { FormEvent, ReactNode } from "react";
import { getLanguage, getLocale, setLanguage, subscribeLanguage, t, feedback, enumLabel } from "./i18n.mjs";

type Account = {
  id: string;
  name: string;
  market: string;
  currency: string;
};

type Instrument = {
  id: string;
  market: string;
  symbol: string;
  name: string;
  currency: string | null;
  exchange: string | null;
  chain: string | null;
};

type DashboardSummary = {
  positions: number;
  active_intents: number;
  pending_briefings: number;
  last_snapshot_at: string | null;
  research_runner: string;
};

type Position = {
  snapshot_id: string;
  position_snapshot_item_id: string;
  as_of: string;
  owner_id: string;
  owner_name: string;
  owner_type: string;
  instrument_id: string;
  market: string;
  currency: string | null;
  symbol: string;
  instrument_name: string;
  quantity: string;
  average_cost: string | null;
  market_price: string | null;
  market_price_as_of: string | null;
  market_value: string | null;
  unrealized_pnl: string | null;
};

type PositionChange = {
  owner_id: string;
  owner_name: string;
  owner_type: string;
  instrument_id: string;
  market: string;
  symbol: string;
  instrument_name: string;
  previous_quantity: string;
  current_quantity: string;
  quantity_delta: string;
  change_type: "opened" | "increased" | "reduced" | "closed" | "reversed";
  current_as_of: string;
  previous_as_of: string;
};

type Trade = {
  id: string;
  owner_id: string;
  owner_name: string;
  instrument_id: string;
  market: string;
  currency: string | null;
  symbol: string;
  instrument_name: string;
  side: "buy" | "sell";
  quantity: string;
  price: string | null;
  realized_pnl: string | null;
  position_quantity_after: string | null;
  average_cost_after: string | null;
  executed_at: string;
  voided_at: string | null;
  can_void: boolean;
};

type TradeIntent = {
  id: string;
  title: string;
  strategy_type: string;
  time_horizon: string;
  status: string;
  version: number;
  thesis: string;
  risk_budget: string | null;
  target_position: string | null;
  catalysts: string[];
  invalidation_conditions: string[];
  exit_principles: string[];
  notes: string | null;
  instrument_ids: string[];
  account_id: string | null;
  wallet_id: string | null;
  owner_name: string;
  position_slice: string | null;
};

type Briefing = {
  id: string;
  kind: "report";
  status: "pending" | "running" | "complete" | "partial" | "failed";
  as_of: string;
  revision: number;
  rendered_markdown: string | null;
  data_gaps: string[];
  supersedes_id: string | null;
  skill_name: string | null;
  skill_version: string | null;
  model: string | null;
  validation_status: string | null;
  completed_research_tasks: number;
  total_research_tasks: number;
  current_research_tasks: string[];
};

type ResearchRun = {
  id: string;
  market: string;
  run_type: string;
  status: string;
  as_of: string;
  skill_name: string;
  skill_version: string;
  supporting_skills: Record<string, string>;
  model: string | null;
  validation_status: string | null;
  data_gaps: string[];
  error: string | null;
};

type EvidenceSource = {
  id: string;
  source_type: string;
  coverage_scope: "position" | "company" | "product" | "industry" | "market" | "macro" | null;
  related_instrument_id: string | null;
  frozen_object_type: string | null;
  frozen_object_id: string | null;
  title: string;
  url: string | null;
  publisher: string | null;
  published_at: string | null;
  observed_at: string;
  excerpt: string | null;
};

type ResearchFinding = {
  id: string;
  finding_type: string;
  market: string;
  affected_object_type: string | null;
  affected_object_id: string | null;
  affected_instrument_id: string | null;
  affected_account_id: string | null;
  affected_wallet_id: string | null;
  draft_as_of: string | null;
  draft_position_current: boolean;
  fact_summary: string;
  inference: string | null;
  impact_direction: string | null;
  impact_strength: number | null;
  time_horizon: string | null;
  confidence: string;
  conditional_action: string | null;
  confirmation_conditions: string[];
  invalidation_conditions: string[];
  data_gaps: string[];
  skill_name: string;
  skill_version: string;
  evidence: EvidenceSource[];
  diagnostic_dimensions: {
    intent_assessment?: "matched" | "missing";
    intent_draft?: TradeIntentDraftSuggestion | null;
    macro_context: string;
    industry_context: string;
    company_context: string;
    market_pricing: string;
    future_expectation: string;
    expectation_gap: string;
    upside_catalysts: string[];
    downside_pressures: string[];
    core_risks: string[];
  } | null;
};

type TradeIntentDraftSuggestion = {
  thesis: string;
  time_horizon: "short" | "swing" | "medium" | "long";
  catalysts: string[];
  invalidation_conditions: string[];
};

type ResearchIntentSuggestion = {
  finding_id: string;
  position_snapshot_item_id: string;
  as_of: string | null;
  draft: TradeIntentDraftSuggestion;
  modified?: boolean;
};

type MarketNarrative = {
  id: string;
  market: string;
  name: string;
  narrative_type: "explicit" | "latent";
  as_of: string;
  stage: string;
  summary: string;
  confidence: string;
  capital_flow_inference: string | null;
};

type IntelligenceData = {
  runs: ResearchRun[];
  findings: ResearchFinding[];
  narratives: MarketNarrative[];
  transmission_paths: Array<{ id: string; source_driver: string; target_market: string; direction: string; confidence: string; steps: { path: string[] } }>;
};

type SourceSnapshot = {
  id: string;
  provider: string;
  source_type: string;
  captured_at: string;
  as_of: string;
  checksum: string;
};

type AppSettings = {
  briefing_schedule: { timezone: string; time: string; enabled: boolean; language?: "en" | "zh" };
  research_runner: string;
  source_snapshots: number;
};

type LoadState = "loading" | "ready" | "offline";
type View = "overview" | "portfolio" | "intelligence" | "briefings" | "settings";
type Theme = "light" | "dark";

const API_BASE = import.meta.env.VITE_API_BASE_URL ?? "http://127.0.0.1:8088";
const THEME_STORAGE_KEY = "alpha-echo.theme";

function getInitialTheme(): Theme {
  if (typeof window === "undefined") return "light";
  try {
    return window.localStorage.getItem(THEME_STORAGE_KEY) === "dark" ? "dark" : "light";
  } catch {
    return "light";
  }
}

const marketLabels: Record<string, string> = {
  a_share: "A 股",
  hk: "港股",
  us: "美股",
};

const statusLabels: Record<string, string> = {
  pending: "等待中",
  running: "生成中",
  complete: "已完成",
  partial: "部分完成",
  failed: "失败",
  active: "有效",
  draft: "草稿",
  paused: "暂停",
  invalidated: "已失效",
  closed: "已关闭",
};

const changeLabels: Record<PositionChange["change_type"], string> = {
  opened: "建仓",
  increased: "加仓",
  reduced: "减仓",
  closed: "清仓",
  reversed: "反向",
};

function formatTime(value: string | null) {
  if (!value) return t("尚无快照");
  return new Intl.DateTimeFormat(getLocale(), {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(new Date(value));
}

function reportDate(value: string) {
  return new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Shanghai",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date(value));
}

function todayInShanghai() {
  return reportDate(new Date().toISOString());
}

function formatNumber(value: string | null, digits = 2) {
  if (value === null) return "—";
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return value;
  return new Intl.NumberFormat(getLocale(), { maximumFractionDigits: digits }).format(parsed);
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers: { "content-type": "application/json", ...init?.headers },
  });
  if (!response.ok) {
    const payload = await response.json().catch(() => null) as { error?: string } | null;
    throw new Error(payload?.error ?? `request failed: ${response.status}`);
  }
  return response.json() as Promise<T>;
}

async function requestAllBriefings() {
  const reports: Briefing[] = [];
  const pageSize = 100;
  for (let offset = 0; ; offset += pageSize) {
    const page = await request<Briefing[]>(`/api/briefings?limit=${pageSize}&offset=${offset}`);
    reports.push(...page);
    if (page.length < pageSize) return reports;
  }
}

export function IntelligenceDesk() {
  const language = useSyncExternalStore(subscribeLanguage, getLanguage, () => "zh");
  const [theme, setTheme] = useState<Theme>(getInitialTheme);
  useEffect(() => {
    document.documentElement.lang = language === "en" ? "en" : "zh-CN";
    document.title = t("Alpha Echo · 跨市场交易情报");
  }, [language]);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, theme);
    } catch {
      // The interface remains usable when storage is unavailable.
    }
  }, [theme]);
  const [view, setView] = useState<View>("portfolio");
  const [loadState, setLoadState] = useState<LoadState>("loading");
  const [summary, setSummary] = useState<DashboardSummary>({
    positions: 0,
    active_intents: 0,
    pending_briefings: 0,
    last_snapshot_at: null,
    research_runner: "not_configured",
  });
  const [positions, setPositions] = useState<Position[]>([]);
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [instruments, setInstruments] = useState<Instrument[]>([]);
  const [positionChanges, setPositionChanges] = useState<PositionChange[]>([]);
  const [trades, setTrades] = useState<Trade[]>([]);
  const [intents, setIntents] = useState<TradeIntent[]>([]);
  const [briefings, setBriefings] = useState<Briefing[]>([]);
  const [intelligence, setIntelligence] = useState<IntelligenceData>({ runs: [], findings: [], narratives: [], transmission_paths: [] });
  const [reportIntelligence, setReportIntelligence] = useState<IntelligenceData>({ runs: [], findings: [], narratives: [], transmission_paths: [] });
  const [selectedReportId, setSelectedReportId] = useState<string | null>(null);
  const [selectedReportDate, setSelectedReportDate] = useState(todayInShanghai);
  const [reportSelectionMode, setReportSelectionMode] = useState<"auto" | "manual">("auto");
  const [selectedReportDetail, setSelectedReportDetail] = useState<BriefingDetail | null>(null);
  const [loadedReportId, setLoadedReportId] = useState<string | null>(null);
  const [sourceSnapshots, setSourceSnapshots] = useState<SourceSnapshot[]>([]);
  const [settings, setSettings] = useState<AppSettings>({ briefing_schedule: { timezone: "Asia/Shanghai", time: "19:00", enabled: false, language: "en" }, research_runner: "not_configured", source_snapshots: 0 });
  const [creatingBriefing, setCreatingBriefing] = useState(false);
  const [refreshingQuotes, setRefreshingQuotes] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [nextSummary, nextPositions, nextIntents, nextBriefings, nextAccounts, nextInstruments, nextChanges, nextTrades, nextIntelligence, nextSources, nextSettings] = await Promise.all([
        request<DashboardSummary>("/api/dashboard"),
        request<Position[]>("/api/portfolio/positions"),
        request<TradeIntent[]>("/api/trade-intents"),
        requestAllBriefings(),
        request<Account[]>("/api/accounts"),
        request<Instrument[]>("/api/instruments"),
        request<PositionChange[]>("/api/portfolio/changes"),
        request<Trade[]>("/api/trades?limit=24"),
        request<IntelligenceData>("/api/intelligence?limit=100"),
        request<SourceSnapshot[]>("/api/data-sources/snapshots?limit=12"),
        request<AppSettings>("/api/settings"),
      ]);
      setSummary(nextSummary);
      setPositions(nextPositions);
      setIntents(nextIntents);
      setBriefings(nextBriefings);
      setAccounts(nextAccounts);
      setInstruments(nextInstruments);
      setPositionChanges(nextChanges);
      setTrades(nextTrades);
      setIntelligence(nextIntelligence);
      setSourceSnapshots(nextSources);
      setSettings(nextSettings);
      setLoadState("ready");
    } catch {
      setLoadState("offline");
    }
  }, []);

  const automaticReport = useMemo(() => {
    const terminal = briefings.filter((report) => report.status === "complete" || report.status === "partial");
    const today = todayInShanghai();
    return terminal.find((report) => reportDate(report.as_of) === today) ?? terminal[0] ?? briefings[0] ?? null;
  }, [briefings]);
  const effectiveReportId = reportSelectionMode === "auto" ? automaticReport?.id ?? null : selectedReportId;
  const effectiveReportDate = reportSelectionMode === "auto" && automaticReport ? reportDate(automaticReport.as_of) : selectedReportDate;
  const effectiveReportDetail = loadedReportId === effectiveReportId ? selectedReportDetail : null;
  const effectiveReportIntelligence: IntelligenceData = loadedReportId === effectiveReportId
    ? reportIntelligence
    : { runs: [], findings: [], narratives: [], transmission_paths: [] };

  useEffect(() => {
    if (!effectiveReportId) return;
    let cancelled = false;
    void Promise.all([
      request<BriefingDetail>(`/api/briefings/${effectiveReportId}`),
      request<IntelligenceData>(`/api/intelligence?limit=300&briefing_id=${effectiveReportId}`),
    ]).then(([detail, scopedIntelligence]) => {
      if (!cancelled) {
        setSelectedReportDetail(detail);
        setReportIntelligence(scopedIntelligence);
        setLoadedReportId(effectiveReportId);
      }
    }).catch(() => {
      if (!cancelled) setActionError("所选报告的历史研究数据读取失败。");
    });
    return () => { cancelled = true; };
  }, [effectiveReportId, briefings]);

  useEffect(() => {
    const timeout = window.setTimeout(() => void refresh(), 0);
    return () => window.clearTimeout(timeout);
  }, [refresh]);

  const hasActiveBriefing = briefings.some((briefing) => briefing.status === "pending" || briefing.status === "running");
  useEffect(() => {
    if (!hasActiveBriefing) return;
    let cancelled = false;
    let timeout: number;
    const poll = async () => {
      await refresh();
      if (!cancelled) timeout = window.setTimeout(() => void poll(), 3000);
    };
    timeout = window.setTimeout(() => void poll(), 3000);
    return () => {
      cancelled = true;
      window.clearTimeout(timeout);
    };
  }, [hasActiveBriefing, refresh]);

  async function createBriefing() {
    setCreatingBriefing(true);
    setActionError(null);
    try {
      await request("/api/briefings", { method: "POST", body: JSON.stringify({ language: getLanguage() }) });
      setReportSelectionMode("auto");
      setSelectedReportDate(todayInShanghai());
      await refresh();
      setView("overview");
      window.setTimeout(() => void refresh(), 2500);
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "报告任务创建失败，请确认本地后端与数据库正在运行。");
    } finally {
      setCreatingBriefing(false);
    }
  }

  function selectReportDate(date: string) {
    setReportSelectionMode("manual");
    setSelectedReportDate(date);
    const report = briefings.find((item) => reportDate(item.as_of) === date && (item.status === "complete" || item.status === "partial"))
      ?? briefings.find((item) => reportDate(item.as_of) === date)
      ?? null;
    setSelectedReportId(report?.id ?? null);
  }

  function selectReport(id: string) {
    const report = briefings.find((item) => item.id === id);
    setReportSelectionMode("manual");
    setSelectedReportId(id || null);
    if (report) setSelectedReportDate(reportDate(report.as_of));
  }

  async function refreshQuotes() {
    setRefreshingQuotes(true);
    setActionError(null);
    try {
      const result = await request<{ requested: number; fetched: number; failures: Array<{ symbol: string; reason: string }> }>("/api/market-data/refresh", { method: "POST" });
      await refresh();
      if (result.requested > 0 && result.fetched === 0) {
        setActionError(result.failures[0] ? `${result.failures[0].symbol}：${result.failures[0].reason}` : "行情源没有返回可用现价。");
      }
    } catch {
      setActionError("自动行情刷新失败，请稍后重试。");
    } finally {
      setRefreshingQuotes(false);
    }
  }

  return (
    <main className="app-shell" data-theme={theme}>
      <aside className="sidebar">
        <div className="brand-block">
          <span className="brand-mark" aria-hidden="true">α</span>
          <div>
            <strong>Alpha Echo</strong>
            <span>{t("投资组合情报")}</span>
          </div>
        </div>

        <nav className="primary-nav" aria-label={t("主导航")}>
          <NavButton active={view === "overview"} onClick={() => setView("overview")} icon="⌁" label={t("今日总览")} />
          <NavButton active={view === "portfolio"} onClick={() => setView("portfolio")} icon="▥" label={t("持仓工作台")} />
          <NavButton active={view === "intelligence"} onClick={() => setView("intelligence")} icon="◎" label={t("市场情报")} />
          <NavButton active={view === "briefings"} onClick={() => setView("briefings")} icon="≡" label={t("报告记录")} />
          <NavButton active={view === "settings"} onClick={() => setView("settings")} icon="⌘" label={t("系统设置")} />
        </nav>

        <div className="system-card">
          <span className={`status-dot ${loadState}`} />
          <div>
            <strong>{loadState === "ready" ? t("事实库已连接") : loadState === "loading" ? t("正在连接") : t("后端未启动")}</strong>
            <span>{loadState === "ready" ? t("PostgreSQL · 本机") : t("数据未被模拟")}</span>
          </div>
        </div>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">
              {view === "overview" || view === "intelligence" || view === "briefings"
                ? t("报告截止 · {0}", { 0: formatTime(effectiveReportDetail?.run.as_of ?? null) })
                : t("持仓截至 · {0}", { 0: formatTime(summary.last_snapshot_at) })}
            </p>
            <h1>{viewTitle(view)}</h1>
          </div>
          <div className="topbar-actions">
            <button
              type="button"
              className="theme-toggle"
              aria-label={theme === "light" ? t("切换至暗夜模式") : t("切换至日间模式")}
              aria-pressed={theme === "dark"}
              onClick={() => setTheme((current) => current === "light" ? "dark" : "light")}
            >
              <span aria-hidden="true">{theme === "light" ? "◐" : "☼"}</span>
              {theme === "light" ? t("暗夜") : t("日间")}
            </button>
            <div className="language-toggle" role="group" aria-label={t("语言")}>
              <button type="button" lang="en" aria-label="English" aria-pressed={language === "en"} onClick={() => setLanguage("en")}>EN</button>
              <span aria-hidden="true">|</span>
              <button type="button" lang="zh-CN" aria-label="中文" aria-pressed={language === "zh"} onClick={() => setLanguage("zh")}>中文</button>
            </div>
            <button className="quiet-button" onClick={() => void refreshQuotes()} disabled={loadState === "loading" || refreshingQuotes}>
              {refreshingQuotes ? t("获取行情中…") : t("刷新现价")}
            </button>
            <button className="primary-button" onClick={() => { setView("portfolio"); window.setTimeout(() => document.getElementById("trade-composer")?.scrollIntoView({ behavior: "smooth", block: "start" }), 0); }} disabled={loadState !== "ready"}>
              {t("记一笔交易")}</button>
          </div>
        </header>

        {loadState === "offline" && <OfflineNotice />}
        {actionError ? <div className="action-error" role="alert">{feedback(actionError)}</div> : null}

        {view === "overview" && (
          <Overview
            briefings={briefings}
            detail={effectiveReportDetail}
            data={effectiveReportIntelligence}
            selectedDate={effectiveReportDate}
            selectedReportId={effectiveReportId}
            onDateChange={selectReportDate}
            onReportChange={selectReport}
            onCreate={createBriefing}
            disabled={creatingBriefing || hasActiveBriefing || loadState !== "ready"}
          />
        )}
        {view === "portfolio" && (
          <PortfolioView
            positions={positions}
            intents={intents}
            accounts={accounts}
            instruments={instruments}
            changes={positionChanges}
            trades={trades}
            findings={intelligence.findings}
            onCreated={refresh}
          />
        )}
        {view === "intelligence" && <IntelligenceView runner={summary.research_runner} data={effectiveReportIntelligence} briefings={briefings} detail={effectiveReportDetail} selectedDate={effectiveReportDate} selectedReportId={effectiveReportId} onDateChange={selectReportDate} onReportChange={selectReport} onCreate={createBriefing} disabled={creatingBriefing || hasActiveBriefing || loadState !== "ready"} />}
        {view === "briefings" && <BriefingsView briefings={briefings} onCreate={createBriefing} disabled={creatingBriefing || hasActiveBriefing || loadState !== "ready"} />}
        {view === "settings" && <SettingsView key={`${settings.briefing_schedule.time}-${settings.briefing_schedule.enabled}`} sourceSnapshots={sourceSnapshots} settings={settings} onCreated={refresh} />}
      </section>
    </main>
  );
}

function NavButton({ active, onClick, icon, label }: { active: boolean; onClick: () => void; icon: string; label: string }) {
  return (
    <button className={active ? "nav-item active" : "nav-item"} onClick={onClick} aria-current={active ? "page" : undefined}>
      <span aria-hidden="true">{icon}</span>{t(label)}
    </button>
  );
}

function viewTitle(view: View) {
  return {
    overview: t("今日决策台"),
    portfolio: t("持仓工作台"),
    intelligence: t("市场情报"),
    briefings: t("报告记录"),
    settings: t("系统设置"),
  }[view];
}

function OfflineNotice() {
  return (
    <div className="offline-notice" role="status">
      <span>{t("本地后端尚未运行")}</span>
      <p>{t("界面不会伪造持仓或研究结论。启动 PostgreSQL 与 Rust 服务后，这里会自动读取事实库。")}</p>
    </div>
  );
}

type ReportViewProps = {
  briefings: Briefing[];
  detail: BriefingDetail | null;
  selectedDate: string;
  selectedReportId: string | null;
  onDateChange: (date: string) => void;
  onReportChange: (id: string) => void;
  onCreate: () => Promise<void>;
  disabled: boolean;
};

function ReportSelector({ briefings, detail, selectedDate, selectedReportId, onDateChange, onReportChange, onCreate, disabled }: ReportViewProps) {
  const reportsForDate = briefings.filter((report) => reportDate(report.as_of) === selectedDate);
  const expired = Boolean(detail && reportDate(detail.run.as_of) !== todayInShanghai());
  return (
    <section className={`report-context ${expired ? "expired" : ""}`} aria-label={t("报告日期")}>
      <div><span>{expired ? t("历史报告 · 已过期") : detail ? t("当前报告") : t("该日期没有报告")}</span><strong>{detail ? t("截止 {0}", { 0: formatTime(detail.run.as_of) }) : selectedDate}</strong></div>
      <label>{t("日期")}<input type="date" value={selectedDate} onChange={(event) => onDateChange(event.target.value)} /></label>
      {reportsForDate.length > 1 ? <label>{t("同日报告")}<select value={selectedReportId ?? ""} onChange={(event) => onReportChange(event.target.value)}>{reportsForDate.map((report) => <option key={report.id} value={report.id}>{formatTime(report.as_of)} · {t(statusLabels[report.status])}</option>)}</select></label> : null}
      <button className="primary-button" disabled={disabled} onClick={() => void onCreate()}>{disabled ? t("报告生成中") : t("生成当前报告")}</button>
    </section>
  );
}

function positionsFromReport(detail: BriefingDetail | null): Position[] {
  const snapshot = detail?.run.input_snapshot as { positions?: Array<Record<string, unknown>> } | undefined;
  return (snapshot?.positions ?? []).filter((row) => ["a_share", "hk", "us"].includes(String(row.market)) && Number(row.quantity ?? 0) !== 0).map((row) => ({
    snapshot_id: String(row.snapshot_id ?? ""),
    position_snapshot_item_id: String(row.position_snapshot_item_id ?? ""),
    as_of: String(row.as_of ?? detail?.run.as_of ?? ""),
    owner_id: String(row.account_id ?? row.wallet_id ?? ""),
    owner_name: String(row.owner_name ?? t("未命名账户")),
    owner_type: String(row.owner_type ?? "account"),
    instrument_id: String(row.instrument_id ?? ""),
    market: String(row.market ?? ""),
    currency: row.currency == null ? null : String(row.currency),
    symbol: String(row.symbol ?? ""),
    instrument_name: String(row.instrument_name ?? row.symbol ?? ""),
    quantity: String(row.quantity ?? "0"),
    average_cost: row.average_cost == null ? null : String(row.average_cost),
    market_price: row.market_price == null ? null : String(row.market_price),
    market_price_as_of: String(row.as_of ?? detail?.run.as_of ?? ""),
    market_value: row.market_value == null ? null : String(row.market_value),
    unrealized_pnl: row.unrealized_pnl == null ? null : String(row.unrealized_pnl),
  }));
}

function findingPositionItemId(finding: ResearchFinding) {
  if (finding.affected_object_type === "position_snapshot_item") return finding.affected_object_id;
  return finding.evidence.find((evidence) => evidence.frozen_object_type === "position_snapshot_item")?.frozen_object_id ?? null;
}

export function Overview(props: ReportViewProps & { data: IntelligenceData }) {
  const positions = positionsFromReport(props.detail);
  const macro = props.data.findings.filter((finding) => finding.market === "macro" || finding.market === "cross");
  const portfolio = props.data.findings.filter((finding) => finding.market === "portfolio");
  const diagnoses = props.data.findings.filter((finding) => finding.finding_type === "equity_position_diagnosis");
  const groups = [...positions.reduce((map, position) => {
    const group = map.get(position.owner_id) ?? { id: position.owner_id, name: position.owner_name, currency: position.currency, positions: [] as Position[] };
    group.positions.push(position);
    map.set(position.owner_id, group);
    return map;
  }, new Map<string, { id: string; name: string; currency: string | null; positions: Position[] }>()).values()];
  return (
    <div className="content-stack decision-desk">
      <ReportSelector {...props} />
      {!props.detail ? <EmptyState title={t("该日期没有已保存的报告")} body={t("历史日期不会重新生成；如需当前判断，请生成一份新的当前报告。")} /> : <>
        <section className="panel decision-summary"><PanelHeading kicker={t("总览")} title={t("宏观与跨市场判断")} />
          {macro.length ? <div className="decision-summary-list">{macro.map((finding) => <article key={finding.id}><span className="market-chip">{t(finding.market === "macro" ? "宏观" : "跨市场")}</span><h3>{finding.fact_summary}</h3>{finding.inference ? <p>{finding.inference}</p> : null}</article>)}</div> : <EmptyState title={t("本报告没有宏观结论")} body={t("报告的数据缺口和运行状态会原样保留。")} />}
        </section>
        <section className="panel portfolio-conclusion"><PanelHeading kicker={t("建议")} title={t("组合结论、条件式建议与风险预警")} />
          {portfolio.length ? <div className="decision-summary-list">{portfolio.map((finding) => <article key={finding.id}><h3>{finding.fact_summary}</h3>{finding.inference ? <p>{finding.inference}</p> : null}{finding.conditional_action ? <div className="conditional-action"><strong>{t("条件式建议")}</strong>{finding.conditional_action}</div> : null}{finding.invalidation_conditions.length ? <ul>{finding.invalidation_conditions.map((risk) => <li key={risk}>{risk}</li>)}</ul> : null}</article>)}</div> : <EmptyState title={t("本报告没有组合结论")} body={t("不会为了填充页面而制造操作建议。")} />}
        </section>
        <section className="report-account-groups" aria-label={t("报告持仓诊断")}>
          {groups.map((group) => <section className="report-account" key={group.id}><header><div><span>{t("账户")}</span><h2>{group.name}</h2></div><small>{group.positions.length} {t("项报告时持仓")}</small></header><div className="report-position-grid">{group.positions.map((position) => <PositionDiagnosisCard key={position.position_snapshot_item_id} position={position} finding={diagnoses.find((finding) => findingPositionItemId(finding) === position.position_snapshot_item_id) ?? null} />)}</div></section>)}
          {!groups.length ? <EmptyState title={t("报告中没有股票持仓")} body={t("决策台只展示报告截止时间冻结的 A股、港股和美股持仓。")} /> : null}
        </section>
      </>}
    </div>
  );
}

export function companyNewsForPosition(position: Position, finding: ResearchFinding | null): EvidenceSource[] {
  const seen = new Set<string>();
  return (finding?.evidence ?? [])
    .filter((item) => item.related_instrument_id === position.instrument_id && item.published_at && ["company", "product"].includes(item.coverage_scope ?? ""))
    .sort((a, b) => (Date.parse(b.published_at ?? "") || 0) - (Date.parse(a.published_at ?? "") || 0))
    .filter((item) => {
      const key = item.url ?? item.id;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
}

export function PositionDiagnosisCard({ position, finding }: { position: Position; finding: ResearchFinding | null }) {
  const analysis = finding?.diagnostic_dimensions;
  const news = companyNewsForPosition(position, finding);
  const newsFeed = (items: EvidenceSource[]) => <ol className="company-news-feed">{items.map((item) => <li key={item.id}>
    <div className="news-feed-meta">{item.publisher ? <span>{item.publisher}</span> : null}<time dateTime={item.published_at ?? undefined}>{formatTime(item.published_at)}</time></div>
    <h5>{item.url ? <a href={item.url} target="_blank" rel="noreferrer">{item.title}<span aria-hidden="true"> ↗</span></a> : item.title}</h5>
    {item.excerpt ? <p className="news-feed-excerpt">{item.excerpt}</p> : null}
  </li>)}</ol>;
  const points = (items: string[]) => items.length ? <ul>{items.map((item, index) => <li key={index}>{item}</li>)}</ul> : <p className="analysis-empty">{t("暂无")}</p>;
  return <article className="report-position-card">
    <header><div><span className="market-chip">{t(marketLabels[position.market] ?? position.market)}</span><h3>{position.symbol}<small>{position.instrument_name}</small></h3></div><span className={"impact " + (analysis?.intent_assessment === "missing" ? "missing-intent" : finding?.impact_direction ?? "unchanged")}>{finding ? findingImpactLabel(finding) : t("诊断缺失")}</span></header>
    {finding ? <>
      <section className="diagnosis-lead"><h4>{t("诊断结论")}</h4><p>{finding.fact_summary}</p></section>
      <div className="position-analysis-layout">
        <div className="position-outlook">
          {analysis ? <>
            <section className="position-analysis-section"><h4>{t("当前公司状态")}</h4><p>{analysis.company_context}</p></section>
            <section className="position-analysis-section future-expectations"><h4>{t("未来预期")}</h4><p>{analysis.future_expectation}</p></section>
          </> : null}
        </div>
        <section className="position-news"><h4>{t("公司与产品动态")}</h4>
          {news.length ? <>{newsFeed(news.slice(0, 5))}{news.length > 5 ? <details className="news-feed-more"><summary>{t("查看其余 {0} 条动态", { 0: news.length - 5 })}</summary>{newsFeed(news.slice(5))}</details> : null}</> : <p className="analysis-empty">{t("本报告未获得精确绑定该标的的近期公司或产品证据。")}</p>}
        </section>
      </div>
      {analysis ? <div className="position-risk-grid">
        <section className="position-analysis-section upside"><h4>{t("上行催化剂")}</h4>{points(analysis.upside_catalysts)}</section>
        <section className="position-analysis-section downside"><h4>{t("下行压力")}</h4>{points(analysis.downside_pressures)}</section>
        <section className="position-analysis-section core-risk"><h4>{t("核心风险")}</h4>{points(analysis.core_risks)}</section>
      </div> : null}
    </> : <p className="missing-diagnosis">{t("这项报告时持仓没有通过验证的逐持仓诊断。")}</p>}
  </article>;
}

function PanelHeading({ kicker, title, action, onAction }: { kicker: string; title: string; action?: string; onAction?: () => void }) {
  return <header className="panel-heading"><div><span>{kicker}</span><h2>{title}</h2></div>{action && <button onClick={onAction}>{action} →</button>}</header>;
}

function EmptyState({ title, body }: { title: string; body: string }) {
  return <div className="empty-state"><span aria-hidden="true">· · ·</span><strong>{title}</strong><p>{body}</p></div>;
}

function defaultCurrency(market: string) {
  if (market === "hk") return "HKD";
  return market === "us" ? "USD" : "RMB";
}

function CurrencyOptions() {
  return <><option value="RMB">RMB</option><option value="HKD">HKD</option><option value="USD">USD</option></>;
}

function instrumentEntryLabel(instrument: Instrument) {
  return [instrument.symbol, instrument.name, instrument.exchange].filter(Boolean).join(" · ");
}

type TradeSeed = { side: "buy" | "sell"; accountId: string; instrumentId: string; nonce: number };

function AccountForm({ onCreated, compact = false }: { onCreated: () => Promise<void>; compact?: boolean }) {
  const [draft, setDraft] = useState({ name: "", market: "a_share", broker: "" });
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setStatus(null);
    try {
      await request("/api/accounts", { method: "POST", body: JSON.stringify(draft) });
      await onCreated();
      setDraft({ name: "", market: draft.market, broker: "" });
      setStatus("账户已建立，现在可以直接记买入。");
    } catch {
      setStatus("账户保存失败，请检查名称和市场。");
    } finally {
      setBusy(false);
    }
  }

  return <form className={compact ? "account-form compact" : "account-form"} onSubmit={submit}>
    <label>{t("账户名称")}<input required value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} placeholder={t("例如：港股长线")} /></label>
    <label>{t("市场")}<select value={draft.market} onChange={(event) => setDraft({ ...draft, market: event.target.value })}><EquityMarketOptions /></select></label>
    <label>{t("记账币种")}<span className="fixed-field">{defaultCurrency(draft.market)}</span></label>
    {!compact ? <label>{t("券商")} <span>{t("可选")}</span><input value={draft.broker} onChange={(event) => setDraft({ ...draft, broker: event.target.value })} /></label> : null}
    <button disabled={busy}>{busy ? t("保存中…") : t("创建账户")}</button>
    {status ? <small role="status">{feedback(status)}</small> : null}
  </form>;
}

function InstrumentForm({ onCreated }: { onCreated: () => Promise<void> }) {
  const [draft, setDraft] = useState({ market: "a_share", symbol: "", name: "", currency: "RMB", exchange: "" });
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setStatus(null);
    try {
      await request("/api/instruments", { method: "POST", body: JSON.stringify({ ...draft, exchange: draft.exchange || null, chain: null, contract_address: null }) });
      await onCreated();
      setStatus("标的身份已保存。");
      setDraft({ ...draft, symbol: "", name: "" });
    } catch {
      setStatus("保存失败，请检查代码和市场是否重复。");
    } finally {
      setBusy(false);
    }
  }

  return <form className="instrument-form" onSubmit={submit}>
    <div className="field-pair"><label>{t("市场")}<select value={draft.market} onChange={(event) => { const market = event.target.value; setDraft({ ...draft, market, currency: defaultCurrency(market) }); }}><EquityMarketOptions /></select></label><label>{t("代码")}<input required value={draft.symbol} onChange={(event) => setDraft({ ...draft, symbol: event.target.value.toUpperCase() })} /></label></div>
    <label>{t("名称")}<input required value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} /></label>
    <div className="field-pair"><label>{t("币种")}<select value={draft.currency} onChange={(event) => setDraft({ ...draft, currency: event.target.value })}><CurrencyOptions /></select></label><label>{t("交易所")} <span>{t("可选")}</span><input value={draft.exchange} onChange={(event) => setDraft({ ...draft, exchange: event.target.value })} /></label></div>
    <button disabled={busy}>{busy ? t("保存中…") : t("保存标的身份")}</button>
    {status ? <small role="status">{feedback(status)}</small> : null}
  </form>;
}

function TradeComposer({ accounts, instruments, positions, seed, onCreated }: {
  accounts: Account[];
  instruments: Instrument[];
  positions: Position[];
  seed: TradeSeed | null;
  onCreated: () => Promise<void>;
}) {
  const seededInstrument = seed ? instruments.find((item) => item.id === seed.instrumentId) : undefined;
  const [draft, setDraft] = useState({ side: seed?.side ?? "buy", account_id: seed?.accountId ?? accounts[0]?.id ?? "", instrument_query: seededInstrument ? instrumentEntryLabel(seededInstrument) : "", instrument_name: "", quantity: "", price: "" });
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<{ tone: "ok" | "error"; message: string; raw?: boolean } | null>(null);
  const instrumentInput = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!seed) return;
    window.setTimeout(() => instrumentInput.current?.focus(), 80);
  }, [seed]);

  if (!accounts.length) {
    return <section className="trade-onboarding" id="trade-composer"><div><span className="step-pill">{t("第一步")}</span><h2>{t("创建一个交易账户")}</h2><p>{t("只需要名称、市场和币种。完成后，这里会立刻变成买入 / 卖出录入器。")}</p></div><AccountForm onCreated={onCreated} compact /></section>;
  }

  const selectedAccount = accounts.find((item) => item.id === draft.account_id) ?? accounts[0];
  const heldPositions = positions.filter((item) => item.owner_type === "account" && item.owner_id === selectedAccount.id);
  const eligibleInstruments = draft.side === "sell"
    ? heldPositions.map((position) => instruments.find((item) => item.id === position.instrument_id)).filter((item): item is Instrument => Boolean(item))
    : instruments.filter((item) => item.market === selectedAccount.market);
  const query = draft.instrument_query.trim();
  const normalizedQuery = query.toUpperCase();
  const exactInstrument = eligibleInstruments.find((item) => instrumentEntryLabel(item).toUpperCase() === normalizedQuery);
  const shorthandMatches = eligibleInstruments.filter((item) => item.symbol.toUpperCase() === normalizedQuery || item.name.toUpperCase() === normalizedQuery);
  const ambiguousInstrument = !exactInstrument && shorthandMatches.length > 1;
  const resolvedInstrument = exactInstrument ?? (shorthandMatches.length === 1 ? shorthandMatches[0] : undefined);
  const heldPosition = resolvedInstrument ? heldPositions.find((item) => item.instrument_id === resolvedInstrument.id) : undefined;
  const isNewInstrument = draft.side === "buy" && Boolean(query && !resolvedInstrument && !ambiguousInstrument);
  const canCreateInstrument = ["a_share", "hk", "us"].includes(selectedAccount.market) && /^[A-Z0-9.-]+$/.test(normalizedQuery);
  const quantity = Number(draft.quantity);
  const price = Number(draft.price);
  const amount = quantity > 0 && price > 0 ? quantity * price : null;
  const realizedEstimate = draft.side === "sell" && heldPosition?.average_cost && amount !== null
    ? quantity * (price - Number(heldPosition.average_cost))
    : null;
  const exceedsHolding = draft.side === "sell" && heldPosition && quantity > Number(heldPosition.quantity);
  const canSubmit = Boolean(selectedAccount && query && quantity > 0 && price > 0 && !ambiguousInstrument && !exceedsHolding && (resolvedInstrument || (isNewInstrument && canCreateInstrument)));
  let instrumentHint = draft.side === "sell" ? t("只显示这个账户当前持有的标的") : t("可输入已有标的；股票代码不存在时会自动建立");
  if (ambiguousInstrument) instrumentHint = t("存在同名标的，请选择带交易所的完整项");
  else if (resolvedInstrument) instrumentHint = `${resolvedInstrument.name} · ${t(marketLabels[resolvedInstrument.market] ?? resolvedInstrument.market)}`;
  else if (isNewInstrument && canCreateInstrument) instrumentHint = t("将自动建立 {0}", { 0: normalizedQuery });
  else if (query && !resolvedInstrument) instrumentHint = draft.side === "sell" ? t("这个账户当前没有该持仓") : t("链上标的需先在低频设置中补齐身份");

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!canSubmit) return;
    setBusy(true);
    setStatus(null);
    try {
      await request("/api/trades", { method: "POST", body: JSON.stringify({
        account_id: selectedAccount.id,
        instrument_id: resolvedInstrument?.id ?? null,
        symbol: resolvedInstrument ? null : normalizedQuery,
        name: resolvedInstrument ? null : draft.instrument_name || null,
        side: draft.side,
        quantity: draft.quantity,
        price: draft.price,
      }) });
      await onCreated();
      setStatus({ tone: "ok", message: `${draft.side === "buy" ? "买入" : "卖出"}已记录，总持仓与平均成本已自动更新。` });
      setDraft((current) => ({ ...current, instrument_query: "", instrument_name: "", quantity: "", price: "" }));
      window.setTimeout(() => instrumentInput.current?.focus(), 50);
    } catch {
      setStatus({ tone: "error", message: draft.side === "sell" ? "卖出记录失败，请确认数量没有超过当前持仓。" : "买入记录失败，请检查代码、数量和成交价。" });
    } finally {
      setBusy(false);
    }
  }

  return <section className={`trade-composer ${draft.side}`} id="trade-composer">
    <header className="trade-composer-header"><div><span className="eyebrow">{t("交易录入")}</span><h2>{t("记一笔买入或卖出")}</h2><p>{t("你只录入成交事实；总持仓、平均成本和已实现盈亏由系统计算。")}</p></div><div className="side-switch" aria-label={t("交易方向")}><button type="button" className={draft.side === "buy" ? "active buy" : "buy"} onClick={() => setDraft({ ...draft, side: "buy", instrument_query: "", quantity: "", price: "" })}>{t("买入")}</button><button type="button" className={draft.side === "sell" ? "active sell" : "sell"} onClick={() => setDraft({ ...draft, side: "sell", instrument_query: "", quantity: "", price: "" })}>{t("卖出")}</button></div></header>
    {status ? <div className={`trade-status ${status.tone}`} role="status">{status.raw ? status.message : feedback(status.message)}</div> : null}
    <form className="trade-form" onSubmit={submit}>
      <label>{t("账户")}<select value={selectedAccount.id} onChange={(event) => setDraft({ ...draft, account_id: event.target.value, instrument_query: "" })}>{accounts.map((account) => <option key={account.id} value={account.id}>{account.name} · {t(marketLabels[account.market] ?? account.market)}</option>)}</select></label>
      <label className="trade-instrument">{t("标的")}<input ref={instrumentInput} required list="trade-instruments" autoComplete="off" value={draft.instrument_query} onChange={(event) => setDraft({ ...draft, instrument_query: event.target.value, instrument_name: "" })} placeholder={draft.side === "buy" ? t("代码或名称") : t("选择当前持仓")} /><datalist id="trade-instruments">{eligibleInstruments.map((item) => <option key={item.id} value={instrumentEntryLabel(item)} />)}</datalist><small>{instrumentHint}</small></label>
      <label>{t("数量")}<input required inputMode="decimal" value={draft.quantity} onChange={(event) => setDraft({ ...draft, quantity: event.target.value })} placeholder="0" /><small>{heldPosition ? t("最多 {0}", { 0: formatNumber(heldPosition.quantity, 4) }) : t("股 / 份 / 枚")}</small></label>
      <label>{t("成交价")}<input required inputMode="decimal" value={draft.price} onChange={(event) => setDraft({ ...draft, price: event.target.value })} placeholder="0.00" /><small>{selectedAccount.currency}{t("，不含手续费")}</small></label>
      {isNewInstrument && canCreateInstrument ? <label className="new-instrument-name">{t("标的名称")} <span>{t("可选")}</span><input value={draft.instrument_name} onChange={(event) => setDraft({ ...draft, instrument_name: event.target.value })} placeholder={t("默认 {0}", { 0: normalizedQuery })} /></label> : null}
      <div className="trade-preview"><span>{draft.side === "buy" ? t("预计成交额") : t("预计卖出额")}</span><strong>{amount === null ? "—" : `${selectedAccount.currency} ${formatNumber(String(amount), 2)}`}</strong>{realizedEstimate !== null ? <small className={realizedEstimate >= 0 ? "positive" : "negative"}>{t("预计已实现")} {selectedAccount.currency} {formatNumber(String(realizedEstimate), 2)}</small> : <small>{t("现价将由行情源自动更新")}</small>}</div>
      <button className={draft.side === "buy" ? "submit-buy" : "submit-sell"} disabled={busy || !canSubmit}>{busy ? t("记录中…") : draft.side === "buy" ? t("确认买入") : t("确认卖出")}</button>
      {exceedsHolding ? <small className="trade-error">{t("卖出数量不能超过当前持仓。")}</small> : null}
    </form>
  </section>;
}

function RecentTrades({ trades, onCreated }: { trades: Trade[]; onCreated: () => Promise<void> }) {
  const [voidingId, setVoidingId] = useState<string | null>(null);
  const [voidError, setVoidError] = useState<string | null>(null);
  async function voidTrade(trade: Trade) {
    if (!window.confirm(t("撤销 {0} 这笔{1}？系统会保留审计记录并恢复成交前持仓。", { 0: trade.symbol, 1: trade.side === "buy" ? t("买入") : t("卖出") }))) return;
    setVoidingId(trade.id);
    setVoidError(null);
    try {
      await request(`/api/trades/${trade.id}/void`, { method: "POST", body: JSON.stringify({ reason: "用户从持仓工作台撤销错误成交" }) });
      await onCreated();
    } catch {
      setVoidError("撤销失败：只能撤销该账户最新一笔有效成交。");
    } finally {
      setVoidingId(null);
    }
  }
  return <section className="panel recent-trades"><PanelHeading kicker={t("最近成交")} title={t("最近成交")} />{voidError ? <div className="trade-status error" role="status">{feedback(voidError)}</div> : null}{trades.length ? <div>{trades.slice(0, 8).map((trade) => <article key={trade.id} className={trade.voided_at ? "voided" : ""}><span className={`trade-side ${trade.side}`}>{trade.side === "buy" ? t("买") : t("卖")}</span><div><strong>{trade.symbol}</strong><small>{trade.owner_name} · {formatTime(trade.executed_at)}</small></div><div><strong>{formatNumber(trade.quantity, 4)} × {formatNumber(trade.price)}</strong>{trade.voided_at ? <small>{t("已撤销 · 审计记录保留")}</small> : trade.realized_pnl ? <small className={Number(trade.realized_pnl) >= 0 ? "positive" : "negative"}>{t("已实现")} {trade.currency} {formatNumber(trade.realized_pnl, 2)}</small> : <small>{trade.currency} {formatNumber(trade.average_cost_after)}  {t("新均价")}</small>}{trade.can_void ? <button type="button" className="void-trade" disabled={voidingId === trade.id} onClick={() => void voidTrade(trade)}>{voidingId === trade.id ? t("撤销中…") : t("撤销错误成交")}</button> : null}</div></article>)}</div> : <EmptyState title={t("还没有成交")} body={t("第一笔买入会出现在这里，并自动生成总持仓。")} />}</section>;
}

function LowFrequencySetup({ onCreated }: { onCreated: () => Promise<void> }) {
  return <details className="panel low-frequency-setup"><summary><div><strong>{t("账户与标的设置")}</strong><span>{t("低频操作 · 日常录入不需要进入这里")}</span></div><span aria-hidden="true">＋</span></summary><div className="setup-drawer"><section><h3>{t("新增账户")}</h3><p>{t("一个账户固定一个市场和记账币种。")}</p><AccountForm onCreated={onCreated} /></section><section><h3>{t("补充标的身份")}</h3><p>{t("股票会在首次买入时自动建立；也可以在此补充标的名称和交易所。")}</p><InstrumentForm onCreated={onCreated} /></section></div></details>;
}

function EquityMarketOptions() {
  return <><option value="a_share">{t("A 股")}</option><option value="hk">{t("港股")}</option><option value="us">{t("美股")}</option></>;
}

function PortfolioView({ positions, intents, accounts, instruments, trades, changes, findings, onCreated }: {
  positions: Position[];
  intents: TradeIntent[];
  accounts: Account[];
  instruments: Instrument[];
  trades: Trade[];
  changes: PositionChange[];
  findings: ResearchFinding[];
  onCreated: () => Promise<void>;
}) {
  const [tradeSeed, setTradeSeed] = useState<TradeSeed | null>(null);
  const linkedIntentIds = new Set<string>();
  const workItems = positions.map((position) => {
    const candidates = intents.filter((intent) => intent.status === "active" && intent.instrument_ids.includes(position.instrument_id));
    const scoped = candidates.find((intent) => position.owner_type === "account" ? intent.account_id === position.owner_id : intent.wallet_id === position.owner_id);
    const intent = scoped;
    if (intent) linkedIntentIds.add(intent.id);
    const diagnosis = findings.find((finding) =>
      finding.finding_type === "equity_position_diagnosis"
      && finding.affected_object_type === "position_snapshot_item"
      && (finding.affected_object_id === position.position_snapshot_item_id || (
        finding.draft_position_current
        && finding.affected_instrument_id === position.instrument_id
        && (position.owner_type === "account"
          ? finding.affected_account_id === position.owner_id
          : finding.affected_wallet_id === position.owner_id)
      ))
      && finding.diagnostic_dimensions?.intent_assessment === "missing"
      && finding.diagnostic_dimensions.intent_draft
    );
    const suggestion = diagnosis?.diagnostic_dimensions?.intent_draft && diagnosis.affected_object_id
      ? {
          finding_id: diagnosis.id,
          position_snapshot_item_id: diagnosis.affected_object_id,
          as_of: diagnosis.draft_as_of,
          draft: diagnosis.diagnostic_dimensions.intent_draft,
        }
      : undefined;
    return { position, intent, suggestion };
  });
  const otherIntents = intents.filter((intent) => !linkedIntentIds.has(intent.id));
  const accountGroups = new Map<string, Array<{ position: Position; intent?: TradeIntent; suggestion?: ResearchIntentSuggestion }>>();
  for (const item of workItems) {
    const key = `${item.position.owner_type}:${item.position.owner_id}`;
    accountGroups.set(key, [...(accountGroups.get(key) ?? []), item]);
  }
  function startTrade(side: "buy" | "sell", position: Position) {
    if (position.owner_type !== "account") return;
    setTradeSeed({ side, accountId: position.owner_id, instrumentId: position.instrument_id, nonce: Date.now() });
    document.getElementById("trade-composer")?.scrollIntoView({ behavior: "smooth", block: "start" });
  }
  return (
    <div className="content-stack portfolio-stack">
      <section className="portfolio-primary-grid"><TradeComposer key={tradeSeed?.nonce ?? "trade-composer"} accounts={accounts} instruments={instruments} positions={positions} seed={tradeSeed} onCreated={onCreated} /><RecentTrades trades={trades} onCreated={onCreated} /></section>
      <IntentOnboarding items={workItems} onCreated={onCreated} />
      <LowFrequencySetup onCreated={onCreated} />
      <header className="section-title"><div><span className="eyebrow">{t("持仓聚合")}</span><h2>{t("当前总持仓")}</h2></div><p>{t("按账户展开；买卖与持仓逻辑都在标的行内完成。")}</p></header>
      {workItems.length ? <section className="account-position-groups" aria-label={t("按账户分组的当前持仓")}>{[...accountGroups.entries()].map(([key, items]) => <AccountPositionGroup key={key} items={items} onCreated={onCreated} onTrade={startTrade} />)}</section> : <section className="panel"><EmptyState title={t("还没有当前持仓")} body={t("在上方创建账户并录入第一笔买入，持仓会自动出现。")} /></section>}
      {changes.length ? <details className="panel history-details"><summary>{t("查看快照变化记录")}</summary><div className="change-list">{changes.map((change) => <article key={`${change.owner_id}-${change.instrument_id}`}><span className={`change-kind ${change.change_type}`}>{t(changeLabels[change.change_type])}</span><div><strong>{change.symbol} · {change.instrument_name}</strong><small>{change.owner_name} · {formatTime(change.current_as_of)}</small></div><div className="change-quantity"><small>{formatNumber(change.previous_quantity, 4)} → {formatNumber(change.current_quantity, 4)}</small><strong>{Number(change.quantity_delta) > 0 ? "+" : ""}{formatNumber(change.quantity_delta, 4)}</strong></div></article>)}</div></details> : null}
      {otherIntents.length ? <section className="panel"><PanelHeading kicker={t("其他持仓逻辑")} title={t("未绑定当前持仓的逻辑")} /><div className="intent-grid">{otherIntents.map((intent) => <IntentCard key={intent.id} intent={intent} onCreated={onCreated} />)}</div></section> : null}
    </div>
  );
}

function summarizeByCurrency(items: Array<{ position: Position }>, field: "market_value" | "unrealized_pnl") {
  const totals = new Map<string, number>();
  for (const { position } of items) {
    const currency = position.currency ?? "—";
    totals.set(currency, (totals.get(currency) ?? 0) + Number(position[field] ?? 0));
  }
  return [...totals.entries()].map(([currency, value]) => `${currency} ${formatNumber(String(value), 0)}`).join(" · ") || "—";
}

function AccountPositionGroup({ items, onCreated, onTrade }: { items: Array<{ position: Position; intent?: TradeIntent; suggestion?: ResearchIntentSuggestion }>; onCreated: () => Promise<void>; onTrade: (side: "buy" | "sell", position: Position) => void }) {
  const first = items[0].position;
  const latestAsOf = items.reduce((latest, item) => new Date(item.position.as_of) > new Date(latest) ? item.position.as_of : latest, first.as_of);
  const pnl = items.reduce((total, item) => total + Number(item.position.unrealized_pnl ?? 0), 0);
  return (
    <details className="account-position-group" open>
      <summary>
        <div className={`account-badge ${first.owner_type}`} aria-hidden="true">{first.owner_type === "account" ? t("账") : t("链")}</div>
        <div className="account-identity"><span>{first.owner_type === "account" ? t("证券账户") : t("链上钱包")}</span><h2>{first.owner_name}</h2><small>{items.length}  {t("项持仓 · 更新于")} {formatTime(latestAsOf)}</small></div>
        <div className="account-rollup"><div><span>{t("账户市值")}</span><strong>{summarizeByCurrency(items, "market_value")}</strong></div><div><span>{t("浮动盈亏")}</span><strong className={pnl >= 0 ? "positive" : "negative"}>{summarizeByCurrency(items, "unrealized_pnl")}</strong></div></div>
        <span className="account-disclosure" aria-hidden="true">⌄</span>
      </summary>
      <div className="account-position-items">{items.map(({ position, intent, suggestion }) => <PositionWorkCard key={position.instrument_id} position={position} intent={intent} suggestion={suggestion} onCreated={onCreated} onTrade={onTrade} />)}</div>
    </details>
  );
}

function PositionWorkCard({ position, intent, suggestion, onCreated, onTrade }: { position: Position; intent?: TradeIntent; suggestion?: ResearchIntentSuggestion; onCreated: () => Promise<void>; onTrade: (side: "buy" | "sell", position: Position) => void }) {
  const [showLogic, setShowLogic] = useState(false);
  return (
    <article className="holding-row">
      <div className="holding-identity"><span className="market-chip">{t(marketLabels[position.market] ?? position.market)}</span><div><h3>{position.symbol}</h3><p>{position.instrument_name}</p></div></div>
      <dl className="holding-facts"><div><dt>{t("数量")}</dt><dd>{formatNumber(position.quantity, 4)}</dd></div><div><dt>{t("平均成本")}</dt><dd>{position.currency} {formatNumber(position.average_cost)}</dd></div><div><dt>{t("参考现价")}</dt><dd>{position.market_price ? `${position.currency} ${formatNumber(position.market_price)}` : t("待行情同步")}</dd>{position.market_price_as_of ? <small>{formatTime(position.market_price_as_of)}</small> : null}</div><div><dt>{t("市值")}</dt><dd>{position.currency} {formatNumber(position.market_value, 0)}</dd></div><div><dt>{t("浮动盈亏")}</dt><dd className={Number(position.unrealized_pnl ?? 0) >= 0 ? "positive" : "negative"}>{position.currency} {formatNumber(position.unrealized_pnl, 0)}</dd></div></dl>
      <div className="holding-intent-summary"><span>{intent ? t("持仓逻辑") : t("逻辑缺失")}</span><strong>{intent?.title ?? t("补充为什么持有")}</strong><small>{intent ? `${enumLabel(intent.time_horizon)} · V${intent.version}` : t("研究无法判断新信息是否改变原判断")}</small></div>
      <div className="holding-actions">{position.owner_type === "account" ? <><button type="button" className="buy-action" onClick={() => onTrade("buy", position)}>{t("买入")}</button><button type="button" className="sell-action" onClick={() => onTrade("sell", position)}>{t("卖出")}</button></> : null}<button type="button" className="logic-action" onClick={() => setShowLogic(!showLogic)}>{showLogic ? t("收起") : intent ? t("编辑逻辑") : t("补充逻辑")}</button></div>
      {showLogic ? <section className="holding-logic-editor">{intent ? <IntentCard intent={intent} onCreated={onCreated} embedded /> : <CreatePositionIntent position={position} suggestion={suggestion} onCreated={onCreated} />}</section> : null}
    </article>
  );
}

function CreatePositionIntent({ position, suggestion, onCreated, onSaved, compact = false }: { position: Position; suggestion?: ResearchIntentSuggestion; onCreated: () => Promise<void>; onSaved?: () => Promise<void>; compact?: boolean }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState({ thesis: "", catalyst: "", invalidation: "", time_horizon: "medium" });
  const [adoptedSuggestion, setAdoptedSuggestion] = useState<ResearchIntentSuggestion | null>(null);

  async function createIntent(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await request("/api/trade-intents", {
        method: "POST",
        body: JSON.stringify({
          title: `${position.symbol} 持仓逻辑`,
          strategy_type: "position",
          time_horizon: draft.time_horizon,
          status: "active",
          instrument_ids: [position.instrument_id],
          account_id: position.owner_type === "account" ? position.owner_id : null,
          wallet_id: position.owner_type === "wallet" ? position.owner_id : null,
          position_slice: null,
          research_draft: adoptedSuggestion ? {
            finding_id: adoptedSuggestion.finding_id,
            position_snapshot_item_id: adoptedSuggestion.position_snapshot_item_id,
            modified: adoptedSuggestion.modified ?? false,
          } : null,
          version: { thesis: draft.thesis, catalysts: splitLines(draft.catalyst), invalidation_conditions: splitLines(draft.invalidation), exit_principles: [] },
        }),
      });
      await (onSaved ? onSaved() : onCreated());
    } catch {
      setError("持仓逻辑保存失败，请检查必填字段。");
    } finally {
      setBusy(false);
    }
  }

  const suggestedDraft = suggestion?.draft;
  function markEdited() {
    if (adoptedSuggestion) setAdoptedSuggestion({ ...adoptedSuggestion, modified: true });
  }
  return <div className={`empty-logic ${compact ? "compact" : ""}`}><span className="eyebrow">{t("持仓逻辑")}</span>{compact ? null : <><h3>{t("这项持仓还没有逻辑")}</h3><p>{t("只回答为什么继续持有，以及什么情况说明判断错了。")}</p></>}{suggestedDraft ? <aside className="intent-ai-draft"><div><strong>{t("模型已根据经验证诊断起草")}</strong><span>{t("诊断截止")} {formatTime(suggestion?.as_of ?? null)}  {t("· 持仓数量与成本未变，行情可能已更新 · 采用后仍需你确认")}</span><p>{suggestedDraft.thesis}</p></div><button type="button" onClick={() => { setDraft({ thesis: suggestedDraft.thesis, catalyst: suggestedDraft.catalysts.join("\n"), invalidation: suggestedDraft.invalidation_conditions.join("\n"), time_horizon: suggestedDraft.time_horizon }); setAdoptedSuggestion(suggestion); }}>{t("采用研究草稿")}</button></aside> : null}<form className="intent-editor embedded-editor quick-logic-form" onSubmit={createIntent}><label>{t("为什么继续持有？")}<textarea required value={draft.thesis} onChange={(event) => { setDraft({ ...draft, thesis: event.target.value }); markEdited(); }} placeholder={t("例如：订单兑现会推动未来两个季度利润上修")} /></label><label>{t("什么情况说明判断错了？")}<textarea required value={draft.invalidation} onChange={(event) => { setDraft({ ...draft, invalidation: event.target.value }); markEdited(); }} placeholder={t("例如：核心客户订单连续两个季度低于预期")} /></label><div className="quick-logic-meta"><label>{t("计划持有多久？")}<select value={draft.time_horizon} onChange={(event) => { setDraft({ ...draft, time_horizon: event.target.value }); markEdited(); }}><option value="short">{t("几天")}</option><option value="swing">{t("几周")}</option><option value="medium">{t("几个月")}</option><option value="long">{t("一年以上")}</option></select></label><label>{t("近期催化")} <span>{t("可选")}</span><textarea value={draft.catalyst} onChange={(event) => { setDraft({ ...draft, catalyst: event.target.value }); markEdited(); }} placeholder={t("财报、产品、政策或事件；每行一项")} /></label></div>{adoptedSuggestion?.modified ? <small>{t("你已修改模型草稿；保存时仍会保留研究来源。")}</small> : null}{error ? <small className="inline-error">{feedback(error)}</small> : null}<button disabled={busy}>{busy ? t("保存中…") : onSaved ? t("确认并启用，然后处理下一项") : t("确认并启用持仓逻辑")}</button></form></div>;
}

function positionWorkKey(position: Position) {
  return `${position.owner_type}:${position.owner_id}:${position.instrument_id}`;
}

function IntentOnboarding({ items, onCreated }: { items: Array<{ position: Position; intent?: TradeIntent; suggestion?: ResearchIntentSuggestion }>; onCreated: () => Promise<void> }) {
  const stockItems = items.filter(({ position }) => ["a_share", "hk", "us"].includes(position.market));
  const missing = stockItems.filter(({ intent }) => !intent);
  const [selectedKey, setSelectedKey] = useState(() => missing[0] ? positionWorkKey(missing[0].position) : "");
  const activeIndex = Math.max(0, missing.findIndex(({ position }) => positionWorkKey(position) === selectedKey));
  const active = missing[activeIndex] ?? missing[0];

  if (!missing.length || !active) return null;
  const completed = stockItems.length - missing.length;
  async function saved() {
    const next = missing[activeIndex + 1] ?? missing[activeIndex - 1];
    setSelectedKey(next ? positionWorkKey(next.position) : "");
    await onCreated();
  }

  return <section className="panel intent-onboarding" aria-label={t("待补充持仓逻辑")}><header><div><span className="eyebrow">{t("逻辑补全队列")}</span><h2>{t("连续补全持仓逻辑")}</h2><p>{t("可以采用模型根据经验证诊断生成的逻辑草稿；确认后自动进入下一项。")}</p></div><div className="logic-progress"><strong>{completed} / {stockItems.length}</strong><span>{t("已完成 · 还剩")} {missing.length}</span><progress max={stockItems.length} value={completed}>{completed} / {stockItems.length}</progress></div></header><div className="intent-onboarding-body"><nav aria-label={t("缺少逻辑的持仓")}>{missing.map(({ position }) => { const key = positionWorkKey(position); return <button type="button" key={key} className={key === positionWorkKey(active.position) ? "active" : ""} onClick={() => setSelectedKey(key)}><span>{position.symbol}</span><small>{position.owner_name} · {t(marketLabels[position.market])}</small></button>; })}</nav><div className="intent-onboarding-editor"><div className="logic-position-context"><div><span className="market-chip">{t(marketLabels[active.position.market])}</span><h3>{active.position.symbol} · {active.position.instrument_name}</h3><p>{active.position.owner_name}</p></div><dl><div><dt>{t("成本")}</dt><dd>{active.position.currency} {formatNumber(active.position.average_cost)}</dd></div><div><dt>{t("现价")}</dt><dd>{active.position.currency} {formatNumber(active.position.market_price)}</dd></div><div><dt>{t("浮盈亏")}</dt><dd className={Number(active.position.unrealized_pnl ?? 0) >= 0 ? "positive" : "negative"}>{active.position.currency} {formatNumber(active.position.unrealized_pnl, 0)}</dd></div></dl></div><CreatePositionIntent key={positionWorkKey(active.position)} position={active.position} suggestion={active.suggestion} onCreated={onCreated} onSaved={saved} compact /></div></div></section>;
}

function splitLines(value: string) {
  return value.split(/\n|,/).map((item) => item.trim()).filter(Boolean);
}

function IntentCard({ intent, onCreated, embedded = false }: { intent: TradeIntent; onCreated: () => Promise<void>; embedded?: boolean }) {
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState({
    title: intent.title,
    strategy_type: intent.strategy_type,
    time_horizon: intent.time_horizon,
    status: intent.status,
    thesis: intent.thesis,
    risk_budget: intent.risk_budget ?? "",
    target_position: intent.target_position ?? "",
    catalysts: intent.catalysts.join("\n"),
    invalidation_conditions: intent.invalidation_conditions.join("\n"),
    exit_principles: intent.exit_principles.join("\n"),
    notes: intent.notes ?? "",
    owner: intent.account_id ? `account:${intent.account_id}` : intent.wallet_id ? `wallet:${intent.wallet_id}` : "",
    instrument_id: intent.instrument_ids[0] ?? "",
    position_slice: intent.position_slice ?? "",
  });

  function startEditing() {
    setDraft({
      title: intent.title,
      strategy_type: intent.strategy_type,
      time_horizon: intent.time_horizon,
      status: intent.status,
      thesis: intent.thesis,
      risk_budget: intent.risk_budget ?? "",
      target_position: intent.target_position ?? "",
      catalysts: intent.catalysts.join("\n"),
      invalidation_conditions: intent.invalidation_conditions.join("\n"),
      exit_principles: intent.exit_principles.join("\n"),
      notes: intent.notes ?? "",
      owner: intent.account_id ? `account:${intent.account_id}` : intent.wallet_id ? `wallet:${intent.wallet_id}` : "",
      instrument_id: intent.instrument_ids[0] ?? "",
      position_slice: intent.position_slice ?? "",
    });
    setError(null);
    setEditing(true);
  }

  async function saveVersion(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const [ownerType, ownerId] = draft.owner.split(":", 2);
      await request(`/api/trade-intents/${intent.id}/versions`, {
        method: "POST",
        body: JSON.stringify({
          status: draft.status,
          title: draft.title,
          strategy_type: draft.strategy_type,
          time_horizon: draft.time_horizon,
          thesis: draft.thesis,
          risk_budget: draft.risk_budget || null,
          target_position: draft.target_position || null,
          catalysts: splitLines(draft.catalysts),
          invalidation_conditions: splitLines(draft.invalidation_conditions),
          exit_principles: splitLines(draft.exit_principles),
          notes: draft.notes || null,
          scope: {
            instrument_ids: [draft.instrument_id],
            account_id: ownerType === "account" ? ownerId : null,
            wallet_id: ownerType === "wallet" ? ownerId : null,
            position_slice: draft.position_slice || null,
          },
        }),
      });
      await onCreated();
      setEditing(false);
    } catch {
      setError("新版本保存失败，请检查数值和必填字段。");
    } finally {
      setBusy(false);
    }
  }

  return (
    <article className={`intent-card ${embedded ? "embedded" : ""} ${editing ? "editing" : ""}`}>
      <header><span className={`run-status ${intent.status}`}>{t(statusLabels[intent.status] ?? intent.status)}</span><small>V{intent.version}</small></header>
      <h3>{intent.title}</h3><p>{intent.thesis}</p>
      <dl><div><dt>{t("策略")}</dt><dd>{enumLabel(intent.strategy_type)}</dd></div><div><dt>{t("周期")}</dt><dd>{enumLabel(intent.time_horizon)}</dd></div><div><dt>{t("仓位归属")}</dt><dd>{intent.owner_name}</dd></div><div><dt>{t("仓位切片")}</dt><dd>{intent.position_slice ?? t("全部")}</dd></div></dl>
      <footer><span>{t("失效条件")} {intent.invalidation_conditions.length || t("未填写")}</span><button type="button" onClick={() => editing ? setEditing(false) : startEditing()}>{editing ? t("取消") : t("编辑逻辑")}</button></footer>
      {editing ? (
        <form className="intent-editor" onSubmit={saveVersion}>
          <label>{t("标题")}<input required value={draft.title} onChange={(event) => setDraft({ ...draft, title: event.target.value })} /></label>
          <label>{t("核心逻辑")}<textarea required value={draft.thesis} onChange={(event) => setDraft({ ...draft, thesis: event.target.value })} /></label>
          <label>{t("催化剂（每行一项）")}<textarea value={draft.catalysts} onChange={(event) => setDraft({ ...draft, catalysts: event.target.value })} /></label>
          <label>{t("失效条件（每行一项）")}<textarea required value={draft.invalidation_conditions} onChange={(event) => setDraft({ ...draft, invalidation_conditions: event.target.value })} /></label>
          <details className="intent-advanced"><summary>{t("更多设置")}</summary><div className="field-pair"><label>{t("策略类型")}<input required value={draft.strategy_type} onChange={(event) => setDraft({ ...draft, strategy_type: event.target.value })} /></label><label>{t("时间尺度")}<input required value={draft.time_horizon} onChange={(event) => setDraft({ ...draft, time_horizon: event.target.value })} /></label></div><div className="field-pair"><label>{t("状态")}<select value={draft.status} onChange={(event) => setDraft({ ...draft, status: event.target.value })}><option value="draft">{t("草稿")}</option><option value="active">{t("有效")}</option><option value="paused">{t("暂停")}</option><option value="invalidated">{t("失效")}</option><option value="closed">{t("关闭")}</option></select></label><label>{t("仓位切片")}<input inputMode="decimal" value={draft.position_slice} onChange={(event) => setDraft({ ...draft, position_slice: event.target.value })} /></label></div><div className="field-pair"><label>{t("风险预算")}<input inputMode="decimal" value={draft.risk_budget} onChange={(event) => setDraft({ ...draft, risk_budget: event.target.value })} /></label><label>{t("目标仓位")}<input inputMode="decimal" value={draft.target_position} onChange={(event) => setDraft({ ...draft, target_position: event.target.value })} /></label></div><label>{t("退出原则（每行一项）")}<textarea value={draft.exit_principles} onChange={(event) => setDraft({ ...draft, exit_principles: event.target.value })} /></label><label>{t("备注")}<textarea value={draft.notes} onChange={(event) => setDraft({ ...draft, notes: event.target.value })} /></label></details>
          {error ? <small className="inline-error">{feedback(error)}</small> : null}
          <button disabled={busy}>{busy ? t("保存中…") : t("保存修改")}</button>
        </form>
      ) : null}
    </article>
  );
}

const intelligenceMarkets = [
  ["all", "全部"], ["macro", "宏观"], ["a_share", "A 股"], ["hk", "港股"],
  ["us", "美股"],
] as const;
const activeStockResearchMarkets = new Set(["macro", "a_share", "hk", "us"]);

function findingImpactLabel(finding: ResearchFinding) {
  if (finding.diagnostic_dimensions?.intent_assessment === "missing") return t("缺少持仓逻辑");
  return enumLabel(finding.impact_direction ?? finding.finding_type);
}

function IntelligenceView({ runner, data, ...reportProps }: { runner: string; data: IntelligenceData } & ReportViewProps) {
  const [market, setMarket] = useState<string>("all");
  const filterMarket = (value: string) => activeStockResearchMarkets.has(value) && (market === "all" || value === market);
  const findings = data.findings.filter((finding) => filterMarket(finding.market));
  const narratives = data.narratives.filter((narrative) => filterMarket(narrative.market));
  const runs = data.runs.filter((run) => filterMarket(run.market));
  return (
    <div className="content-stack">
      <ReportSelector {...reportProps} />
      {runner === "not_configured" ? <div className="runner-warning" role="status"><span className="status-dot offline" /><span>{t("研究执行器未配置，报告目前只能记录数据缺口。")}</span></div> : null}
      <nav className="market-filter" aria-label={t("市场情报筛选")}>
        {intelligenceMarkets.map(([value, label]) => <button key={value} className={market === value ? "active" : ""} onClick={() => setMarket(value)}>{t(label)}</button>)}
      </nav>
      <section className="intelligence-columns">
        <div className="panel intelligence-section">
          <PanelHeading kicker={t("已验证研究")} title={t("宏观与持仓诊断")} />
          {reportProps.detail && findings.length ? <div className="finding-list">{findings.map((finding) => (
            <article key={finding.id}>
              <header><span className="market-chip">{t(marketLabels[finding.market] ?? finding.market)}</span><span className={`impact ${finding.diagnostic_dimensions?.intent_assessment === "missing" ? "missing-intent" : finding.impact_direction ?? "unchanged"}`}>{findingImpactLabel(finding)}</span><small>{Math.round(Number(finding.confidence) * 100)}{t("% 置信")}</small></header>
              <h3>{finding.fact_summary}</h3>{finding.inference ? <p>{finding.inference}</p> : null}
              {finding.diagnostic_dimensions ? <div className="diagnostic-dimensions">
                <div><span>{t("持仓逻辑")}</span><p>{finding.diagnostic_dimensions.intent_assessment === "matched" ? t("已匹配有效 Trade Intent") : finding.diagnostic_dimensions.intent_assessment === "missing" ? t("缺少有效 Trade Intent；本诊断不判断逻辑加强或削弱") : t("历史诊断未记录匹配状态")}</p></div>
                <div><span>{t("宏观")}</span><p>{finding.diagnostic_dimensions.macro_context}</p></div>
                <div><span>{t("行业")}</span><p>{finding.diagnostic_dimensions.industry_context}</p></div>
                <div><span>{t("公司")}</span><p>{finding.diagnostic_dimensions.company_context}</p></div>
                <div><span>{t("市场定价")}</span><p>{finding.diagnostic_dimensions.market_pricing}</p></div>
                <div><span>{t("未来预期")}</span><p>{finding.diagnostic_dimensions.future_expectation}</p></div>
                <div><span>{t("预期差")}</span><p>{finding.diagnostic_dimensions.expectation_gap}</p></div>
                <div><span>{t("上行催化剂")}</span><p>{finding.diagnostic_dimensions.upside_catalysts.join("；") || t("暂无可靠催化剂")}</p></div>
                <div><span>{t("下行压力")}</span><p>{finding.diagnostic_dimensions.downside_pressures.join("；") || t("暂无明确下行压力")}</p></div>
                <div><span>{t("核心风险")}</span><p>{finding.diagnostic_dimensions.core_risks.join("；") || t("暂无新增核心风险")}</p></div>
              </div> : null}
              {finding.conditional_action ? <div className="conditional-action"><strong>{t("条件式建议")}</strong>{finding.conditional_action}</div> : null}
              <dl><div><dt>{t("确认条件")}</dt><dd>{finding.confirmation_conditions.join("；") || t("未给出")}</dd></div><div><dt>{t("失效条件")}</dt><dd>{finding.invalidation_conditions.join("；") || t("未给出")}</dd></div></dl>
              <details><summary>{finding.evidence.length}  {t("条证据 ·")} {finding.skill_name}</summary><div className="evidence-list">{finding.evidence.map((evidence) => <div key={evidence.id}><strong>{evidence.url ? <a href={evidence.url} target="_blank" rel="noreferrer">{evidence.title}</a> : evidence.title}</strong><small>{evidence.publisher ?? evidence.source_type} · {formatTime(evidence.published_at ?? evidence.observed_at)}</small>{evidence.excerpt ? <p>{evidence.excerpt}</p> : null}</div>)}</div><code>{finding.skill_version}</code></details>
            </article>
          ))}</div> : <EmptyState title={t("所选报告没有通过验证的研究发现")} body={t("历史市场情报只读取所选报告，不会混入其他日期的结果。")} />}
        </div>
        <div className="content-stack tight">
          <section className="panel intelligence-section"><PanelHeading kicker={t("叙事")} title={t("明线、暗线与资金流")} />{narratives.length ? <div className="narrative-list">{narratives.map((item) => <article key={`${item.id}-${item.as_of}`}><header><span className={`narrative-type ${item.narrative_type}`}>{item.narrative_type === "explicit" ? t("明线") : t("暗线")}</span><small>{enumLabel(item.stage)} · {Math.round(Number(item.confidence) * 100)}%</small></header><h3>{item.name}</h3><p>{item.summary}</p><small>{item.capital_flow_inference ?? t("没有可靠资金流推断")}</small></article>)}</div> : <EmptyState title={t("尚无叙事快照")} body={t("没有足够证据时，系统不会强行指定市场主线。")} />}</section>
          <section className="panel intelligence-section"><PanelHeading kicker={t("研究运行")} title={t("运行与溯源")} />{runs.length ? <div className="run-list">{runs.slice(0, 20).map((run) => <article key={run.id}><span className={`run-status ${run.status}`}>{t(statusLabels[run.status] ?? run.status)}</span><div><strong>{run.skill_name}</strong><small>{formatTime(run.as_of)} · {enumLabel(run.validation_status ?? "未验证")}</small><small>{t("公共标准")} {run.supporting_skills["apply-investment-research-standards"]?.slice(0, 20) ?? t("未记录")}</small></div></article>)}</div> : <EmptyState title={t("尚无 Research Run")} body={t("未配置执行器时不会伪造研究运行记录。")} />}</section>
        </div>
      </section>
    </div>
  );
}

type BriefingDetail = {
  run: Briefing & { input_snapshot: Record<string, unknown>; error: string | null; research_window_start: string };
  items: Array<{ id: string; section: string; title: string; body: string; severity: string | null; source_finding_ids: string[]; metadata: Record<string, unknown> }>;
  research_runs: ResearchRun[];
};

function BriefingProgress({ briefing }: { briefing: Briefing }) {
  const total = Math.max(briefing.total_research_tasks, 1);
  const completed = Math.min(briefing.completed_research_tasks, total);
  const current = Math.min(completed + 1, total);
  const active = briefing.current_research_tasks;
  const currentLabel = active.length
    ? active.length === 1
      ? active[0]
      : t("{0} 等 {1} 项并行", { 0: active[0], 1: active.length })
    : briefing.status === "pending"
      ? t("等待研究 worker")
      : completed === total
        ? briefing.status === "running" ? t("正在合成最终报告") : t("研究任务已完成")
        : t("等待下一阶段");
  return (
    <div className="briefing-progress" aria-label={t("已完成 {0} / {1}", { 0: completed, 1: total })}>
      <div><strong>{t("当前任务")} {current} / {total}</strong><span>{t("已完成")} {completed}  {t("项 ·")} {currentLabel}</span></div>
      <progress max={total} value={completed}>{completed} / {total}</progress>
    </div>
  );
}

function renderInlineMarkdown(value: string): ReactNode[] {
  const tokens = value.split(/(\*\*[^*]+\*\*|\[[^\]]+\]\(https?:\/\/[^\s)]+\)|https?:\/\/[^\s)）]+)/g);
  return tokens.filter(Boolean).map((token, index) => {
    const link = token.match(/^\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)$/);
    if (link) return <a key={index} href={link[2]} target="_blank" rel="noreferrer">{link[1]}</a>;
    if (/^https?:\/\//.test(token)) return <a key={index} href={token} target="_blank" rel="noreferrer">{t("来源链接")}</a>;
    if (token.startsWith("**") && token.endsWith("**")) return <strong key={index}>{token.slice(2, -2)}</strong>;
    return token;
  });
}

function BriefingDocument({ markdown }: { markdown: string }) {
  const blocks: ReactNode[] = [];
  const lines = markdown.split("\n");
  let paragraph: string[] = [];
  let bullets: string[] = [];
  let ordered: string[] = [];

  function flush() {
    if (paragraph.length) {
      const text = paragraph.join(" ").trim();
      if (text) blocks.push(<p key={`p-${blocks.length}`}>{renderInlineMarkdown(text)}</p>);
      paragraph = [];
    }
    if (bullets.length) {
      blocks.push(<ul key={`ul-${blocks.length}`}>{bullets.map((item, index) => <li key={index}>{renderInlineMarkdown(item)}</li>)}</ul>);
      bullets = [];
    }
    if (ordered.length) {
      blocks.push(<ol key={`ol-${blocks.length}`}>{ordered.map((item, index) => <li key={index}>{renderInlineMarkdown(item)}</li>)}</ol>);
      ordered = [];
    }
  }

  for (const rawLine of lines) {
    const line = rawLine.trim();
    if (!line) {
      flush();
    } else if (line.startsWith("### ")) {
      flush(); blocks.push(<h3 key={`h3-${blocks.length}`}>{renderInlineMarkdown(line.slice(4))}</h3>);
    } else if (line.startsWith("## ")) {
      flush(); blocks.push(<h2 key={`h2-${blocks.length}`}>{renderInlineMarkdown(line.slice(3))}</h2>);
    } else if (line.startsWith("# ")) {
      flush(); blocks.push(<h1 key={`h1-${blocks.length}`}>{renderInlineMarkdown(line.slice(2))}</h1>);
    } else if (/^[-*] /.test(line)) {
      if (paragraph.length || ordered.length) flush();
      bullets.push(line.slice(2));
    } else if (/^\d+\. /.test(line)) {
      if (paragraph.length || bullets.length) flush();
      ordered.push(line.replace(/^\d+\. /, ""));
    } else {
      if (bullets.length || ordered.length) flush();
      paragraph.push(line);
    }
  }
  flush();
  return <article className="briefing-document">{blocks}</article>;
}

function BriefingsView({ briefings, onCreate, disabled }: { briefings: Briefing[]; onCreate: () => Promise<void>; disabled: boolean }) {
  const [detail, setDetail] = useState<BriefingDetail | null>(null);
  const [loadingDetail, setLoadingDetail] = useState(false);
  const selectedDetailId = detail?.run.id;
  const latestDetailRun = detail ? briefings.find((briefing) => briefing.id === detail.run.id) : null;
  const displayedDetail = detail && latestDetailRun ? { ...detail, run: { ...detail.run, ...latestDetailRun } } : detail;

  useEffect(() => {
    if (!selectedDetailId) return;
    let cancelled = false;
    void request<BriefingDetail>(`/api/briefings/${selectedDetailId}`).then((nextDetail) => {
      if (!cancelled) setDetail(nextDetail);
    });
    return () => {
      cancelled = true;
    };
  }, [briefings, selectedDetailId]);

  async function showDetail(id: string) {
    setLoadingDetail(true);
    try {
      setDetail(await request<BriefingDetail>(`/api/briefings/${id}`));
    } finally {
      setLoadingDetail(false);
    }
  }
  return (
    <div className="content-stack">
      <section className="briefing-toolbar" aria-label={t("生成报告")}>
        <div><strong>{t("生成当前报告")}</strong><span>{t("任务将在后台继续运行；历史报告不可重新生成")}</span></div>
        <div><button className="primary-button" disabled={disabled} onClick={() => void onCreate()}>{disabled ? t("报告生成中") : t("生成当前报告")}</button></div>
      </section>
      <section className="panel">
        <PanelHeading kicker={t("历史记录")} title={t("不可变报告记录")} />
        {briefings.length ? <div className="report-list">{briefings.map((briefing) => (
          <article key={briefing.id} className={detail?.run.id === briefing.id ? "selected" : ""}>
            <div className="briefing-stamp small"><span>{t("报")}</span><small>{reportDate(briefing.as_of).slice(5)}</small></div>
            <div><h3>{formatTime(briefing.as_of)}</h3><p>{briefing.rendered_markdown?.split("\n").filter(Boolean).at(-1) ?? t("任务已入队，等待后台处理。")}</p>{briefing.status === "pending" || briefing.status === "running" ? <BriefingProgress briefing={briefing} /> : null}</div>
            <div className="report-row-actions"><span className={`run-status ${briefing.status}`}>{t(statusLabels[briefing.status])}</span><button className="text-button" onClick={() => void showDetail(briefing.id)}>{loadingDetail ? t("读取中") : t("查看")}</button></div>
          </article>
        ))}</div> : <EmptyState title={t("没有历史报告")} body={t("创建第一份报告后，它会作为不可变记录保存在这里。")} />}
      </section>
      {displayedDetail ? <section className="panel briefing-detail">
        <header><div><span className="eyebrow">{t("不可变报告")}</span><h2>{formatTime(displayedDetail.run.as_of)}  {t("报告")}</h2></div><div><span className={`run-status ${displayedDetail.run.status}`}>{t(statusLabels[displayedDetail.run.status])}</span></div></header>
        <div className="trace-strip"><span>{t("截止")} {formatTime(displayedDetail.run.as_of)}</span><span>{displayedDetail.research_runs.length}  {t("个 Research Run")}</span><span>{displayedDetail.run.skill_name ?? t("未调用合成 Skill")}</span><span>{enumLabel(displayedDetail.run.validation_status ?? "未验证")}</span></div>
        {displayedDetail.run.status === "pending" || displayedDetail.run.status === "running" ? <BriefingProgress briefing={displayedDetail.run} /> : null}
        {displayedDetail.run.rendered_markdown ? <BriefingDocument markdown={displayedDetail.run.rendered_markdown} /> : <EmptyState title={t("报告尚未渲染")} body={t("任务仍在后台队列中；刷新后可查看结果。")} />}
        {displayedDetail.items.length ? <details className="briefing-citations"><summary>{t("查看结构化条目与研究引用")}</summary><div className="briefing-item-trace">{displayedDetail.items.map((item) => { const intentStatus = typeof item.metadata.intent_status === "string" ? item.metadata.intent_status : null; const actionability = typeof item.metadata.actionability === "string" ? item.metadata.actionability : null; return <article key={item.id}><div className="briefing-item-labels"><span>{item.section}</span>{intentStatus === "draft_unconfirmed" ? <span className="intent-state draft">{t("持仓逻辑待确认")}</span> : intentStatus === "active" ? <span className="intent-state active">{t("已启用持仓逻辑")}</span> : null}{actionability === "non_actionable" ? <span className="intent-state non-actionable">{t("不可操作")}</span> : null}</div><h3>{item.title}</h3><BriefingDocument markdown={item.body} /><small>{t("来源 Findings：")}{item.source_finding_ids.length ? item.source_finding_ids.join(" · ") : t("系统状态项，无研究事实引用")}</small></article>; })}</div></details> : null}
        {displayedDetail.research_runs.length ? <div className="research-trace">{displayedDetail.research_runs.map((run) => <article key={run.id}><span className={`run-status ${run.status}`}>{t(statusLabels[run.status] ?? run.status)}</span><div><strong>{run.skill_name}</strong><small>{run.market} · {enumLabel(run.validation_status ?? "未验证")}</small><code>{run.skill_version}</code><code>{run.supporting_skills["apply-investment-research-standards"] ?? t("公共标准版本未记录")}</code></div></article>)}</div> : null}
        {displayedDetail.run.data_gaps.length ? <div className="data-gap-box"><strong>{t("数据缺口")}</strong><ul>{displayedDetail.run.data_gaps.map((gap) => <li key={gap}>{gap}</li>)}</ul></div> : null}
      </section> : null}
    </div>
  );
}

function SettingsView({ sourceSnapshots, settings, onCreated }: { sourceSnapshots: SourceSnapshot[]; settings: AppSettings; onCreated: () => Promise<void> }) {
  const [schedule, setSchedule] = useState(settings.briefing_schedule);
  const [source, setSource] = useState({ provider: "manual_adapter", source_type: "market_data", as_of: new Date().toISOString().slice(0, 16), payload: "{}" });
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<{ tone: "ok" | "error"; message: string; raw?: boolean } | null>(null);

  async function submit(event: FormEvent<HTMLFormElement>, path: string, payload: object, success: string, method = "POST") {
    event.preventDefault();
    setBusy(true);
    setStatus(null);
    try {
      await request(path, { method, body: JSON.stringify(payload) });
      await onCreated();
      setStatus({ tone: "ok", message: success });
    } catch {
      setStatus({ tone: "error", message: "保存失败，请检查配置字段是否完整。" });
    } finally {
      setBusy(false);
    }
  }

  async function setAutomaticReportsEnabled(enabled: boolean) {
    const nextSchedule = { ...schedule, enabled };
    setBusy(true);
    setStatus(null);
    try {
      await request("/api/settings", { method: "PUT", body: JSON.stringify(nextSchedule) });
      setSchedule(nextSchedule);
      await onCreated();
      setStatus({ tone: "ok", message: enabled ? "自动日报已恢复。" : "自动日报已暂停；手动生成报告不受影响。" });
    } catch {
      setStatus({ tone: "error", message: "日报计划更新失败，请稍后重试。" });
    } finally {
      setBusy(false);
    }
  }

  function saveSource(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    let payload: unknown;
    try {
      payload = JSON.parse(source.payload);
    } catch {
      setStatus({ tone: "error", message: "来源 Payload 必须是有效 JSON。" });
      return;
    }
    void submit(event, "/api/data-sources/snapshots", { ...source, as_of: new Date(source.as_of).toISOString(), request: {}, payload }, t("来源快照已固化，可进入下一份报告的研究窗口。"));
  }

  return (
    <div className="settings-grid">
      <section className="panel settings-card"><span className="settings-index">01</span><h2>{t("每日报告计划")}</h2><p>Asia/Shanghai · {schedule.enabled ? t("自动日报运行中") : t("自动日报已暂停")}  {t("· 手动生成报告不受影响")}</p><div className={`configuration-state ${schedule.enabled ? "safe" : ""}`}>{schedule.enabled ? t("每天 {0} 自动生成", { 0: schedule.time }) : t("已暂停自动生成")}</div><form onSubmit={(event) => void submit(event, "/api/settings", schedule, t("每日报告时间已更新。"), "PUT") }><label>{t("报告语言")}<select value={schedule.language ?? "en"} onChange={(event) => setSchedule({ ...schedule, language: event.target.value === "en" ? "en" : "zh" })}><option value="en">English</option><option value="zh">中文</option></select></label><label>{t("自动生成时间")}<input type="time" required value={schedule.time} onChange={(event) => setSchedule({ ...schedule, time: event.target.value })} /></label><button disabled={busy}>{t("更新时间")}</button><button type="button" className="text-button" disabled={busy} onClick={() => void setAutomaticReportsEnabled(!schedule.enabled)}>{schedule.enabled ? t("暂停自动日报") : t("恢复自动日报")}</button></form></section>
      <section className="panel settings-card"><span className="settings-index">02</span><h2>{t("研究执行器")}</h2><p>{t("本地 Codex · read-only sandbox · 结构化输出")}</p><div className={`configuration-state ${settings.research_runner === "configured" ? "safe" : ""}`}>{settings.research_runner === "configured" ? t("已配置") : t("待配置环境变量")}</div></section>
      <section className="panel settings-card"><span className="settings-index">03</span><h2>{t("数据源")}</h2><p>{t("行情、公告与新闻 Adapter")}</p><div className={`configuration-state ${sourceSnapshots.length ? "safe" : ""}`}>{sourceSnapshots.length}  {t("份不可变来源快照")}</div></section>
      <section className="panel settings-card"><span className="settings-index">04</span><h2>{t("安全边界")}</h2><p>{t("只读研究 · 不执行交易")}</p><div className="configuration-state safe">{t("已启用")}</div></section>
      <section className="panel settings-card adapter-settings"><span className="settings-index">05</span><h2>{t("来源快照 Adapter")}</h2><p>{t("把行情、公告或新闻 Provider 的实际响应固化；报告只读取截止时间内的快照。")}</p><form onSubmit={saveSource}><div className="field-pair"><label>{t("提供商")}<input required value={source.provider} onChange={(event) => setSource({ ...source, provider: event.target.value })} /></label><label>{t("类型")}<input required value={source.source_type} onChange={(event) => setSource({ ...source, source_type: event.target.value })} /></label></div><label>{t("数据截止")}<input type="datetime-local" required value={source.as_of} onChange={(event) => setSource({ ...source, as_of: event.target.value })} /></label><label>{t("原始数据 JSON")}<textarea required value={source.payload} onChange={(event) => setSource({ ...source, payload: event.target.value })} /></label><button disabled={busy}>{t("固化来源快照")}</button></form><div className="source-snapshot-list">{sourceSnapshots.slice(0, 6).map((item) => <div key={item.id}><strong>{item.provider} · {item.source_type}</strong><span>{formatTime(item.as_of)} · {item.checksum.slice(0, 12)}</span></div>)}</div></section>
      {status ? <div className={`form-status ${status.tone} settings-status`} role={status.tone === "error" ? "alert" : "status"}>{status.raw ? status.message : feedback(status.message)}</div> : null}
    </div>
  );
}
