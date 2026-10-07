#![feature(rustc_private)]
//! Compiler adapter for the pinned nightly. This exports analyzed HIR, not
//! pretty-printed macro text or rustdoc's different cfg(doc) inventory.
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

use rustc_driver::{Callbacks, Compilation};
use rustc_hir::{
    def::DefKind,
    def_id::{DefId, LOCAL_CRATE},
};
use rustc_interface::interface::Compiler;
use rustc_middle::ty::{self, TyCtxt};
use serde_json::{Value, json};
use std::{
    env, fs,
    path::PathBuf,
    process::{Command, ExitCode},
};

mod architecture;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> ExitCode {
    execute().unwrap_or_else(|error| {
        eprintln!("compiler inventory: {error}");
        ExitCode::FAILURE
    })
}

fn execute() -> Result<ExitCode> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args == ["--compiler-info"] {
        println!(
            "{}",
            json!({"schema_version": 1, "backend": "rustc-hir", "rustc": env!("RQLENS_DRIVER_RUSTC"), "compiler_version": env!("RQLENS_DRIVER_VERSION"), "host": env!("RQLENS_DRIVER_HOST")})
        );
        return Ok(ExitCode::SUCCESS);
    }
    validate_compiler(&args)?;
    let primary = env::var("CARGO_PRIMARY_PACKAGE").as_deref() == Ok("1");
    let Some(destination) = env::var_os("RQLENS_COMPILER_CAPTURE").filter(|_| primary) else {
        return Ok(delegate(&args));
    };
    let source = source_root(&args)?;
    let build_scripts: Vec<PathBuf> = serde_json::from_str(
        &env::var("RQLENS_COMPILER_BUILD_SCRIPTS").unwrap_or_else(|_| "[]".into()),
    )?;
    if build_scripts.contains(&source) {
        return Ok(delegate(&args));
    }
    let mut callback = Inventory {
        destination: PathBuf::from(destination),
        source,
        captured: false,
    };
    if !args
        .iter()
        .any(|arg| arg == "--sysroot" || arg.starts_with("--sysroot="))
    {
        args.extend(["--sysroot".into(), env!("RQLENS_DRIVER_SYSROOT").into()]);
    }
    let exit =
        rustc_driver::catch_with_exit_code(|| rustc_driver::run_compiler(&args, &mut callback));
    if exit == ExitCode::SUCCESS && !callback.captured {
        return Err("compiler completed without an analyzed inventory".into());
    }
    Ok(exit)
}

fn validate_compiler(args: &[String]) -> Result<()> {
    let compiler = args.first().ok_or("missing compiler argument")?;
    let expected = fs::canonicalize(env!("RQLENS_DRIVER_RUSTC"))?;
    if fs::canonicalize(compiler)? != expected {
        return Err(
            "compiler adapter invoked with an incompatible compiler or nested wrapper".into(),
        );
    }
    Ok(())
}

fn source_root(args: &[String]) -> Result<PathBuf> {
    let sources = args
        .iter()
        .skip(1)
        .filter(|arg| arg.ends_with(".rs") && PathBuf::from(arg).is_file())
        .collect::<Vec<_>>();
    if sources.len() != 1 {
        return Err("compiler inventory requires exactly one source root".into());
    }
    Ok(fs::canonicalize(sources[0])?)
}

// Called after main has established a nonempty compiler argv.
fn delegate(args: &[String]) -> ExitCode {
    Command::new(&args[0])
        .args(&args[1..])
        .status()
        .map_or(ExitCode::FAILURE, |status| {
            ExitCode::from(status.code().unwrap_or(1) as u8)
        })
}

struct Inventory {
    destination: PathBuf,
    source: PathBuf,
    captured: bool,
}

impl Callbacks for Inventory {
    fn after_analysis<'tcx>(&mut self, _compiler: &Compiler, tcx: TyCtxt<'tcx>) -> Compilation {
        match self.capture(tcx) {
            Ok(()) => self.captured = true,
            Err(error) => {
                self.captured = false;
                eprintln!("compiler capture failed: {error}");
                return Compilation::Stop;
            }
        }
        Compilation::Continue
    }
}

