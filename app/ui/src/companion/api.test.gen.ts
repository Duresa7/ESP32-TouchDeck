// Generated from td-companion's test hooks. Do not edit.
export type ApprovalScript = { answers: Array<ScriptAnswer>, };
export type BoardKind = "device" | "device-v1" | "silent" | "other-firmware" | "busy";
export type ClockAdvance = { advance_ms: number, };
export type ConsoleLine = { line: string, };
export type Delivery = { url: string | null, notification: string | null, };
export type DeviceId = string;
export type DevicePath = { device: DeviceId, };
export type JsonValue = number | string | boolean | Array<JsonValue> | { [key in string]?: JsonValue } | null;
export type MockAction = { 
/**
 * `no-confirm`, `rollback`, `normal-update`, `stop` or `hang-up`.
 */
action: string, };
export type MockEmit = { events: Array<JsonValue>, repeat: number | null, every_ms: number | null, };
export type MockPortChange = { "plug": PlugBoard } | { "unplug": DeviceId };
export type MockPress = { page: string, tile: string, kind: string, };
export type MockRecord = { received: Array<JsonValue>, lines: Array<string>, settings: JsonValue, reboots: number, assets: Array<string>, layouts: JsonValue, };
export type NoQuery = null;
export type PlugBoard = { id: DeviceId, kind: BoardKind, };
export type ScriptAnswer = "once" | "always" | "deny" | "fail" | "never";
export const testEndpoints = {
  'test.approvals_script': { method: 'POST', path: '/test/approvals/script', access: 'session' },
  'test.clock': { method: 'POST', path: '/test/clock', access: 'session' },
  'test.console': { method: 'POST', path: '/test/devices/{device}/console', access: 'session' },
  'test.emit': { method: 'POST', path: '/test/devices/{device}/emit', access: 'session' },
  'test.input_log': { method: 'GET', path: '/test/input-log', access: 'session' },
  'test.mock': { method: 'POST', path: '/test/devices/{device}/mock', access: 'session' },
  'test.pairing_delivery': { method: 'GET', path: '/test/pairing-delivery', access: 'session' },
  'test.ports': { method: 'POST', path: '/test/ports', access: 'session' },
  'test.press': { method: 'POST', path: '/test/devices/{device}/press', access: 'session' },
  'test.record': { method: 'GET', path: '/test/devices/{device}/record', access: 'session' },
  'test.sets': { method: 'GET', path: '/test/devices/{device}/sets', access: 'session' },
} as const;
