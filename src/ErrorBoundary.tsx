// Catches render errors so one broken view shows a message instead of a blank window.

import { Component, type ErrorInfo, type ReactNode } from "react";
import { api } from "./api";
import { t } from "./i18n";

interface Props {
  children: ReactNode;
  /** Offered as a way out (e.g. back to the shelf); reloading is always offered. */
  onReset?: () => void;
  resetLabel?: string;
}

export default class ErrorBoundary extends Component<Props, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    api.logError(`${error.stack ?? error.message}\n${info.componentStack ?? ""}`).catch(() => {});
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div className="error-screen" role="alert">
        <h2>{t("error.title")}</h2>
        <p className="muted">{t("error.hint")}</p>
        <pre className="error-detail">{error.message}</pre>
        <div className="form-actions">
          {this.props.onReset && (
            <button
              onClick={() => {
                this.setState({ error: null });
                this.props.onReset?.();
              }}
            >
              {this.props.resetLabel}
            </button>
          )}
          <button className="primary" onClick={() => window.location.reload()}>
            {t("error.reload")}
          </button>
        </div>
      </div>
    );
  }
}
