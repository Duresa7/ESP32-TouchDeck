/** An app update Electron's updater has found, as the Update icon shows it. */
export interface UpdateOffer {
  phase: 'available' | 'downloading' | 'ready';
  /** 0 to 1 while downloading */
  progress: number;
}

/**
 * The update the Update icon shows, or null. Updates live in Electron's preload
 * (ADR 0005), and #87 reads them from there. Until then there's never one.
 */
export function useUpdate(): UpdateOffer | null {
  return null;
}

/**
 * The Update icon's own puck, 8px right of the nav cluster, so nothing in it moves
 * the nav buttons. It isn't rendered when no update exists. #87 draws the icon in it.
 */
export function UpdateSlot() {
  const update = useUpdate();
  if (!update) return null;
  return <div className="update-slot" data-phase={update.phase} />;
}
