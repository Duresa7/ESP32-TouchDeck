type SenderContext = {
  sender: unknown;
  frame: unknown;
  origin: string | undefined;
  window: { webContents: { mainFrame: unknown } } | undefined;
  closing: boolean;
  expectedOrigin: string | undefined;
};

export function trustedSender({ sender, frame, origin, window, closing, expectedOrigin }: SenderContext): boolean {
  return !!window && !!expectedOrigin && !closing && sender === window.webContents &&
    frame === window.webContents.mainFrame && origin === expectedOrigin;
}
