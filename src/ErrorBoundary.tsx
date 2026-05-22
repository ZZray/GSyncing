import React from "react";

interface State {
  error?: Error;
  info?: React.ErrorInfo;
}

/**
 * Last line of defense against a white-screen-of-death. Renders the actual
 * exception + stack so the user has something to copy/paste back when
 * something inevitably blows up in a release build (where there's no
 * DevTools by default).
 */
export class ErrorBoundary extends React.Component<
  { children: React.ReactNode },
  State
> {
  constructor(props: { children: React.ReactNode }) {
    super(props);
    this.state = {};
  }

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo): void {
    // Keep both — `error` has the message/stack, `info` has the React
    // component stack which usually pinpoints which JSX threw.
    this.setState({ error, info });
    console.error("GSyncing render error:", error, info);
  }

  render() {
    if (this.state.error) {
      return (
        <div
          style={{
            padding: 28,
            fontFamily:
              '"Inter", -apple-system, "Microsoft YaHei", sans-serif',
            color: "#222",
          }}
        >
          <h2 style={{ color: "#cf1322", margin: "0 0 12px" }}>
            GSyncing 启动失败
          </h2>
          <p style={{ color: "#555" }}>
            前端渲染时抛出异常，请把下面内容截图发给开发者：
          </p>
          <pre
            style={{
              background: "#fff5f5",
              border: "1px solid #ffccc7",
              padding: 14,
              borderRadius: 8,
              fontSize: 12,
              maxHeight: 280,
              overflow: "auto",
              whiteSpace: "pre-wrap",
              wordBreak: "break-word",
            }}
          >
            {String(this.state.error?.stack ?? this.state.error)}
          </pre>
          {this.state.info?.componentStack && (
            <pre
              style={{
                background: "#fafafa",
                border: "1px solid #e0e0e0",
                padding: 14,
                borderRadius: 8,
                fontSize: 12,
                maxHeight: 220,
                overflow: "auto",
                whiteSpace: "pre-wrap",
                marginTop: 12,
              }}
            >
              {this.state.info.componentStack}
            </pre>
          )}
          <p style={{ color: "#888", fontSize: 12, marginTop: 14 }}>
            提示：按 <b>F12</b> 打开开发者工具能看到更多细节。
          </p>
        </div>
      );
    }
    return this.props.children;
  }
}