impl Inventory {
    fn capture(&self, tcx: TyCtxt<'_>) -> Result<()> {
        let mut types = Vec::new();
        let mut impls = Vec::new();
        let mut functions = Vec::new();
        for local in tcx.hir_crate_items(()).definitions() {
            let id = local.to_def_id();
            match tcx.def_kind(id) {
                DefKind::Struct | DefKind::Enum | DefKind::Union => {
                    types.push(type_record(tcx, id))
                }
                DefKind::Impl { .. } => impls.push(impl_record(tcx, id)),
                DefKind::Fn | DefKind::AssocFn => functions.push(json!({
                    "id": identity(tcx, id), "qualified_name": tcx.def_path_str(id),
                    "origin": origin(tcx, id),
                })),
                _ => {}
            }
        }
        for rows in [&mut types, &mut impls, &mut functions] {
            rows.sort_by_key(|row| row["id"].as_str().unwrap_or_default().to_string());
        }
        let mut cfg = tcx
            .sess
            .psess
            .config
            .iter()
            .map(|(key, value)| {
                value.map_or_else(|| key.to_string(), |value| format!("{key}={value:?}"))
            })
            .collect::<Vec<_>>();
        cfg.sort();
        let test = cfg.iter().any(|entry| entry == "test");
        let manifest = fs::canonicalize(
            env::var_os("CARGO_MANIFEST_DIR").ok_or("Cargo manifest directory missing")?,
        )?;
        let package_name = env::var("CARGO_PKG_NAME")?;
        let mut record = json!({
            "schema_version": 1, "backend": "rustc-hir",
            "compiler_version": env!("RQLENS_DRIVER_VERSION"),
            "package_name": package_name,
            "manifest_dir": manifest, "source_root": self.source,
            "crate_name": tcx.crate_name(LOCAL_CRATE).to_string(),
            "target_triple": tcx.sess.opts.target_triple.to_string(),
            "test": test, "cfg": cfg,
            "types": types, "impls": impls, "functions": functions,
        });
        if env::var("RQLENS_COMPILER_ARCHITECTURE").as_deref() == Ok("1") {
            record["architecture"] = architecture::capture(tcx);
        }
        // One atomic file per invocation, never interleaved Cargo-process output.
        let path = self
            .destination
            .join(format!("{}.json", std::process::id()));
        assert!(!path.exists(), "duplicate compiler capture filename");
        let temp = path.with_extension("tmp");
        fs::write(&temp, serde_json::to_vec(&record)?)?;
        fs::rename(temp, path)?;
        Ok(())
    }
}

fn identity(tcx: TyCtxt<'_>, id: DefId) -> String {
    format!("{:?}", tcx.def_path_hash(id))
}

fn origin(tcx: TyCtxt<'_>, id: DefId) -> Value {
    origin_at(tcx, id, tcx.def_span(id))
}

fn origin_at(tcx: TyCtxt<'_>, id: DefId, span: rustc_span::Span) -> Value {
    // include! and attribute passthrough can retain root-context token spans.
    // The resolver's definition expansion, not span hygiene alone, is authoritative.
    let mut expansion = tcx.expn_that_defined(id);
    let generated = expansion != rustc_span::ExpnId::root() || span.from_expansion();
    let mut anchor = span.source_callsite();
    while expansion != rustc_span::ExpnId::root() {
        let data = expansion.expn_data();
        anchor = data.call_site.source_callsite();
        expansion = data.parent;
    }
    let location = if anchor.is_dummy() {
        Value::Null
    } else {
        let map = tcx.sess.source_map();
        let start = map.lookup_char_pos(anchor.lo());
        let end = map.lookup_char_pos(anchor.hi());
        match start.file.name.clone().into_local_path() {
            Some(path) if start.file.name == end.file.name => {
                let path = fs::canonicalize(&path).unwrap_or(path);
                if env::var_os("OUT_DIR").is_some_and(|out| path.starts_with(out)) {
                    return json!({"generated": true, "location_kind": "unavailable", "location": null});
                }
                json!({ "path": path, "line": start.line, "column": start.col.0 + 1,
                    "end_line": end.line, "end_column": end.col.0 + 1 })
            }
            _ => Value::Null,
        }
    };
    json!({"generated": generated, "location_kind": if location.is_null() { "unavailable" } else if generated { "invocation_anchor" } else { "authored" }, "location": location})
}

fn type_record(tcx: TyCtxt<'_>, id: DefId) -> Value {
    let adt = tcx.adt_def(id);
    let kind = if adt.is_enum() {
        "enum"
    } else if adt.is_union() {
        "union"
    } else {
        "struct"
    };
    json!({
        "id": identity(tcx, id), "qualified_name": tcx.def_path_str(id), "name": tcx.item_name(id).to_string(),
        "kind": kind, "field_count": if adt.is_enum() { 0 } else { adt.non_enum_variant().fields.len() },
        "variant_count": if adt.is_enum() { adt.variants().len() } else { 0 },
        "variant_field_count": if adt.is_enum() { adt.variants().iter().map(|variant| variant.fields.len()).sum::<usize>() } else { 0 },
        "origin": origin(tcx, id),
    })
}

fn impl_record(tcx: TyCtxt<'_>, id: DefId) -> Value {
    let owner = tcx.type_of(id).instantiate_identity();
    let owner_id = match owner.kind() {
        ty::Adt(adt, _) if adt.did().is_local() => Some(identity(tcx, adt.did())),
        _ => None,
    };
    let mut methods = tcx
        .associated_items(id)
        .in_definition_order()
        .filter(|item| matches!(item.kind, ty::AssocKind::Fn { .. }))
        .map(|item| identity(tcx, item.def_id))
        .collect::<Vec<_>>();
    methods.sort();
    json!({
        "id": identity(tcx, id), "owner_id": owner_id, "owner_display": owner.to_string(),
        "owner_kind": if owner_id.is_some() { "local_adt" } else { "external_or_non_nominal" },
        "method_ids": methods, "origin": origin(tcx, id),
    })
}
