import { Component, type ReactNode, type ErrorInfo } from 'react';

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

/**
 * Global error boundary — catches rendering errors and shows a recovery UI
 * instead of crashing the entire app with a white screen.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false, error: null };

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('[ErrorBoundary]', error, info.componentStack);
  }

  handleRetry = () => {
    this.setState({ hasError: false, error: null });
  };

  render() {
    if (this.state.hasError) {
      return (
        <div style={{
          display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
          minHeight: '100vh', padding: 32, background: 'var(--ob-bg-primary, #0f1117)',
          color: 'var(--ob-text-primary, #e5e7eb)', fontFamily: "'Inter', sans-serif",
        }}>
          <div style={{ fontSize: 48, marginBottom: 16 }}>⚠️</div>
          <h1 style={{ fontSize: '1.5rem', marginBottom: 8 }}>Something went wrong</h1>
          <p style={{ color: 'var(--ob-text-secondary, #9ca3af)', marginBottom: 24, maxWidth: 500, textAlign: 'center' }}>
            An unexpected error occurred. You can try again or refresh the page.
          </p>
          {this.state.error && (
            <pre style={{
              padding: '12px 16px', borderRadius: 8, marginBottom: 24,
              background: 'rgba(239, 68, 68, 0.1)', border: '1px solid rgba(239, 68, 68, 0.3)',
              color: '#ef4444', fontSize: '0.85rem', maxWidth: 600, overflow: 'auto',
            }}>
              {this.state.error.message}
            </pre>
          )}
          <div style={{ display: 'flex', gap: 12 }}>
            <button
              onClick={this.handleRetry}
              className="btn-primary"
              style={{ padding: '10px 24px', borderRadius: 8, fontSize: '0.9rem' }}
            >
              Try Again
            </button>
            <button
              onClick={() => window.location.reload()}
              style={{
                padding: '10px 24px', borderRadius: 8, fontSize: '0.9rem',
                background: 'transparent', border: '1px solid var(--ob-glass-border, #333)',
                color: 'var(--ob-text-secondary)', cursor: 'pointer',
              }}
            >
              Reload Page
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
