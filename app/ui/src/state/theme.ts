/** The Appearance setting. System follows Windows through prefers-color-scheme. */
export type Appearance = 'system' | 'light' | 'dark';

// tokens.css reads data-theme on <html>: no attribute follows the system, and
// "light" or "dark" forces that theme.
export function themeAttribute(appearance: Appearance): 'light' | 'dark' | undefined {
  return appearance === 'system' ? undefined : appearance;
}

export function applyTheme(root: { dataset: DOMStringMap }, appearance: Appearance): void {
  const theme = themeAttribute(appearance);
  if (theme) root.dataset.theme = theme;
  else delete root.dataset.theme;
}

