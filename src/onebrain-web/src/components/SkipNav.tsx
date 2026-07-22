/**
 * SkipNav — Accessibility skip navigation link.
 * Visually hidden until focused (Tab key), then jumps to main content.
 */
export function SkipNav() {
  return (
    <a href="#main-content" className="skip-nav">
      Skip to main content
    </a>
  );
}
