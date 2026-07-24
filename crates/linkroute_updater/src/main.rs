mod manifest;

use manifest::UpdateManifest;

fn main() {
    let Some(command) = std::env::args().nth(1) else {
        eprintln!("usage: linkroute-updater [--parse-manifest <path>|--verify-manifest <path> <public-key-hex>|--version]");
        std::process::exit(2);
    };

    match command.as_str() {
        "--parse-manifest" => parse_manifest(),
        "--verify-manifest" => verify_manifest(),
        "--version" => println!("linkroute-updater {}", env!("CARGO_PKG_VERSION")),
        other => {
            eprintln!("unknown command: {other}");
            std::process::exit(2);
        }
    }
}

fn parse_manifest() {
    let manifest = load_manifest_arg();
    println!(
        "manifest ok: version={}, url={}, sha256={}",
        manifest.version, manifest.url, manifest.sha256
    );
}

fn verify_manifest() {
    let manifest = load_manifest_arg();
    let Some(public_key_hex) = std::env::args().nth(3) else {
        eprintln!("missing public key hex");
        std::process::exit(2);
    };

    match manifest.verify_signature(&public_key_hex) {
        Ok(()) => println!("manifest signature ok"),
        Err(error) => {
            eprintln!("manifest signature invalid: {error}");
            std::process::exit(1);
        }
    }
}

fn load_manifest_arg() -> UpdateManifest {
    let Some(path) = std::env::args().nth(2) else {
        eprintln!("missing manifest path");
        std::process::exit(2);
    };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("could not read manifest: {error}");
            std::process::exit(1);
        }
    };

    match UpdateManifest::parse(&text) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("manifest invalid: {error}");
            std::process::exit(1);
        }
    }
}
