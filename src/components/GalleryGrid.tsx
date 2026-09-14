import type { GalleryItem } from "../types";
import { GalleryCard } from "./GalleryCard";

interface Props {
  items: GalleryItem[];
  onOpen: (item: GalleryItem) => void;
  /** Gallery gids already present in the download queue. */
  downloadedGids: ReadonlySet<number>;
}

export function GalleryGrid({ items, onOpen, downloadedGids }: Props) {
  return (
    <div className="ggrid">
      {items.map((item) => (
        <GalleryCard
          key={item.gid}
          item={item}
          onClick={onOpen}
          queued={downloadedGids.has(item.gid)}
        />
      ))}
    </div>
  );
}
