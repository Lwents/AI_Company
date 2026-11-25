import { Component, type ErrorInfo, type ReactNode } from "react";

/** Keeps one broken panel from blanking the whole page. */
export class ErrorBoundary extends Component<{ children: ReactNode; label?: string }, { error: Error | null }> {
  state: { error: Error | null } = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[AI Company] lỗi giao diện:", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="card crash">
        <b>⚠ {this.props.label ?? "Phần này"} gặp lỗi hiển thị</b>
        <p className="small muted">{String(this.state.error.message || this.state.error)}</p>
        <div className="row gap4">
          <button className="primary" onClick={() => this.setState({ error: null })}>
            Thử lại
          </button>
          <button className="ghost" onClick={() => location.reload()}>
            Tải lại trang
          </button>
        </div>
      </div>
    );
  }
}
