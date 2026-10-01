/**
 * Error boundaries.
 *
 * A class component is the documented React exception to the
 * function-component preference: boundaries cannot be written as functions
 * (spec 7.8.3). A render failure is contained, and recovering never clears
 * accounts or restarts polling (AC-116).
 *
 * Every fallback is written out twice because the two boundaries differ in
 * heading level and in wording, not because they share behaviour.
 */
import { Component, type ErrorInfo, type ReactNode } from "react";

/** What a boundary reports about a caught render failure. */
export interface BoundaryFailure {
  readonly message: string;
  readonly componentStack: string | null;
}

/** The state a boundary keeps. */
interface BoundaryState {
  readonly failure: BoundaryFailure | null;
}

/** How a boundary is told what it was rendering. */
export interface BoundaryProps {
  /** The surface name shown in the fallback, such as `overview`. */
  readonly surface: string;
  /** Extra work after a failure, such as reporting it. It must not throw. */
  readonly onFailure?: (failure: BoundaryFailure) => void;
  readonly children: ReactNode;
}

/** A recoverable boundary around one feature surface. */
export class FeatureBoundary extends Component<BoundaryProps, BoundaryState> {
  override state: BoundaryState = { failure: null };

  static getDerivedStateFromError(error: unknown): BoundaryState {
    return {
      failure: {
        message: error instanceof Error ? error.message : "render failed",
        componentStack: null,
      },
    };
  }

  override componentDidCatch(_error: unknown, info: ErrorInfo): void {
    const { failure } = this.state;
    const next: BoundaryFailure = {
      message: failure?.message ?? "render failed",
      componentStack: info.componentStack ?? null,
    };
    this.setState({ failure: next });
    this.props.onFailure?.(next);
  }

  override render(): ReactNode {
    const { failure } = this.state;
    if (failure === null) {
      return this.props.children;
    }
    return (
      <div className="fallback" role="alert">
        <h2>The {this.props.surface} view stopped rendering.</h2>
        <p>
          Accounts and monitoring continue in the backend. Reloading this view does not
          clear accounts and does not restart polling.
        </p>
        <button
          type="button"
          className="button button--primary"
          onClick={() => {
            this.setState({ failure: null });
          }}
        >
          Reload this view
        </button>
      </div>
    );
  }
}

/** The application-level boundary, which replaces the whole surface. */
export class AppBoundary extends Component<
  { readonly children: ReactNode },
  BoundaryState
> {
  override state: BoundaryState = { failure: null };

  static getDerivedStateFromError(error: unknown): BoundaryState {
    return {
      failure: {
        message: error instanceof Error ? error.message : "render failed",
        componentStack: null,
      },
    };
  }

  override render(): ReactNode {
    const { failure } = this.state;
    if (failure === null) {
      return this.props.children;
    }
    return (
      <div className="fallback fallback--app" role="alert">
        <h1>Quota stopped rendering.</h1>
        <p>
          The backend keeps monitoring and storing readings. Reloading the window does not
          clear accounts and does not restart polling.
        </p>
        <button
          type="button"
          className="button button--primary"
          onClick={() => {
            this.setState({ failure: null });
          }}
        >
          Reload the window
        </button>
      </div>
    );
  }
}
