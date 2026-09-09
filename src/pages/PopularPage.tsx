import type { GalleryItem } from "../types";
import { GalleryBrowser } from "../components/GalleryBrowser";

interface Props {
  onOpen?: (item: GalleryItem) => void;
}

export function PopularPage({ onOpen }: Props) {
  return (
    <section className="page">
      <GalleryBrowser showSearch={false} onOpen={onOpen} />
    </section>
  );
}
