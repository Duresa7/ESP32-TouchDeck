use ts_rs::TS;
pub const AUTH_ENDPOINTS: &[(&str, &str, &str)] = &[
    ("system.ping", "GET", "/system/ping"),
    ("auth.proof", "POST", "/auth/proof"),
    ("auth.session_create", "POST", "/auth/sessions"),
    ("auth.sessions", "GET", "/auth/sessions"),
    ("auth.session_delete", "DELETE", "/auth/sessions/{id}"),
    ("auth.reset", "POST", "/auth/reset"),
];
pub const EVENTS: &[&str] = &[
    "hello",
    "sync",
    "device.added",
    "device.updated",
    "device.removed",
    "auth.revoked",
    "companion.stopping",
];
pub const ERRORS: &[&str] = &[
    "security.host",
    "security.origin",
    "auth.missing",
    "auth.invalid",
    "auth.revoked",
    "auth.forbidden",
    "request.invalid",
    "request.too_large",
    "request.media_type",
    "limits.exceeded",
    "route.not_found",
    "route.method_not_allowed",
    "device.not_found",
    "companion.stopping",
];
pub fn generate() -> String {
    let mut out = String::from("// Generated from td-companion. Do not edit.\n");
    for decl in [
        crate::devices::DeviceSummary::decl(),
        crate::devices::FirmwareInfo::decl(),
        crate::devices::LinkState::decl(),
        crate::devices::KeyMirrorState::decl(),
        crate::features::devices::DevicePath::decl(),
        crate::features::system::SystemInfo::decl(),
        crate::api::ErrorBody::decl(),
        crate::api::SessionInfo::decl(),
        crate::api::ClientInfo::decl(),
    ] {
        out.push_str("export ");
        out.push_str(&decl);
        out.push('\n');
    }
    out.push_str("export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };\n");
    for decl in crate::api::auth_declarations() {
        out.push_str("export ");
        out.push_str(&decl);
        out.push('\n');
    }
    out.push_str("export const endpoints = {\n");
    for feature in crate::api::registry() {
        for e in feature.endpoints {
            out.push_str(&format!(
                "  '{}': {{ method: '{}', path: '{}' }},\n",
                e.name, e.method, e.path
            ));
        }
    }
    for (name, method, path) in AUTH_ENDPOINTS {
        out.push_str(&format!(
            "  '{name}': {{ method: '{method}', path: '{path}' }},\n"
        ));
    }
    out.push_str("} as const;\nexport type ErrorCode = ");
    out.push_str(
        &ERRORS
            .iter()
            .map(|s| format!("'{s}'"))
            .collect::<Vec<_>>()
            .join(" | "),
    );
    out.push_str(";\nexport type SyncData = { devices: DeviceSummary[] };\nexport type JobMap = Record<string, never>;\nexport type EventMap = {\n");
    for event in EVENTS {
        let typ = match *event {
            "sync" => "SyncData",
            "device.added" | "device.updated" => "DeviceSummary",
            "device.removed" => "{ id: string }",
            _ => "Record<string, unknown>",
        };
        out.push_str(&format!("  '{event}': {typ};\n"));
    }
    out.push_str("};\n");
    out.push_str(r#"
export function createCompanion(base: string, token: () => string) {
  async function call<T>(name: keyof typeof endpoints, path: Record<string, string>, query: unknown, body: unknown): Promise<T> {
    const endpoint = endpoints[name];
    let route: string = endpoint.path;
    for (const [key, value] of Object.entries(path)) route = route.replace(`{${key}}`, encodeURIComponent(value));
    const url = new URL(`/api/v1${route}`, base);
    if (query) for (const [key, value] of Object.entries(query)) url.searchParams.set(key, String(value));
    const response = await fetch(url, { method: endpoint.method, headers: { ...(name === 'auth.proof' || name === 'system.ping' ? {} : { Authorization: `Bearer ${token()}` }), ...(body == null ? {} : { 'Content-Type': 'application/json' }) }, ...(body == null ? {} : { body: JSON.stringify(body) }) });
    if (!response.ok) throw (await response.json()).error;
    return response.status === 204 ? undefined as T : response.json();
  }
  return {
"#);
    for feature in crate::api::registry() {
        out.push_str(&format!("    {}: {{\n", feature.name));
        for e in feature.endpoints {
            let action = e.name.split('.').nth(1).unwrap();
            let path = if e.path_type == "null" {
                "Record<string, never> = {}"
            } else {
                &e.path_type
            };
            out.push_str(&format!("      {action}: (path: {path}, query: {} = null as {}, body: {} = null as {}) => call<{}>('{}', path, query, body),\n",e.query_type,e.query_type,e.body_type,e.body_type,e.ok_type,e.name));
        }
        if feature.name == "system" {out.push_str("      ping: () => call<{ api: string[] }>('system.ping', {}, null, null),\n");}
        out.push_str("    },\n");
    }
    out.push_str(r#"    auth: {
      proof: (body: ProofRequest) => call<Proof>('auth.proof', {}, null, body),
      session_create: (body: SessionRequest) => call<SessionCreated>('auth.session_create', {}, null, body),
      sessions: () => call<SessionInfo[]>('auth.sessions', {}, null, null),
      session_delete: (id: string) => call<void>('auth.session_delete', { id }, null, null),
      reset: () => call<void>('auth.reset', {}, null, null),
    },
  };
}
"#);
    out.replace("bigint", "number")
}
