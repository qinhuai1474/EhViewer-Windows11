import type { PageRoute } from "../types";
import { HomeIcon, PopularIcon, DownloadsIcon, SettingsIcon } from "./icons";
import "./Sidebar.css";

interface SidebarProps {
  route: PageRoute;
  onNavigate: (route: PageRoute) => void;
}

const items: { route: PageRoute; label: string; Icon: typeof HomeIcon }[] = [
  { route: "home", label: "主页", Icon: HomeIcon },
  { route: "popular", label: "热门", Icon: PopularIcon },
  { route: "downloads", label: "下载", Icon: DownloadsIcon },
  { route: "settings", label: "设置", Icon: SettingsIcon },
];

export function Sidebar({ route, onNavigate }: SidebarProps) {
  return (
    <nav className="sidebar" aria-label="主导航">
      {items.map(({ route: r, label, Icon }) => (
        <button
          key={r}
          type="button"
          className={"sidebar-item" + (r === route ? " active" : "")}
          onClick={() => onNavigate(r)}
          title={label}
          aria-label={label}
          aria-current={r === route ? "page" : undefined}
        >
          <Icon />
        </button>
      ))}
    </nav>
  );
}
