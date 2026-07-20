//! Extracts the static app catalog (name, description, color, icon, params)
//! straight from `faderpunk/src/apps/*.rs` and emits
//! `configurator/src/demo/catalog.ts`, the app list used by the
//! configurator's simulator mode. This keeps the simulator catalog from
//! ever drifting out of sync with the actual registered apps.
//!
//! This only parses Rust *syntax* (via `syn`) — it does not compile the
//! hardware-dependent `faderpunk` crate, which wouldn't be possible on a
//! host target anyway. Every app's `CONFIG` static is a fully literal
//! `const fn` builder chain (`Config::new(...).add_param(...)...`), so
//! syntactic extraction is sufficient.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use syn::punctuated::Punctuated;
use syn::{
    Expr, ExprArray, ExprCall, ExprLit, ExprMethodCall, ExprPath, ExprReference, ExprStruct, Ident,
    Item, Lit, LitInt, Member, Token,
};

struct AppEntry {
    id: LitInt,
    module: Ident,
}

impl syn::parse::Parse for AppEntry {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let id: LitInt = input.parse()?;
        input.parse::<Token![=>]>()?;
        let module: Ident = input.parse()?;
        Ok(AppEntry { id, module })
    }
}

/// A minimal JSON-like tree used to render TS object/array literals.
#[derive(Clone)]
enum TsValue {
    Str(String),
    /// Pre-formatted numeric literal source, e.g. "42" or "-1.5".
    Num(String),
    Object(Vec<(String, TsValue)>),
    Array(Vec<TsValue>),
}

impl TsValue {
    fn tag(name: &str) -> TsValue {
        TsValue::Object(vec![("tag".into(), TsValue::Str(name.into()))])
    }

    fn tag_with_value(name: &str, fields: Vec<(String, TsValue)>) -> TsValue {
        TsValue::Object(vec![
            ("tag".into(), TsValue::Str(name.into())),
            ("value".into(), TsValue::Object(fields)),
        ])
    }

    fn render(&self, out: &mut String, indent: usize) {
        match self {
            TsValue::Str(s) => {
                write!(out, "{:?}", s).unwrap();
            }
            TsValue::Num(n) => {
                out.push_str(n);
            }
            TsValue::Object(fields) => {
                if fields.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push_str("{\n");
                for (key, value) in fields {
                    push_indent(out, indent + 1);
                    write!(out, "{}: ", key).unwrap();
                    value.render(out, indent + 1);
                    out.push_str(",\n");
                }
                push_indent(out, indent);
                out.push('}');
            }
            TsValue::Array(items) => {
                if items.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push_str("[\n");
                for item in items {
                    push_indent(out, indent + 1);
                    item.render(out, indent + 1);
                    out.push_str(",\n");
                }
                push_indent(out, indent);
                out.push(']');
            }
        }
    }
}

fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("  ");
    }
}

