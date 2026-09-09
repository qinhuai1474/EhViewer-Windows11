import { useEffect, useState } from "react";
import {
  cookieSave,
  cookieVerify,
  exportData,
  importData,
  importTagTranslation,
  networkDiag,
  settingsGet,
  settingsSet,
  type NetDiag,
} from "../lib/api";
import type { AppSettings, CookiePair } from "../lib/types";
import "./settings.css";

const SITES = [
  { value: 0, label: "E-Hentai（公共）" },
  { value: 1, label: "ExHentai（需登录）" },
];
const DIRECTIONS = [
  { value: "ltr", label: "从左到右" },
  { value: "rtl", label: "从右到左" },
  { value: "vertical", label: "竖向滚动" },
];
const ZOOMS = [
  { value: "original", label: "原尺寸" },
  { value: "fit_width", label: "适配宽度" },
  { value: "fit_height", label: "适配高度" },
  { value: "fit_screen", label: "适配屏幕" },
  { value: "custom", label: "固定缩放" },
];
const THUMBS = [
  { value: 0, label: "默认" },
  { value: 1, label: "250" },
  { value: 2, label: "300" },
];
const STARTS = [
  { value: "first", label: "首页开始" },
  { value: "resume", label: "上次阅读位置" },
];
const PROXY_TYPES = [
  { value: 0, label: "直连" },
  { value: 1, label: "系统代理" },
  { value: 2, label: "HTTP 代理" },
  { value: 3, label: "SOCKS5 代理" },
];

