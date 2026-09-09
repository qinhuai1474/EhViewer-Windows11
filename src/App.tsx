import { useState } from "react";
import type { ComponentType } from "react";
import type { PageRoute, GalleryItem } from "./types";
import { Sidebar } from "./components/Sidebar";
import { HomePage } from "./pages/HomePage";
import { PopularPage } from "./pages/PopularPage";
import { DownloadsPage } from "./pages/DownloadsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { GalleryView } from "./pages/viewer/GalleryView";
import "./App.css";

type PageProps = { onOpen?: (item: GalleryItem) => void };

const pages: Record<PageRoute, ComponentType<PageProps>> = {
  home: HomePage,
  popular: PopularPage,
  downloads: DownloadsPage,
  settings: SettingsPage,
};

function App() {
  const [route, setRoute] = useState<PageRoute>("home");
  const [openItem, setOpenItem] = useState<GalleryItem | null>(null);
  const Page = pages[route];

  const openDetail = (item: GalleryItem) => setOpenItem(item);
  // Sidebar stays usable while a gallery is open: picking any section closes
  // the gallery and jumps straight there (no round-trip through the source page).
  const navigate = (r: PageRoute) => {
    setRoute(r);
    setOpenItem(null);
  };

  if (openItem) {
    return (
      <div className="app">
        <Sidebar route={route} onNavigate={navigate} />
        <main className="content">
          <GalleryView item={openItem} onExit={() => setOpenItem(null)} />
        </main>
      </div>
    );
  }

  return (
    <div className="app">
      <Sidebar route={route} onNavigate={navigate} />
      <main className="content">
        <Page onOpen={openDetail} />
      </main>
    </div>
  );
}

export default App;
