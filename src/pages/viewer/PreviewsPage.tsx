import { useCallback, useEffect, useState } from "react";
import type { GalleryItem } from "../../types";
import type { PreviewItem } from "../../lib/types";
import { getPreviewSet } from "../../lib/api";
import { RemoteThumb } from "../../components/RemoteThumb";
import "./viewer.css";

const PER_PAGE = 20;

interface Props {
  item: GalleryItem;
  onBack: () => void;
  onRead: (index: number) => void;
}

export function PreviewsPage({ item, onBack, onRead }: Props) {
  const [pv, setPv] = useState<PreviewItem[]>([]);
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const totalPreviewPages = Math.max(1, Math.ceil(item.pages / PER_PAGE));

  const load = useCallback(
    async (p: number) => {
      setLoading(true);
      setError(null);
      try {
        const list = await getPreviewSet(0, item.gid, item.token, p);
        setPv(list);
        setPage(p);
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    },
    [item.gid, item.token],
  );

  useEffect(() => {
    load(0);
  }, [load]);

  return (
    <section className="page v-previews">
      <div className="v-head">
        <button className="back" onClick={onBack}>← 返回详情</button>
        <span className="v-dim">{item.title}</span>
      </div>
      <h1 className="page-title">缩略图</h1>

      {error ? (
        <div className="state"><p>{error}</p><button onClick={() => load(page)}>重试</button></div>
      ) : loading ? (
        <div className="state">加载缩略图…</div>
      ) : pv.length === 0 ? (
        <div className="state">没有缩略图</div>
      ) : (
        <div className="v-pv-grid">
          {pv.map((p) => (
            <button key={p.index} className="v-pv-item" onClick={() => onRead(p.index)} title={`第 ${p.index + 1} 页`}>
              <RemoteThumb
                url={p.imageUrl}
                alt=""
                x={p.xOffset}
                y={p.yOffset}
                w={p.clipWidth}
                h={p.clipHeight}
                loading="lazy"
              />
              <span>{p.index + 1}</span>
            </button>
          ))}
        </div>
      )}

      <div className="pagination">
        <div className="pagination-buttons">
          <button disabled={page <= 0} onClick={() => load(0)}>«</button>
          <button disabled={page <= 0} onClick={() => load(page - 1)}>‹</button>
          <span className="pagination-pos">{page + 1} / {totalPreviewPages}</span>
          <button disabled={page >= totalPreviewPages - 1} onClick={() => load(page + 1)}>›</button>
          <button disabled={page >= totalPreviewPages - 1} onClick={() => load(totalPreviewPages - 1)}>»</button>
        </div>
        <button className="primary" onClick={() => onRead(page * PER_PAGE)}>从此页开始阅读</button>
      </div>
    </section>
  );
}

