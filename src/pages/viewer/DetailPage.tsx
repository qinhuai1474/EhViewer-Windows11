import { useEffect, useState } from "react";
import type { GalleryItem } from "../../types";
import { downloadStart, getGalleryDetail } from "../../lib/api";
import { RemoteImg } from "../../components/RemoteImg";
import "./viewer.css";

interface Props {
  item: GalleryItem;
  onBack: () => void;
  onPreviews: () => void;
  onRead: (index: number) => void;
}

export function DetailPage({ item, onBack, onPreviews, onRead }: Props) {
  const [detail, setDetail] = useState<import("../../lib/types").GalleryDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showJpn, setShowJpn] = useState(false);
  const [dlMsg, setDlMsg] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setDlMsg(null);
    getGalleryDetail(0, item.gid, item.token)
      .then((d) => alive && setDetail(d))
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [item.gid]);

  const d = detail;
  const title = showJpn && d?.titleJpn ? d.titleJpn : (d?.title ?? item.title);

  const onDownload = () => {
    if (!d) return;
    setDlMsg(null);
    // GalleryItem has no `url`; use the canonical detail URL (metadata only).
    const galleryUrl = `https://e-hentai.org/g/${item.gid}/${item.token}/`;
    downloadStart(item.gid, item.token, d.title || item.title, "", d.pages, galleryUrl)
      .then(() => setDlMsg("已加入下载队列"))
      .catch((e) => setDlMsg("下载失败：" + String(e)));
  };

  return (
    <section className="page v-detail">
      <button className="back" onClick={onBack}>← 返回</button>
      {error ? (
        <div className="state">
          <p>详情加载失败：{error}</p>
        </div>
      ) : !d ? (
        <div className="state">加载详情…</div>
      ) : (
        <>
          <div className="v-detail-top">
            <div className="v-detail-cover">
              <RemoteImg url={d.thumb} alt="" />
            </div>
            <div className="v-detail-meta">
              <h1 className="v-title">{title}</h1>
              <label className="v-title-toggle">
                <input type="checkbox" checked={showJpn} onChange={(e) => setShowJpn(e.currentTarget.checked)} />
                显示日文标题
              </label>
              <ul className="v-meta-list">
                <Meta label="分类" value={d.category} />
                <Meta label="上传者" value={d.uploader} />
                <Meta label="发布时间" value={d.posted} />
                <Meta label="语言" value={d.language} />
                <Meta label="大小" value={d.size} />
                <Meta label="页数" value={`${d.pages}`} />
                <Meta label="评分" value={d.rating >= 0 ? `${d.rating.toFixed(2)}（${d.ratingCount} 票）` : "未评分"} />
                <Meta label="收藏" value={`${d.favoriteCount}`} />
              </ul>
              <div className="v-actions">
                <button className="primary" onClick={() => onRead(0)}>开始阅读</button>
                <button onClick={onPreviews}>预览缩略图</button>
                <button onClick={onDownload}>下载</button>
              </div>
              {dlMsg && <p className="v-dim">{dlMsg}</p>}
            </div>
          </div>

          <h2 className="v-section">标签</h2>
          <div className="v-tags">
            {(d.tags ?? []).length === 0 ? (
              <span className="v-empty">无标签</span>
            ) : (
              d.tags.map((g) => (
                <div key={g.name} className="v-tag-group">
                  <span className="v-tag-ns">{g.name}</span>
                  {g.tags.map((t) => (
                    <span key={t} className="v-tag">{t}</span>
                  ))}
                </div>
              ))
            )}
          </div>

          {(d.comments ?? []).length > 0 && (
            <>
              <h2 className="v-section">评论</h2>
              {d.comments.map((c) => (
                <div key={c.id ?? c.user} className="v-comment">
                  <b>{c.user}</b> <span className="v-dim">{c.time} · {c.score} 分</span>
                  <p>{c.comment}</p>
                </div>
              ))}
            </>
          )}
        </>
      )}
    </section>
  );
}

function Meta({ label, value }: { label: string; value: string }) {
  return (
    <li>
      <span className="v-dim">{label}：</span>
      {value}
    </li>
  );
}
