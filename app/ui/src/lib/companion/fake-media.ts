import type { IntegrationView } from './api.gen.ts';

/** The Volume Integration as docs/companion-api.md describes it: the PC's level, settable, and whether it is muted. */
export function fakeVolume(enabled = true): IntegrationView {
  return {
    config: { enabled }, secrets: {}, detail: null,
    summary: {
      revision: 0, id: 'volume', name: 'Volume', icon: 'volume', enabled, status: { state: 'ok', message: null, delivery: [] },
      config_schema: [{ name: 'enabled', kind: 'boolean', secret: false }],
      streams: [
        { id: 'level', kind: 'numeric', unit: null, settable: true },
        { id: 'muted', kind: 'numeric', unit: null, settable: false },
      ],
      actions: ['mute'],
      presets: [
        {
          id: 'volume_bar', name: 'Volume bar',
          action: { type: 'widget', source: 'volume', account: '', field: 'level' },
          render: 'bar', streams: { value: 'number', muted: 'number' }, input: true,
        },
      ],
    },
  };
}

/** The Media Integration as docs/companion-api.md describes it: the current track, its cover and whether it plays. */
export function fakeMedia(enabled = true): IntegrationView {
  return {
    config: { enabled }, secrets: {}, detail: null,
    summary: {
      revision: 0, id: 'media', name: 'Media', icon: 'music', enabled, status: { state: 'ok', message: null, delivery: [] },
      config_schema: [{ name: 'enabled', kind: 'boolean', secret: false }],
      streams: [
        { id: 'title', kind: 'text', unit: null, settable: false },
        { id: 'artist', kind: 'text', unit: null, settable: false },
        { id: 'cover', kind: 'image', unit: null, settable: false },
        { id: 'state', kind: 'numeric', unit: null, settable: false },
      ],
      actions: ['previous', 'play_pause', 'next'],
      presets: [
        {
          id: 'now_playing', name: 'Now playing',
          action: { type: 'widget', source: 'media', account: '', field: 'title' },
          render: 'now-playing', streams: { title: 'text', artist: 'text', cover: 'image', state: 'number' },
        },
      ],
    },
  };
}
