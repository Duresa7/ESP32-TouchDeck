import { useEffect, useState } from 'react';
import type { SystemInfo } from '../../companion/api.ts';
import { useCompanion } from '../../companion/connection.ts';

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
