import { useEffect, useState } from "react";
import type { ConfigDto, LlmProvider, TransferPolicy, TransferRecord } from "../../shared/contracts";
import { ipc } from "../../shared/ipc";
import { SourcesView } from "../sources/SourcesView";
import type { SourcesApi } from "../sources/api";
import "./settings.css";

const POLICIES: { value: TransferPolicy; label: string; desc: string }[] = [
  { value: "MaskAndMinimize", label: "개인정보를 가리고 전송", desc: "식별 정보를 가리고 필요한 내용만 AI에 전달합니다." },
  { value: "AllowAll", label: "원본 그대로 전송", desc: "개인정보를 포함한 원본 내용을 AI에 전달합니다." },
  { value: "LocalOnlyNoLlm", label: "기기 안에서만 사용", desc: "클라우드 AI에 데이터를 전송하지 않습니다." },
];

const PROVIDERS: { value: LlmProvider; label: string; desc: string; needsKey: boolean }[] = [
  { value: "claude-cli", label: "로컬 Claude Code", desc: "설치된 claude CLI를 구동합니다. API 키가 필요 없습니다.", needsKey: false },
  { value: "anthropic", label: "Anthropic API", desc: "API 키로 api.anthropic.com에 직접 연결합니다.", needsKey: true },
  { value: "openai", label: "OpenAI 호환", desc: "OpenAI·OpenRouter 등 호환 게이트웨이에 연결합니다.", needsKey: true },
];
export type SettingsTab = "general" | "sources" | "transfers";
const TABS: { id: SettingsTab; title: string }[] = [
  { id: "general", title: "일반" }, { id: "sources", title: "연결 소스" }, { id: "transfers", title: "전송 기록" },
];

