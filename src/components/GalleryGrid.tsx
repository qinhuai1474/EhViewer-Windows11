import type { GalleryItem } from "../types";
import { GalleryCard } from "./GalleryCard";

interface Props {
  items: GalleryItem[];
  onOpen: (item: GalleryItem) => void;
}

export function GalleryGrid({ items, onOpen }: Props) {
  return (
    <div className="ggrid">
      {items.map((item) => (
        <GalleryCard key={item.gid} item={item} onClick={onOpen} />
      ))}
    </div>
  );
}
