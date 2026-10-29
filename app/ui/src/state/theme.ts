export type Appearance = 'system' | 'light' | 'dark';

export function themeAttribute(appearance: Appearance): 'light' | 'dark' | undefined {
  return appearance === 'system' ? undefined : appearance;
}

export function applyTheme(root: { dataset: DOMStringMap }, appearance: Appearance): void {
  const theme = themeAttribute(appearance);
  if (theme) root.dataset.theme = theme;
  else delete root.dataset.theme;
}