export function SettingsPage() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const [memberId, setMemberId] = useState("");
  const [passHash, setPassHash] = useState("");
  const [igneous, setIgneous] = useState("");
  const [verifyMsg, setVerifyMsg] = useState<string | null>(null);
  const [importResult, setImportResult] = useState<string | null>(null);
  const [tagPath, setTagPath] = useState("");
  const [tagMsg, setTagMsg] = useState<string | null>(null);
  const [diag, setDiag] = useState<NetDiag | null>(null);
  const [diagBusy, setDiagBusy] = useState(false);

  useEffect(() => {
    settingsGet().then(setSettings).catch((e) => setErr(String(e)));
  }, []);
  useEffect(() => {
    if (settings?.tag_translation_file) setTagPath(settings.tag_translation_file);
  }, [settings]);

  const apply = (key: keyof AppSettings | string, value: unknown) => {
    setSettings((prev) => (prev ? { ...prev, [key]: value } : prev));
    setFlash(null);
    settingsSet(key, value)
      .then((s) => {
        setSettings(s);
        setFlash("已保存");
      })
      .catch((e) => setErr(String(e)));
  };

  const saveCookies = () => {
    const pairs: CookiePair[] = [];
    if (memberId.trim()) pairs.push({ name: "ipb_member_id", value: memberId.trim() });
    if (passHash.trim()) pairs.push({ name: "ipb_pass_hash", value: passHash.trim() });
    if (igneous.trim()) pairs.push({ name: "igneous", value: igneous.trim() });
    if (!settings) return;
    cookieSave(settings.site, pairs)
      .then((s) => {
        setSettings(s);
        setErr(null);
        setFlash("Cookie 已保存（写入时以 DPAPI 加密）");
      })
      .catch((e) => setErr(String(e)));
  };

  const verifySite = () => {
    if (!settings) return;
    setVerifyMsg("验证中…");
    cookieVerify(settings.site)
      .then((r) => setVerifyMsg(r.message))
      .catch((e) => setVerifyMsg(String(e)));
  };

  const importTags = async () => {
    if (!tagPath.trim()) {
      setTagMsg("请先填写翻译库文件路径");
      return;
    }
    try {
      const r = await importTagTranslation(tagPath.trim());
      setTagMsg(`已导入 tag 翻译库（${r.count} 条）`);
      settingsGet().then(setSettings).catch((e) => setErr(String(e)));
    } catch (e) {
      setErr(String(e));
    }
  };

  const doExport = async () => {
    try {
      const json = await exportData();
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `ehviewer-backup-${new Date().toISOString().slice(0, 10)}.json`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
      setFlash("已导出（若未弹出下载，请查收系统下载目录）");
      setErr(null);
    } catch (e) {
      setErr(String(e));
    }
  };

  const runNetworkDiag = async () => {
    if (diagBusy) return;
    setDiagBusy(true);
    setDiag(null);
    try {
      const r = await networkDiag();
      setDiag(r);
    } catch (e) {
      setErr(String(e));
    } finally {
      setDiagBusy(false);
    }
  };

  const onImportFile = async (file: File | null) => {
    if (!file) return;
    try {
      const text = await file.text();
      const r = await importData(text);
      setImportResult(`导入完成：下载 ${r.importedDownloads} 项，阅读记录 ${r.importedProgress} 项`);
      setErr(null);
      settingsGet().then((s) => setSettings(s)).catch((e) => setErr(String(e)));
    } catch (e) {
      setErr(String(e));
    }
  };

  if (!settings) {
    return (
      <section className="page">
        <h1 className="page-title">设置</h1>
        {err ? <p className="st-err">{err}</p> : <p className="page-hint">加载设置…</p>}
      </section>
    );
  }

  const s = settings;

  return (
    <section className="page settings">
      <div className="st-head">
        <h1 className="page-title">设置</h1>
        {flash && <span className="st-flash">{flash}</span>}
      </div>
      {err && <p className="st-err">{err}</p>}

      <Section title="EH">
        <div className="st-grid">
          <SelectSetting label="站点" value={s.site} options={SITES} onChange={(v) => apply("site", Number(v))} />
          <SwitchSetting label="启用标签翻译" checked={s.tag_translation_enabled} onChange={(v) => apply("tag_translation_enabled", v)} />
          <SelectSetting label="缩略图分辨率" value={s.thumb_resolution} options={THUMBS} onChange={(v) => apply("thumb_resolution", Number(v))} />
        </div>
        <div className="st-cookie">
          <div className="st-title-sub">Tag 翻译库（EhTagDatabase 二进制，可选）</div>
          <div className="st-actions">
            <input
              className="st-tagpath"
              placeholder="例：C:/…/data.bin"
              value={tagPath}
              onChange={(e) => setTagPath(e.target.value)}
            />
            <button onClick={importTags}>导入</button>
            {tagMsg && <span className="st-hint">{tagMsg}</span>}
          </div>
        </div>

        <div className="st-cookie">
          <div className="st-title-sub">身份 Cookie（EX 站点访问受限画廊）</div>
          <div className="st-grid">
            <CookieInput label="ipb_member_id" value={memberId} onChange={setMemberId} />
            <CookieInput label="ipb_pass_hash" value={passHash} onChange={setPassHash} />
            <CookieInput label="igneous" value={igneous} onChange={setIgneous} />
          </div>
          <div className="st-actions">
            <button onClick={saveCookies}>保存 Cookie</button>
            <button onClick={verifySite}>验证站点</button>
            {verifyMsg && <span className="st-hint">{verifyMsg}</span>}
          </div>
        </div>
      </Section>

      <Section title="阅读">
        <div className="st-grid">
          <SelectSetting label="翻页方向" value={s.reading_direction} options={DIRECTIONS} onChange={(v) => apply("reading_direction", v)} />
          <SelectSetting label="缩放模式" value={s.zoom_mode} options={ZOOMS} onChange={(v) => apply("zoom_mode", v)} />
          <SelectSetting label="起始位置" value={s.reading_start_position} options={STARTS} onChange={(v) => apply("reading_start_position", v)} />
        </div>
      </Section>

      <Section title="下载">
        <div className="st-grid">
          <TextSetting label="下载目录" value={s.download_dir ?? ""} placeholder="留空使用 图片/EhViewer" onChange={(v) => apply("download_dir", v)} />
          <NumberSetting label="并发线程数（1–10）" value={s.download_threads} onChange={(v) => apply("download_threads", Math.max(1, Math.min(10, v)))} />
          <NumberSetting label="列表每页条数" value={s.download_list_page_size} onChange={(v) => apply("download_list_page_size", Math.max(1, v))} />
          <SwitchSetting label="始终下载原图" checked={s.download_always_original} onChange={(v) => apply("download_always_original", v)} />
          <NumberSetting label="下载间隔（秒）" value={s.download_interval_secs} onChange={(v) => apply("download_interval_secs", Math.max(0, v))} />
        </div>
      </Section>

      <Section title="隐私">
        <div className="st-actions">
          <button onClick={doExport}>导出数据</button>
          <label className="st-file">
            导入数据
            <input type="file" accept=".json,application/json" onChange={(e) => onImportFile(e.target.files?.[0] ?? null)} />
          </label>
        </div>
        {importResult && <p className="st-hint">{importResult}</p>}
      </Section>

      <Section title="高级">
        <div className="st-grid">
          <SelectSetting label="代理类型" value={s.proxy_type} options={PROXY_TYPES} onChange={(v) => apply("proxy_type", Number(v))} />
          <TextSetting label="代理地址（host:port，HTTP/SOCKS5 时使用）" placeholder={s.proxy_type === 3 ? "127.0.0.1:1080" : "127.0.0.1:7890"} value={s.proxy_url ?? ""} onChange={(v) => apply("proxy_url", v)} />
          <NumberSetting label="请求超时（秒）" value={s.timeout_secs} onChange={(v) => apply("timeout_secs", Math.max(1, v))} />
          <NumberSetting label="下载重试次数" value={s.max_retries} onChange={(v) => apply("max_retries", Math.max(0, v))} />
          <NumberSetting label="图片缓存（MB）" value={s.image_cache_size_mb} onChange={(v) => apply("image_cache_size_mb", Math.max(1, v))} />
          <TextSetting label="自定义 Host（自建镜像，留空禁用）" value={s.custom_host ?? ""} onChange={(v) => apply("custom_host", v)} />
          <TextAreaSetting label="自定义 Hosts（每行：域名 IP[,IP…]，优先级最高）" value={s.hosts_override} onChange={(v) => apply("hosts_override", v)} />
          <TextSetting label="DoH 解析器" value={s.doh_url} onChange={(v) => apply("doh_url", v)} />
          <SwitchSetting label="使用内置站点 IP 表" checked={s.use_builtin_hosts} onChange={(v) => apply("use_builtin_hosts", v)} />
        </div>
        <p className="st-hint">内置站点 IP 表 + DoH 可缓解 DNS 污染导致的连接失败；若直连提示 SNI-DPI 阻断，请选择 HTTP/SOCKS5 代理或开启系统代理。自定义 Hosts 会最优先采用。</p>
        <div className="st-actions">
          <button onClick={runNetworkDiag} disabled={diagBusy}>{diagBusy ? "诊断中…" : "网络诊断"}</button>
        </div>
        {diag && (
          <div className="st-diag">
            <p className="st-hint">代理：{diag.proxy || "（直连）"} · 结论：{diag.ok ? "可达（可尝试刷新页面）" : "不可达，见下面分项"}</p>
            <div className="st-diag-list">
              {diag.steps.map((st, i) => (
                <div key={i} className="st-diag-row">
                  <span className="st-diag-label">{st.label}</span>
                  <span className={"st-diag-status " + (st.status === "失败" || st.status.includes("污染") || st.status.includes("超时") || st.status.includes("无候选") || st.status.includes("被拒") || st.status === "端口拒绝" ? "bad" : "")}>
                    {st.status}
                  </span>
                  <span className="st-diag-detail">{st.detail}</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </Section>
    </section>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="st-section">
      <h2 className="st-title">{title}</h2>
      {children}
    </section>
  );
}

function SelectSetting(props: {
  label: string;
  value: string | number;
  options: { value: string | number; label: string }[];
  onChange: (v: any) => void;
}) {
  return (
    <label className="st-field">
      <span>{props.label}</span>
      <select value={props.value} onChange={(e) => props.onChange(e.target.value)}>
        {props.options.map((o) => (
          <option key={o.value} value={o.value}>{o.label}</option>
        ))}
      </select>
    </label>
  );
}

function SwitchSetting(props: { label: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="st-field st-switch">
      <span>{props.label}</span>
      <input type="checkbox" checked={props.checked} onChange={(e) => props.onChange(e.target.checked)} />
    </label>
  );
}

function CookieInput(props: { label: string; value: string; onChange: (v: string) => void }) {
  return (
    <label className="st-field">
      <span>{props.label}</span>
      <input type="text" value={props.value} onChange={(e) => props.onChange(e.target.value)} />
    </label>
  );
}

function TextSetting(props: { label: string; value: string; placeholder?: string; onChange: (v: string) => void }) {
  const [draft, setDraft] = useState(props.value);
  useEffect(() => setDraft(props.value), [props.value]);
  const commit = () => props.onChange(draft);
  return (
    <label className="st-field">
      <span>{props.label}</span>
      <input
        type="text"
        value={draft}
        placeholder={props.placeholder}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
      />
    </label>
  );
}

function NumberSetting(props: { label: string; value: number; onChange: (v: number) => void }) {
  const [draft, setDraft] = useState(String(props.value));
  useEffect(() => setDraft(String(props.value)), [props.value]);
  const commit = () => props.onChange(Number(draft) || 0);
  return (
    <label className="st-field">
      <span>{props.label}</span>
      <input
        type="number"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
      />
    </label>
  );
}

function TextAreaSetting(props: { label: string; value: string; placeholder?: string; onChange: (v: string) => void }) {
  const [draft, setDraft] = useState(props.value);
  useEffect(() => setDraft(props.value), [props.value]);
  const commit = () => props.onChange(draft);
  return (
    <label className="st-field">
      <span>{props.label}</span>
      <textarea
        value={draft}
        rows={4}
        placeholder={props.placeholder}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
      />
    </label>
  );
}
