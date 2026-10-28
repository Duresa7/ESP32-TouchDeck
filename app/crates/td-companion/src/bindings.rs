//! Renders `api.gen.ts` from the registry. Nothing here is listed by hand: every route,
//! event, snapshot and error code comes from a feature's declaration.
use crate::api::{Access, ApiError, CommonError, EventInfo, Feature};
use std::{
    any::TypeId,
    collections::{BTreeMap, BTreeSet, HashSet},
};
use ts_rs::{TS, TypeVisitor};

/// TypeScript declarations keyed by type name, gathered from types and their dependencies.
#[derive(Default)]
pub struct Decls {
    decls: BTreeMap<String, String>,
    seen: HashSet<TypeId>,
}
impl Decls {
    pub fn add<T: TS + 'static + ?Sized>(&mut self) {
        struct Visit<'a>(&'a mut Decls);
        impl TypeVisitor for Visit<'_> {
            fn visit<T: TS + 'static + ?Sized>(&mut self) {
                self.0.add::<T>();
            }
        }
        if !self.seen.insert(TypeId::of::<T>()) {
            return;
        }
        if T::output_path().is_some() {
            self.decls.insert(T::ident(), T::decl());
        }
        T::visit_dependencies(&mut Visit(self));
        T::visit_generics(&mut Visit(self));
    }
}

/// Every error code the Companion can return: the kernel's and each route's.
pub fn error_codes(features: &[Feature]) -> BTreeSet<&'static str> {
    let mut codes: BTreeSet<_> = CommonError::CODES.iter().copied().collect();
    for e in features.iter().flat_map(|f| &f.endpoints) {
        codes.extend(e.errors.iter().copied());
    }
    codes
}
/// Every frame type: the kernel's and each feature's.
pub fn events(features: &[Feature]) -> Vec<EventInfo> {
    let mut events = crate::api::kernel_events();
    for feature in features {
        events.extend(feature.events.iter().map(|e| EventInfo {
            name: e.name,
            data_type: e.data_type.clone(),
            types: e.types,
        }));
    }
    events
}

pub fn generate() -> String {
    let features = crate::api::registry();
    let events = events(&features);
    let mut decls = Decls::default();
    for feature in &features {
        for e in &feature.endpoints {
            (e.types)(&mut decls);
        }
        for s in &feature.snapshots {
            (s.types)(&mut decls);
        }
    }
    for e in &events {
        if e.name != "sync" {
            (e.types)(&mut decls);
        }
    }
    let mut out = String::from("// Generated from td-companion. Do not edit.\n");
    for decl in decls.decls.values() {
        out.push_str(&format!("export {decl}\n"));
    }

    out.push_str("export const endpoints = {\n");
    for e in features.iter().flat_map(|f| &f.endpoints) {
        let access = match e.access {
            Access::Session => "session",
            Access::Pairing => "pairing",
            Access::Public => "public",
        };
        out.push_str(&format!(
            "  '{}': {{ method: '{}', path: '{}', access: '{access}' }},\n",
            e.name, e.method, e.path
        ));
    }
    out.push_str("} as const;\n");

    let codes: Vec<_> = error_codes(&features)
        .into_iter()
        .map(|c| format!("'{c}'"))
        .collect();
    out.push_str(&format!("export type ErrorCode = {};\n", codes.join(" | ")));

    out.push_str("export type SyncData = {\n");
    for s in features.iter().flat_map(|f| &f.snapshots) {
        out.push_str(&format!("  {}: {},\n", s.key, s.data_type));
    }
    out.push_str("};\nexport type JobMap = Record<string, never>;\n");

    out.push_str("export type EventMap = {\n");
    for e in &events {
        let data = if e.name == "sync" {
            "SyncData"
        } else {
            &e.data_type
        };
        out.push_str(&format!("  '{}': {data};\n", e.name));
    }
    out.push_str("};\n");

    out.push_str(
        r#"
export function createCompanion(base: string, token: () => string) {
  async function call<T>(name: keyof typeof endpoints, path: Record<string, string>, query: unknown, body: unknown, bearer?: string): Promise<T> {
    const endpoint = endpoints[name];
    let route: string = endpoint.path;
    for (const [key, value] of Object.entries(path)) route = route.replace(`{${key}}`, encodeURIComponent(value));
    const url = new URL(`/api/v1${route}`, base);
    if (query) for (const [key, value] of Object.entries(query)) url.searchParams.set(key, String(value));
    const auth = bearer ?? (endpoint.access === 'session' ? token() : undefined);
    const response = await fetch(url, { method: endpoint.method, headers: { ...(auth ? { Authorization: `Bearer ${auth}` } : {}), ...(body == null ? {} : { 'Content-Type': 'application/json' }) }, ...(body == null ? {} : { body: JSON.stringify(body) }) });
    if (!response.ok) throw (await response.json()).error;
    return response.status === 204 ? undefined as T : response.json();
  }
  return {
"#,
    );
    for feature in &features {
        out.push_str(&format!("    {}: {{\n", feature.name));
        for e in &feature.endpoints {
            let action = e.name.split_once('.').unwrap().1;
            // A part the route doesn't take defaults to empty; the others are required.
            let param = |name: &str, typ: &str, empty: &str| {
                if typ == "null" {
                    format!("{name}: {empty}")
                } else {
                    format!("{name}: {typ}")
                }
            };
            out.push_str(&format!(
                "      {action}: ({}, {}, {}, bearer?: string) => call<{}>('{}', path, query, body, bearer),\n",
                param("path", &e.path_type, "Record<string, never> = {}"),
                param("query", &e.query_type, "null = null"),
                param("body", &e.body_type, "null = null"),
                e.ok_type,
                e.name
            ));
        }
        out.push_str("    },\n");
    }
    out.push_str("  };\n}\n");
    out.replace("bigint", "number")
}
