import { useState } from "react";
import type { GalleryItem } from "../../types";
import { DetailPage } from "./DetailPage";
import { PreviewsPage } from "./PreviewsPage";
import { ReaderPage } from "./ReaderPage";

type Stage = "detail" | "previews" | "reader";

interface Props {
  item: GalleryItem;
  onExit: () => void;
}

export function GalleryView({ item, onExit }: Props) {
  const [stage, setStage] = useState<Stage>("detail");
  const [readIndex, setReadIndex] = useState(0);

  const goRead = (index: number) => {
    setReadIndex(index);
    setStage("reader");
  };

  if (stage === "reader") {
    return (
      <ReaderPage
        item={item}
        startIndex={readIndex}
        onBack={() => setStage("detail")}
      />
    );
  }
  if (stage === "previews") {
    return (
      <PreviewsPage
        item={item}
        onBack={() => setStage("detail")}
        onRead={goRead}
      />
    );
  }
  return (
    <DetailPage
      item={item}
      onBack={onExit}
      onPreviews={() => setStage("previews")}
      onRead={goRead}
    />
  );
}