fn path_last_segment(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Path(ExprPath { path, .. }) => path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

fn unwrap_reference(expr: &Expr) -> &Expr {
    match expr {
        Expr::Reference(ExprReference { expr, .. }) => expr,
        _ => expr,
    }
}

/// Converts a field-value expression from a `Param::Variant { field: expr, ... }`
/// struct literal into a `TsValue`, inferring the right shape from syntax:
/// string/number literals stay as-is, `&[EnumType::Variant, ...]` arrays of
/// enum paths become arrays of `{ tag: "Variant" }`, and `&["str", ...]`
/// arrays of string literals (only used by `Param::Enum.variants`) stay as
/// plain string arrays.
fn convert_field_value(expr: &Expr) -> TsValue {
    match expr {
        Expr::Lit(ExprLit { lit, .. }) => convert_lit(lit),
        Expr::Path(_) => {
            let name = path_last_segment(expr).expect("path expr must have a segment");
            TsValue::tag(&name)
        }
        Expr::Array(ExprArray { elems, .. }) => convert_array(elems),
        Expr::Reference(_) => convert_field_value(unwrap_reference(expr)),
        other => panic!("unsupported field-value expression: {:?}", other),
    }
}

fn convert_array(elems: &Punctuated<Expr, Token![,]>) -> TsValue {
    TsValue::Array(elems.iter().map(convert_array_elem).collect())
}

fn convert_array_elem(expr: &Expr) -> TsValue {
    match expr {
        Expr::Lit(ExprLit { lit, .. }) => convert_lit(lit),
        Expr::Path(_) => {
            let name = path_last_segment(expr).expect("path expr must have a segment");
            TsValue::tag(&name)
        }
        other => panic!("unsupported array element expression: {:?}", other),
    }
}

fn convert_lit(lit: &Lit) -> TsValue {
    match lit {
        Lit::Str(s) => TsValue::Str(s.value()),
        Lit::Int(i) => TsValue::Num(i.base10_digits().to_string()),
        Lit::Float(f) => TsValue::Num(f.base10_digits().to_string()),
        Lit::Bool(b) => TsValue::Num(b.value.to_string()),
        other => panic!("unsupported literal: {:?}", other),
    }
}

/// Converts a `Param::Variant { .. }` or `Param::Variant` expression (one
/// `.add_param(...)` argument) into a `TsValue` matching the postcard-bindgen
/// TS shape for `Param`.
fn convert_param(expr: &Expr) -> TsValue {
    match expr {
        Expr::Path(_) => {
            let name = path_last_segment(expr).expect("Param path must have a segment");
            TsValue::tag(&name)
        }
        Expr::Struct(ExprStruct { path, fields, .. }) => {
            let name = path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .expect("Param struct path must have a segment");
            let mut rendered_fields = Vec::new();
            for field in fields {
                let key = match &field.member {
                    Member::Named(ident) => ident.to_string(),
                    Member::Unnamed(idx) => idx.index.to_string(),
                };
                rendered_fields.push((key, convert_field_value(&field.expr)));
            }
            TsValue::tag_with_value(&name, rendered_fields)
        }
        other => panic!("unsupported Param expression: {:?}", other),
    }
}

struct ExtractedConfig {
    name: String,
    description: String,
    color: String,
    icon: String,
    params: Vec<TsValue>,
}

/// Walks a `Config::new(name, description, color, icon).add_param(..).add_param(..)`
/// method-call chain and extracts the four base args plus each param.
fn extract_config(expr: &Expr) -> ExtractedConfig {
    fn walk(expr: &Expr, params: &mut Vec<Expr>) -> (Expr, Expr, Expr, Expr) {
        match expr {
            Expr::MethodCall(ExprMethodCall {
                receiver,
                method,
                args,
                ..
            }) => {
                assert_eq!(
                    method, "add_param",
                    "unexpected method call in CONFIG chain: {method}"
                );
                let arg = args.first().expect("add_param takes one argument").clone();
                let base = walk(receiver, params);
                params.push(arg);
                base
            }
            Expr::Call(ExprCall { func, args, .. }) => {
                let func_name = path_last_segment(func).unwrap_or_default();
                assert_eq!(
                    func_name, "new",
                    "expected Config::new(..) at the root of CONFIG chain"
                );
                let mut it = args.iter().cloned();
                let name = it.next().expect("Config::new missing name arg");
                let description = it.next().expect("Config::new missing description arg");
                let color = it.next().expect("Config::new missing color arg");
                let icon = it.next().expect("Config::new missing icon arg");
                (name, description, color, icon)
            }
            other => panic!("unexpected expression in CONFIG chain: {:?}", other),
        }
    }

    let mut param_exprs = Vec::new();
    let (name_expr, description_expr, color_expr, icon_expr) = walk(expr, &mut param_exprs);

    let name = match convert_lit_expr(&name_expr) {
        TsValue::Str(s) => s,
        _ => panic!("Config::new name arg must be a string literal"),
    };
    let description = match convert_lit_expr(&description_expr) {
        TsValue::Str(s) => s,
        _ => panic!("Config::new description arg must be a string literal"),
    };
    let color = path_last_segment(&color_expr).expect("Config::new color arg must be a path");
    let icon = path_last_segment(&icon_expr).expect("Config::new icon arg must be a path");

    let params = param_exprs.iter().map(convert_param).collect();

    ExtractedConfig {
        name,
        description,
        color,
        icon,
        params,
    }
}

fn convert_lit_expr(expr: &Expr) -> TsValue {
    match expr {
        Expr::Lit(ExprLit { lit, .. }) => convert_lit(lit),
        other => panic!("expected a literal expression, got: {:?}", other),
    }
}

fn find_registered_apps(mod_rs: &Path) -> Vec<(u8, String)> {
    let content =
        fs::read_to_string(mod_rs).unwrap_or_else(|e| panic!("reading {}: {e}", mod_rs.display()));
    let file =
        syn::parse_file(&content).unwrap_or_else(|e| panic!("parsing {}: {e}", mod_rs.display()));

    for item in &file.items {
        if let Item::Macro(item_macro) = item {
            if item_macro.mac.path.is_ident("register_apps") {
                let entries = item_macro
                    .mac
                    .parse_body_with(Punctuated::<AppEntry, Token![,]>::parse_terminated)
                    .unwrap_or_else(|e| panic!("parsing register_apps! body: {e}"));
                return entries
                    .into_iter()
                    .map(|e| {
                        let id: u8 = e.id.base10_parse().expect("app id must fit in u8");
                        (id, e.module.to_string())
                    })
                    .collect();
            }
        }
    }
    panic!("no register_apps! invocation found in {}", mod_rs.display());
}

fn find_channels_const(file: &syn::File) -> Option<i64> {
    for item in &file.items {
        if let Item::Const(item_const) = item {
            if item_const.ident == "CHANNELS" {
                if let Expr::Lit(ExprLit {
                    lit: Lit::Int(i), ..
                }) = item_const.expr.as_ref()
                {
                    return Some(i.base10_parse().expect("CHANNELS must be an integer"));
                }
            }
        }
    }
    None
}

fn find_config_static(file: &syn::File) -> Option<Expr> {
    for item in &file.items {
        if let Item::Static(item_static) = item {
            if item_static.ident == "CONFIG" {
                return Some(item_static.expr.as_ref().clone());
            }
        }
    }
    None
}

struct AppData {
    id: u8,
    channels: i64,
    config: ExtractedConfig,
}

fn render_app(app: &AppData) -> TsValue {
    TsValue::Object(vec![
        ("appId".into(), TsValue::Num(app.id.to_string())),
        (
            "channels".into(),
            TsValue::Num(format!("BigInt({})", app.channels)),
        ),
        ("color".into(), TsValue::Str(app.config.color.clone())),
        ("icon".into(), TsValue::Str(app.config.icon.clone())),
        ("name".into(), TsValue::Str(app.config.name.clone())),
        (
            "description".into(),
            TsValue::Str(app.config.description.clone()),
        ),
        (
            "paramCount".into(),
            TsValue::Num(format!("BigInt({})", app.config.params.len())),
        ),
        ("params".into(), TsValue::Array(app.config.params.clone())),
    ])
}

/// `generate` reads `faderpunk/src/apps/mod.rs` + each registered app's
/// source file under `faderpunk_src`, and writes the generated catalog to
/// `out_file`.
pub fn generate(faderpunk_src: &Path, out_file: &Path) {
    let apps_dir = faderpunk_src.join("apps");
    let registered = find_registered_apps(&apps_dir.join("mod.rs"));

    let mut apps = Vec::new();
    for (id, module) in registered {
        let file_path = apps_dir.join(format!("{module}.rs"));
        let content = fs::read_to_string(&file_path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", file_path.display()));
        let parsed = syn::parse_file(&content)
            .unwrap_or_else(|e| panic!("parsing {}: {e}", file_path.display()));

        let channels = find_channels_const(&parsed)
            .unwrap_or_else(|| panic!("no `pub const CHANNELS` found in {}", file_path.display()));
        let config_expr = find_config_static(&parsed)
            .unwrap_or_else(|| panic!("no `pub static CONFIG` found in {}", file_path.display()));
        let config = extract_config(&config_expr);

        apps.push(AppData {
            id,
            channels,
            config,
        });
    }
    apps.sort_by_key(|a| a.id);

    let mut out = String::new();
    out.push_str(
        "// AUTO-GENERATED by gen-bindings from faderpunk/src/apps/*.rs — do not edit by hand.\n",
    );
    out.push_str("// Regenerate with `./gen-bindings.sh` from the repo root (this also runs in\n");
    out.push_str("// CI, beta, and release builds, so this file never needs to be committed).\n");
    out.push_str("import type { AllApps, App } from \"../utils/types\";\n\n");
    out.push_str("const APPS: App[] = [\n");
    for app in &apps {
        push_indent(&mut out, 1);
        render_app(app).render(&mut out, 1);
        out.push_str(",\n");
    }
    out.push_str("];\n\n");
    out.push_str("export const DEMO_APPS: AllApps = new Map(APPS.map((a) => [a.appId, a]));\n");

    if let Some(parent) = out_file.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(out_file, out).unwrap_or_else(|e| panic!("writing {}: {e}", out_file.display()));
}
