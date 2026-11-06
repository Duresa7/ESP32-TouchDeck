import { useEffect, useState } from 'react';
import type { SystemInfo } from '../../lib/companion/api.ts';
import { useCompanion } from '../../lib/companion/connection.ts';

export function useSystemInfo(): SystemInfo | 'unknown' | null {
  const companion = useCompanion();
  const [info, setInfo] = useState<SystemInfo | 'unknown' | null>(null);
  useEffect(() => {
    let live = true;
    companion.system.get().then(
      (system) => live && setInfo(system),
      () => live && setInfo('unknown'),
    );
    return () => {
      live = false;
    };
  }, [companion]);
  return info;
}
