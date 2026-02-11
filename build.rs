#[cfg(feature = "viz-3d")]
use std::process::Command;

fn main() {
    // Only build WASM viz if viz-3d feature is enabled
    #[cfg(feature = "viz-3d")]
    build_wasm_viz();
}

#[cfg(feature = "viz-3d")]
fn build_wasm_viz() {
    println!("cargo:rerun-if-changed=swarf-viz-wasm/src");
    println!("cargo:rerun-if-changed=swarf-viz-wasm/Cargo.toml");

    // Check if wasm-pack is installed
    let wasm_pack_check = Command::new("wasm-pack")
        .arg("--version")
        .output();

    if wasm_pack_check.is_err() {
        println!("cargo:warning=wasm-pack not found. Install with: cargo install wasm-pack");
        println!("cargo:warning=Skipping 3D viz build");
        return;
    }

    // Build the WASM package
    let output = Command::new("wasm-pack")
        .args(&["build", "--target", "web", "--out-dir", "pkg"])
        .current_dir("swarf-viz-wasm")
        .output()
        .expect("Failed to build WASM viz");

    if !output.status.success() {
        println!("cargo:warning=WASM build failed:");
        println!("cargo:warning={}", String::from_utf8_lossy(&output.stderr));
    } else {
        println!("cargo:warning=3D WASM viz built successfully");
    }
}