export function HomeView({ onLock, initialTab = "general", sourcesApi, onIngested }: { onLock: () => void; initialTab?: SettingsTab; sourcesApi: SourcesApi; onIngested?: () => void }) {
  const [tab, setTab] = useState<SettingsTab>(initialTab);
  const [config, setConfig] = useState<ConfigDto | null>(null);
  const [transfers, setTransfers] = useState<TransferRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  // Draft LLM settings the form edits before saving. `apiKey` is write-only:
  // it starts blank and, if left blank on save, the stored key is kept as-is.
  const [provider, setProvider] = useState<LlmProvider>("claude-cli");
  const [model, setModel] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [llmSaved, setLlmSaved] = useState(false);

  useEffect(() => { setTab(initialTab); }, [initialTab]);
  useEffect(() => {
    let active = true;
    void Promise.all([ipc.getConfig(), ipc.listTransfers()]).then(([nextConfig, records]) => {
      if (active) { setConfig(nextConfig); syncLlmForm(nextConfig); setTransfers(records); }
    }).catch(() => { if (active) setError("설정을 불러오지 못했습니다. 다시 열어 주세요."); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, []);

  // Reset the editable form to match a freshly loaded config. The key field is
  // always cleared — the real key never comes back over IPC.
  function syncLlmForm(cfg: ConfigDto) {
    setProvider(cfg.llm_provider);
    setModel(cfg.llm_model);
    setBaseUrl(cfg.llm_base_url ?? "");
    setApiKey("");
  }

  const providerInfo = PROVIDERS.find((p) => p.value === provider) ?? PROVIDERS[0];

  async function choose(policy: TransferPolicy) {
    setBusy(true); setError("");
    try { await ipc.setTransferPolicy(policy); setConfig(await ipc.getConfig()); }
    catch { setError("전송 설정을 저장하지 못했습니다. 다시 시도해 주세요."); }
    finally { setBusy(false); }
  }

  async function saveLlm() {
    setBusy(true); setError(""); setLlmSaved(false);
    try {
      await ipc.setLlmConfig({
        provider,
        model: model.trim(),
        base_url: providerInfo.needsKey ? (baseUrl.trim() || null) : null,
        // Blank → keep the stored key (send null); a typed value replaces it.
        api_key: apiKey.trim() ? apiKey.trim() : null,
      });
      const next = await ipc.getConfig();
      setConfig(next); syncLlmForm(next); setLlmSaved(true);
    } catch { setError("AI 설정을 저장하지 못했습니다. 다시 시도해 주세요."); }
    finally { setBusy(false); }
  }

  async function toggleServer(on: boolean) {
    setBusy(true); setError("");
    try { await ipc.setServerEnabled(on); setConfig(await ipc.getConfig()); }
    catch { setError("로컬 API 서버 설정을 저장하지 못했습니다. 다시 시도해 주세요."); }
    finally { setBusy(false); }
  }
  async function lock() {
    setBusy(true); setError("");
    try { await ipc.lock(); onLock(); }
    catch { setError("보관함을 잠그지 못했습니다. 다시 시도해 주세요."); }
    finally { setBusy(false); }
  }

  return <section className="settings-view" aria-label="설정">
    <header className="settings-heading"><h2>설정</h2><p>연결하는 기록부터 데이터가 전달되는 방식까지.</p></header>
    <div className="settings-tabs" role="tablist" aria-label="설정 항목">
      {TABS.map((item) => <button key={item.id} id={`settings-tab-${item.id}`} role="tab" aria-selected={tab === item.id} aria-controls={`settings-panel-${item.id}`} onClick={() => setTab(item.id)}>{item.title}{item.id === "transfers" && transfers.length > 0 && <span>{transfers.length}</span>}</button>)}
    </div>
    {error && <p role="alert" className="settings-error">{error}</p>}
    <div role="tabpanel" id={`settings-panel-${tab}`} aria-labelledby={`settings-tab-${tab}`}>
      {tab === "general" && <div className="settings-general">
        <section className="settings-block"><div className="settings-block-title"><h3>데이터 전송</h3><p>클라우드 AI가 사용할 수 있는 정보의 범위를 정합니다.</p></div>
          <fieldset className="settings-policies" disabled={loading || busy || !config}><legend className="sr-only">클라우드 전송 정책</legend>
            {POLICIES.map((p) => <label key={p.value} className={`settings-policy${config?.transfer_policy === p.value ? " is-selected" : ""}`}>
              <input type="radio" name="policy" checked={config?.transfer_policy === p.value} onChange={() => void choose(p.value)} />
              <span><strong>{p.label}{p.value === "MaskAndMinimize" && <small>권장</small>}</strong><span>{p.desc}</span></span>
            </label>)}
          </fieldset>
        </section>
        <section className="settings-block"><div className="settings-block-title"><h3>AI 모델</h3><p>지식 추출과 페르소나 대화에 사용할 AI를 정합니다.</p></div>
          <p className="settings-current">현재 사용 중: <strong>{config?.llm_label ?? (loading ? "불러오는 중" : "확인할 수 없음")}</strong></p>
          <fieldset className="settings-llm" disabled={loading || busy || !config}><legend className="sr-only">AI 백엔드 설정</legend>
            <div className="settings-providers" role="radiogroup" aria-label="AI 백엔드">
              {PROVIDERS.map((p) => <label key={p.value} className={`settings-provider${provider === p.value ? " is-selected" : ""}`}>
                <input type="radio" name="llm-provider" value={p.value} checked={provider === p.value} onChange={() => { setProvider(p.value); setLlmSaved(false); }} />
                <span><strong>{p.label}</strong><span>{p.desc}</span></span>
              </label>)}
            </div>
            <label className="settings-field"><span>모델 이름</span>
              <input type="text" value={model} onChange={(e) => { setModel(e.target.value); setLlmSaved(false); }} placeholder="claude-sonnet-5" spellCheck={false} autoCapitalize="off" autoCorrect="off" />
            </label>
            {providerInfo.needsKey && <>
              <label className="settings-field"><span>Base URL <small>(선택)</small></span>
                <input type="text" value={baseUrl} onChange={(e) => { setBaseUrl(e.target.value); setLlmSaved(false); }} placeholder={provider === "openai" ? "https://api.openai.com" : "https://api.anthropic.com"} spellCheck={false} autoCapitalize="off" autoCorrect="off" />
              </label>
              <label className="settings-field"><span>API 키 {config?.has_api_key && <small>(저장됨 · 비워 두면 유지)</small>}</span>
                <input type="password" value={apiKey} onChange={(e) => { setApiKey(e.target.value); setLlmSaved(false); }} placeholder={config?.has_api_key ? "••••••••" : "sk-..."} autoComplete="off" spellCheck={false} />
              </label>
            </>}
            <div className="settings-llm-actions">
              <button className="primary" type="button" onClick={() => void saveLlm()} disabled={busy || !model.trim()}>저장</button>
              {llmSaved && <span className="settings-saved" role="status">저장했습니다.</span>}
            </div>
          </fieldset>
        </section>
        <section className="settings-block"><div className="settings-block-title"><h3>로컬 API 서버</h3><p>다른 앱이 127.0.0.1에서 페르소나에 연결할 수 있게 합니다.</p></div>
          <label className="settings-toggle">
            <input type="checkbox" checked={config?.server_enabled ?? false} disabled={loading || busy || !config} onChange={(e) => void toggleServer(e.target.checked)} />
            <span>{config?.server_enabled ? "활성화됨" : "비활성화됨"}</span>
          </label>
        </section>
        <section className="settings-lock"><div><h3>보관함 잠금</h3><p>다시 열 때 비밀번호를 입력해야 합니다.</p></div><button className="secondary" onClick={() => void lock()} disabled={busy}>지금 잠그기</button></section>
      </div>}
      {tab === "sources" && <SourcesView api={sourcesApi} onIngested={onIngested} />}
      {tab === "transfers" && <section className="settings-transfer-section"><div className="settings-block-title"><h3>기기 밖으로 전달된 기록</h3><p>AI 요청에 사용된 모델과 전송 내용을 확인합니다.</p></div>
        {loading ? <p className="settings-empty">전송 기록을 불러오고 있습니다.</p> : transfers.length === 0 ? <div className="settings-empty"><svg width="30" height="30" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true"><path d="M7 3h10v5l3 4v9H4v-9l3-4Z"/><path d="M4 14h5l1 3h4l1-3h5M7 8h10"/></svg><h4>아직 전송된 기록이 없어요</h4><p>클라우드 AI에 요청을 보내면 여기에 기록됩니다.</p></div> : <ul className="settings-transfer-list">{transfers.map((t, i) => <li key={`${t.at}-${i}`}><div className="settings-transfer-top"><strong>{t.purpose}</strong><time dateTime={t.at}>{new Date(t.at).toLocaleString("ko-KR")}</time></div><p className="settings-transfer-meta">{t.model} · {t.bytes_sent.toLocaleString("ko-KR")} bytes</p><p className="settings-transfer-preview">{t.masked_preview}</p></li>)}</ul>}
      </section>}
    </div>
  </section>;
}
