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
  renameGalleryDirs,
  DEFAULT_RENAME_TERMS,
  type RenameReport,
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
  const [renameTerms, setRenameTerms] = useState<string[]>([]);
  const [termInput, setTermInput] = useState("");
  const [renamePreview, setRenamePreview] = useState<RenameReport | null>(null);
  const [renameBusy, setRenameBusy] = useState(false);

  useEffect(() => {
    settingsGet().then(setSettings).catch((e) => setErr(String(e)));
  }, []);
  useEffect(() => {
    if (settings?.tag_translation_file) setTagPath(settings.tag_translation_file);
  }, [settings]);
  useEffect(() => {
    if (!settings) return;
    setRenameTerms(
      settings.rename_filter_terms?.length ? settings.rename_filter_terms : DEFAULT_RENAME_TERMS,
    );
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

  const applyRenameTerms = (terms: string[]) => {
    setRenameTerms(terms);
    setSettings((prev) => (prev ? { ...prev, rename_filter_terms: terms } : prev));
    settingsSet("rename_filter_terms", terms)
      .then((s) => {
        setSettings(s);
        setFlash("已保存");
      })
      .catch((e) => setErr(String(e)));
  };

  const addTerm = () => {
    const parts = termInput
      .split(/[\s,，;；]+/)
      .map((t) => t.trim())
      .filter(Boolean);
    if (!parts.length) return;
    applyRenameTerms(Array.from(new Set([...renameTerms, ...parts])));
    setTermInput("");
  };

  const removeTerm = (t: string) => applyRenameTerms(renameTerms.filter((x) => x !== t));

  const resetTerms = () => {
    setTermInput("");
    setRenameTerms(DEFAULT_RENAME_TERMS);
    settingsSet("rename_filter_terms", [])
      .then((s) => {
        setSettings(s);
        setFlash("已重置为默认过滤清单");
      })
      .catch((e) => setErr(String(e)));
  };

  const scanPreview = async () => {
    setRenameBusy(true);
    setErr(null);
    try {
      setRenamePreview(await renameGalleryDirs(true));
    } catch (e) {
      setErr(String(e));
    } finally {
      setRenameBusy(false);
    }
  };

  const confirmRename = async () => {
    if (!renamePreview) return;
    setRenameBusy(true);
    setErr(null);
    try {
      const r = await renameGalleryDirs(false);
      setRenamePreview(null);
      setFlash(`已重命名 ${r.renamed} 项并移出队列（跳过 ${r.skipped}，失败 ${r.failed}）`);
    } catch (e) {
      setErr(String(e));
    } finally {
      setRenameBusy(false);
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
          <SwitchSetting label="关闭主界面时最小化到托盘（后台下载）" checked={s.close_to_tray} onChange={(v) => apply("close_to_tray", v)} />
        </div>
      </Section>

      <Section title="画廊重命名">
        <p className="st-hint">
          仅处理下方固定扫描目录内的画廊文件夹（含其子目录），把
          <code> gid-标题 </code> 重命名为 <code>[作者] 作品</code>，并从下载队列移除（磁盘文件保留）；该目录之外一律不改动。
        </p>
        <div className="st-grid" style={{ marginTop: 10 }}>
          <TextSetting
            label="扫描目录（固定路径）"
            value={s.rename_scan_dir ?? ""}
            placeholder="留空使用下载根目录"
            onChange={(v) => apply("rename_scan_dir", v)}
          />
        </div>
        <CollapseSection title="标签库（过滤无用标签）">
          <div className="rn-terms">
            {renameTerms.map((t) => (
              <span key={t} className="rn-chip">
                {t}
                <button onClick={() => removeTerm(t)} aria-label={`移除 ${t}`}>×</button>
              </span>
            ))}
          </div>
          <div className="rn-add">
            <input
              className="rn-input"
              placeholder="输入标签关键词，回车添加，支持空格 / 逗号分隔…"
              value={termInput}
              onChange={(e) => setTermInput(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && addTerm()}
            />
            <button onClick={addTerm} disabled={!termInput.trim()}>添加</button>
            <button onClick={resetTerms}>重置为默认</button>
          </div>
        </CollapseSection>
        <div className="st-actions">
          <button className="primary" onClick={scanPreview} disabled={renameBusy}>
            {renameBusy ? "处理中…" : "扫描并预览"}
          </button>
        </div>
        <p className="st-hint">先扫描预览再执行；取消不会有任何改动。</p>
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

      {renamePreview && (
        <RenamePreviewModal
          report={renamePreview}
          busy={renameBusy}
          onConfirm={confirmRename}
          onClose={() => setRenamePreview(null)}
        />
      )}
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

function CollapseSection({
  title,
  defaultOpen = false,
  children,
}: {
  title: string;
  defaultOpen?: boolean;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <section className="st-section">
      <button
        className="st-collapse-head"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
      >
        <span className="st-collapse-title">{title}</span>
        <span className="st-collapse-arrow">{open ? "▾" : "▸"}</span>
      </button>
      {open && children}
    </section>
  );
}

function RenamePreviewModal({
  report,
  busy,
  onConfirm,
  onClose,
}: {
  report: RenameReport;
  busy: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  return (
    <div className="rn-modal-backdrop" onMouseDown={onClose}>
      <div className="rn-modal" onMouseDown={(e) => e.stopPropagation()}>
        <div className="rn-modal-title">
          重命名预览（将重命名 {report.renamed} 项
          {report.skipped > 0 ? `，跳过 ${report.skipped} 项` : ""}
          {report.failed > 0 ? `，失败 ${report.failed} 项` : ""}）
        </div>
        <div className="rn-modal-scroll">
          <table className="rn-preview">
            <thead>
              <tr>
                <th>重命名前</th>
                <th>重命名后</th>
                <th>状态</th>
              </tr>
            </thead>
            <tbody>
              {report.entries.map((e, i) => (
                <tr key={`${e.gid}-${i}`}>
                  <td className={`rn-old${e.status === "skipped" ? " skip" : ""}`}>{e.oldName}</td>
                  <td className="rn-new">{e.newName || "—"}</td>
                  <td className={`rn-status ${e.status}`}>
                    {e.status === "renamed" ? "改名" : e.status}
                    {e.reason ? <span className="rn-reason">{e.reason}</span> : null}
                  </td>
                </tr>
              ))}
              {report.entries.length === 0 && (
                <tr>
                  <td colSpan={3} className="rn-empty">未在下载目录下发现可重命名的画廊文件夹。</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
        <div className="rn-modal-actions">
          <button onClick={onClose} disabled={busy}>取消</button>
          <button className="danger" onClick={onConfirm} disabled={busy || report.renamed === 0}>
            {busy ? "处理中…" : "确认重命名并从队列移除"}
          </button>
        </div>
      </div>
    </div>
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
