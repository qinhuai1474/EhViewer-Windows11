import type { GalleryItem } from "../types";
import { GalleryBrowser } from "../components/GalleryBrowser";

interface Props {
  onOpen?: (item: GalleryItem) => void;
}

export function HomePage({ onOpen }: Props) {
  return (
    <section className="page">
      <h1 className="page-title">主页</h1>
      <GalleryBrowser showSearch onOpen={onOpen} />
    </section>
  );
}
