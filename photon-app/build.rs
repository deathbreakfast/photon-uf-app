//! Valence model codegen for `schemas/*_valence_schema.rs` (server builds only).

fn main() {
    println!("cargo:rerun-if-changed=schemas/");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_SSR");
    // The reveal log model is server-side only; the browser build doesn't compile it.
    if std::env::var_os("CARGO_FEATURE_SSR").is_none() {
        return;
    }
    let Ok(out_dir) = std::env::var("OUT_DIR") else {
        std::process::exit(1);
    };
    if let Err(error) = valence_codegen::generate_models(&valence_codegen::CodegenConfig {
        schemas_dir: std::path::PathBuf::from("schemas"),
        out_dir: std::path::PathBuf::from(out_dir),
        file_suffix: "_valence_schema.rs",
        trait_file_suffix: "_valence_trait.rs",
    }) {
        panic!("valence codegen failed: {error}");
    }
}
