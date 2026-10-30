import type { IntegrationView } from './api.gen.ts';

const streams: { id: string; name: string; unit: string }[] = [
  { id: 'cpu_pct', name: 'CPU usage', unit: '%' },
  { id: 'ram_pct', name: 'RAM usage', unit: '%' },
  { id: 'ram_used_bytes', name: 'RAM used', unit: 'bytes' },
  { id: 'ram_total_bytes', name: 'RAM total', unit: 'bytes' },
  { id: 'gpu_pct', name: 'GPU usage', unit: '%' },
  { id: 'disk_active_pct', name: 'Disk activity', unit: '%' },
  { id: 'disk_read_bytes_s', name: 'Disk read', unit: 'bytes/s' },
  { id: 'disk_write_bytes_s', name: 'Disk write', unit: 'bytes/s' },
  { id: 'network_receive_bytes_s', name: 'Network receive', unit: 'bytes/s' },
  { id: 'network_send_bytes_s', name: 'Network send', unit: 'bytes/s' },
];

export function fakeThisPc(): IntegrationView {
  return {
    config: { enabled: true }, secrets: {},
    summary: {
      id: 'this_pc', name: 'This PC', icon: 'monitor', enabled: true,
      status: { state: 'ok', message: null, delivery: [] },
      config_schema: [{ name: 'enabled', kind: 'boolean', secret: false }],
      streams: streams.map(({ id, unit }) => ({ id, kind: 'numeric', unit })),
      actions: ['refresh'],
      presets: streams.map(({ id, name }) => ({ id, name, action: { type: 'widget', source: 'this_pc', account: '', field: id } })),
    },
  };
}
