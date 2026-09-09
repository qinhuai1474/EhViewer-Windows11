import { Component, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

/**
 * Catches render-time errors so a crashing view never leaves a blank black
 * window; instead it shows the real message and a way back.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error) {
    console.error("EhViewer render error:", error);
  }

  render() {
    if (this.state.error) {
      return (
        <div
          style={{
            padding: 24,
            fontFamily: "system-ui, sans-serif",
            color: "#eee",
            background: "#1b1d22",
            minHeight: "100vh",
            boxSizing: "border-box",
          }}
        >
          <h2>界面渲染出错</h2>
          <p style={{ whiteSpace: "pre-wrap", wordBreak: "break-all" }}>
            {String(this.state.error?.message || this.state.error)}
          </p>
          <p className="v-dim">以上为真实错误信息，可复制发给开发者。</p>
          <button
            className="primary"
            onClick={() => {
              this.setState({ error: null });
              window.location.reload();
            }}
          >
            重新加载
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
