#![allow(dead_code)]
#![allow(clippy::upper_case_acronyms)]

mod ast;
pub mod black_book;
mod codegen;
mod entry;
mod lexer;
pub mod mesh;
mod parser;
pub mod post;
mod tool_library;
mod validator;

#[cfg(feature = "viz")]
mod viz;

use std::fs;

#[derive(Debug)]
enum Error {
    Io(std::io::Error),
    Parse(parser::ParseError),
    Post(post::Error),
    Validation(Vec<validator::ValidationError>),
}

impl From<post::Error> for Error {
    fn from(e: post::Error) -> Self {
        Error::Post(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<parser::ParseError> for Error {
    fn from(e: parser::ParseError) -> Self {
        Error::Parse(e)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    let command = &args[1];

    match command.as_str() {
        "--viz" | "viz" => {
            #[cfg(feature = "viz")]
            {
                if args.len() < 3 {
                    eprintln!("Usage: swarf --viz [--2d] [--png <output.png>] <path>");
                    eprintln!("  <path>        G-code file (.nc), swarf file (.swarf), or folder");
                    eprintln!("  --2d          Use 2D canvas view (default is 3D if available)");
                    eprintln!("  --png <file>  Export to PNG file instead of starting server");
                    eprintln!();
                    eprintln!("Examples:");
                    eprintln!("  swarf --viz output.nc           # View G-code");
                    eprintln!(
                        "  swarf --viz part.swarf          # View swarf file with live reload"
                    );
                    eprintln!(
                        "  swarf --viz examples/           # Browse all .swarf files in folder"
                    );
                    std::process::exit(1);
                }

                // Check for --2d flag
                let use_2d = args.iter().any(|a| a == "--2d");

                // Check for --png flag
                let png_output = args
                    .iter()
                    .position(|a| a == "--png")
                    .and_then(|i| args.get(i + 1));

                // Find the file argument (first non-flag argument after command)
                let file_arg = args
                    .iter()
                    .skip(2)
                    .find(|a| {
                        if a.starts_with("--") {
                            return false;
                        }
                        // Skip the value after --png
                        if let Some(png_idx) = args.iter().position(|x| x == "--png") {
                            if args.get(png_idx + 1) == Some(a) {
                                return false;
                            }
                        }
                        true
                    })
                    .cloned();

                let file_arg = match file_arg {
                    Some(f) => f,
                    None => {
                        eprintln!("Error: No G-code file specified");
                        std::process::exit(1);
                    }
                };

                if let Some(output) = png_output {
                    // Export to PNG
                    if let Err(e) = viz::export_to_png(&file_arg, output, 800, 600) {
                        eprintln!("Error exporting PNG: {}", e);
                        std::process::exit(1);
                    }
                } else {
                    // Start viz server
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(viz::runviz(file_arg, use_2d));
                }
            }
            #[cfg(not(feature = "viz"))]
            {
                eprintln!("viz feature not enabled. Build with: cargo build --features viz");
                std::process::exit(1);
            }
        }
        "--help" | "-h" | "help" => {
            print_usage();
        }
        "--list-posts" => {
            println!("Available post-processors:");
            println!("  generic   - Fanuc-compatible (default)");
            print_mach_posts();
            println!("  linuxcnc  - LinuxCNC");
            println!("  haas      - Haas");
        }
        "--post-capabilities" => {
            let result = args
                .get(2)
                .ok_or_else(|| "--post-capabilities requires a target".to_string())
                .and_then(|name| post::PostProcessorType::parse(name))
                .and_then(|target| {
                    target.capabilities().ok_or_else(|| {
                        "capability declaration currently available for Mach mill profiles only"
                            .into()
                    })
                });
            match result {
                Ok(caps) => println!("{}", serde_json::to_string_pretty(&caps).unwrap()),
                Err(error) => {
                    eprintln!("Error: {error}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            // Parse options
            let mut post_type = post::PostProcessorType::Generic;
            let mut input_path = None;
            let mut output_path = "output.nc";
            let mut max_rpm: Option<f64> = None;
            let mut tools_path: Option<String> = None;
            let mut stl_path: Option<String> = None;
            let mut stl_voxel: f64 = 0.05;

            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--post" | "-p" => {
                        if i + 1 < args.len() {
                            post_type = post::PostProcessorType::parse(&args[i + 1])
                                .unwrap_or_else(|error| {
                                    eprintln!("Error: {error}");
                                    std::process::exit(1);
                                });
                            i += 2;
                        } else {
                            eprintln!("Error: --post requires a target; see --list-posts");
                            std::process::exit(1);
                        }
                    }
                    "--tools" => {
                        if i + 1 < args.len() {
                            tools_path = Some(args[i + 1].clone());
                            i += 2;
                        } else {
                            eprintln!("Error: --tools requires a path to tools.json");
                            std::process::exit(1);
                        }
                    }
                    "--max-rpm" => {
                        if i + 1 < args.len() {
                            max_rpm = args[i + 1].parse().ok();
                            if max_rpm.is_none() {
                                eprintln!("Error: --max-rpm requires a valid number");
                                std::process::exit(1);
                            }
                            i += 2;
                        } else {
                            eprintln!("Error: --max-rpm requires an argument (e.g., 10000)");
                            std::process::exit(1);
                        }
                    }
                    "--stl" => {
                        if i + 1 < args.len() {
                            stl_path = Some(args[i + 1].clone());
                            i += 2;
                        } else {
                            eprintln!("Error: --stl requires an output path (e.g. part.stl)");
                            std::process::exit(1);
                        }
                    }
                    "--stl-voxel" => {
                        if i + 1 < args.len() {
                            stl_voxel = args[i + 1].parse().unwrap_or_else(|_| {
                                eprintln!("Error: --stl-voxel requires a valid number");
                                std::process::exit(1);
                            });
                            i += 2;
                        } else {
                            eprintln!("Error: --stl-voxel requires a size value");
                            std::process::exit(1);
                        }
                    }
                    "-o" => {
                        if i + 1 < args.len() {
                            output_path = &args[i + 1];
                            i += 2;
                        } else {
                            eprintln!("Error: -o requires an output path");
                            std::process::exit(1);
                        }
                    }
                    arg => {
                        if !arg.starts_with('-') {
                            if input_path.is_none() {
                                input_path = Some(arg);
                            } else if output_path == "output.nc" {
                                // Second positional argument is output path
                                output_path = arg;
                            }
                        }
                        i += 1;
                    }
                }
            }

            let input_path = input_path.unwrap_or_else(|| {
                eprintln!("Error: No input file specified");
                print_usage();
                std::process::exit(1);
            });

            // Load tool library if specified
            let tool_library = if let Some(path) = tools_path {
                match tool_library::ToolLibrary::from_file(&path) {
                    Ok(lib) => {
                        println!("Loaded {} tools from {}", lib.tools.len(), path);
                        Some(lib)
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to load tool library: {}", e);
                        None
                    }
                }
            } else {
                None
            };

            if let Err(e) = compile_with_post_and_tools(
                input_path,
                output_path,
                post_type,
                max_rpm,
                tool_library,
            ) {
                eprintln!("Error: {:?}", e);
                std::process::exit(1);
            }

            // STL export (runs after successful G-code generation)
            if let Some(stl_out) = stl_path {
                match generate_stl(input_path, &stl_out, stl_voxel) {
                    Ok(()) => {}
                    Err(e) => {
                        eprintln!("STL error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}

fn print_mach_posts() {
    println!("  mach3 / mach3-mill - Mach3 XYZ mill, dwell seconds, expanded G81/G82/G83");
    println!("  mach3-mill-ms     - Mach3 XYZ mill configured for millisecond dwell");
    println!("  mach4 / mach4-mill - Mach4 XYZ mill, dwell seconds, expanded G81/G82/G83");
}

fn print_usage() {
    println!("swarf - Natural language to G-code compiler");
    println!();
    println!("Usage:");
    println!("  swarf <input.swarf> [output.nc]        Compile swarf to G-code");
    println!("  swarf <input.swarf> --post <type>      Use post-processor");
    println!("  swarf <input.swarf> --max-rpm <rpm>    Limit spindle RPM (scales feed)");
    println!("  swarf --tools <file> <input.swarf>     Use tool library JSON");
    println!("  swarf --viz <path>                     Start visualizer on http://localhost:3030");
    println!("  swarf --list-posts                     List available post-processors");
    println!("  swarf --post-capabilities <target>     Print Mach mill capability JSON");
    println!("  swarf --help                           Show this help");
    println!();
    println!("Post-processors:");
    println!("  generic   - Fanuc-compatible (default)");
    print_mach_posts();
    println!("  linuxcnc  - LinuxCNC");
    println!("  haas      - Haas");
    println!();
    println!("Tool Library:");
    println!("  swarf --tools tools.json part.swarf    Reference tools by ID or name");
    println!("  In swarf: tool 1  or  tool \"3/8 EM\"");
    println!();
    println!("Visualizer:");
    println!("  swarf --viz output.nc                  View G-code file");
    println!("  swarf --viz part.swarf                 View swarf file (live reload)");
    println!("  swarf --viz examples/                  Browse folder of .swarf files");
    println!();
    println!("Examples:");
    println!("  swarf program.swarf output.nc");
    println!("  swarf program.swarf --post mach3 -o output.nc");
    println!("  swarf program.swarf --tools tools.json -o output.nc");
    println!("  swarf program.swarf --max-rpm 10000 -o output.nc");
    println!("  swarf examples/bracket.swarf");
    println!("  swarf --viz examples/");
}

fn generate_stl(input_path: &str, stl_path: &str, voxel_size: f64) -> Result<(), String> {
    let source = fs::read_to_string(input_path).map_err(|e| e.to_string())?;
    let tokens = lexer::lex(&source);
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse().map_err(|e| e.to_string())?;

    let (final_mesh, snapshots) = mesh::generate_from_program(&program, voxel_size)?;

    // Write per-op snapshots as <base>_op<N>.stl
    let base = stl_path.trim_end_matches(".stl");
    for (i, snap) in snapshots.iter().enumerate() {
        let snap_path = format!("{}_op{}.stl", base, i);
        snap.mesh.write_stl(&snap_path).map_err(|e| e.to_string())?;
        println!(
            "  [op {}] {} → {} ({} triangles)",
            i,
            snap.label,
            snap_path,
            snap.mesh.triangle_count()
        );
    }

    // Write final mesh
    final_mesh.write_stl(stl_path).map_err(|e| e.to_string())?;
    println!(
        "STL: {} ({} triangles, voxel {:.4})",
        stl_path,
        final_mesh.triangle_count(),
        voxel_size
    );

    Ok(())
}

fn compile(input_path: &str, output_path: &str) -> Result<(), Error> {
    compile_with_post_and_tools(
        input_path,
        output_path,
        post::PostProcessorType::Generic,
        None,
        None,
    )
}

fn compile_with_post(
    input_path: &str,
    output_path: &str,
    post_type: post::PostProcessorType,
    max_rpm: Option<f64>,
) -> Result<(), Error> {
    compile_with_post_and_tools(input_path, output_path, post_type, max_rpm, None)
}

fn compile_with_post_and_tools(
    input_path: &str,
    output_path: &str,
    post_type: post::PostProcessorType,
    max_rpm: Option<f64>,
    tool_library: Option<tool_library::ToolLibrary>,
) -> Result<(), Error> {
    // Read input
    let source = fs::read_to_string(input_path)?;

    // Lex
    let tokens = lexer::lex(&source);

    // Parse
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse()?;

    // Resolve tool references from library
    let program = if let Some(ref lib) = tool_library {
        resolve_tools(program, lib)
    } else {
        program
    };

    // Validate
    let validator = validator::Validator::new();
    if let Err(errors) = validator.validate_program(&program) {
        eprintln!("Validation errors:");
        for err in errors {
            eprintln!("  - {}", err);
        }
        return Err(Error::Validation(vec![]));
    }

    // Generate G-code
    let mut codegen = if let Some(rpm) = max_rpm {
        codegen::CodeGenerator::new().with_max_rpm(rpm)
    } else {
        codegen::CodeGenerator::new()
    };

    // Pass tool library to codegen for auto-feeds/speeds
    if let Some(lib) = tool_library {
        codegen = codegen.with_tool_library(lib);
    }

    let gcode_output = codegen.generate_output(&program);

    // Apply post-processor
    let processor = post_type.get_processor();
    let final_output = processor.process(&gcode_output)?;
    let gcode = final_output.to_string();

    // Write output
    fs::write(output_path, gcode)?;

    println!(
        "Generated: {} (using {} post-processor)",
        output_path,
        processor.name()
    );

    Ok(())
}

/// Resolve tool references by looking up in tool library
fn resolve_tools(mut program: ast::Program, library: &tool_library::ToolLibrary) -> ast::Program {
    for op in &mut program.operations {
        if let ast::Operation::ToolChange(ref mut tc) = op {
            // Check if this is just a reference (no tool data) or has minimal data
            let needs_lookup = tc.tool_data.is_none() || {
                // If tool has no diameter, it needs lookup
                tc.tool_data
                    .as_ref()
                    .map(|d| d.diameter == 0.0)
                    .unwrap_or(true)
            };

            if needs_lookup {
                // First try to find by string ID if present
                let tool_def = tc
                    .tool_id
                    .as_ref()
                    .and_then(|id| library.get_by_id(id))
                    .or_else(|| {
                        // Fall back to numeric ID lookup
                        library.get_by_id(&tc.tool_number.to_string())
                    });

                if let Some(tool_def) = tool_def {
                    // Update the tool number from the library tool's numeric ID
                    tc.tool_number = tool_def.numeric_id();
                    tc.tool_data = Some(ast::ToolData {
                        diameter: tool_def.diameter,
                        length: tool_def.length.unwrap_or(0.0),
                        flutes: tool_def.flutes,
                        material: tool_def.material.to_ast_material(),
                    });
                } else {
                    let tool_ref_num = tc.tool_number.to_string();
                    let tool_ref = tc.tool_id.as_deref().unwrap_or(&tool_ref_num);
                    eprintln!("Warning: Tool '{}' not found in tool library", tool_ref);
                }
            }
        }
    }
    program
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_mach_profiles_preserves_existing_output_on_post_error() {
        let dir = std::env::temp_dir().join(format!("swarf-mach-post-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let input = dir.join("drill.swarf");
        let output = dir.join("drill.nc");
        fs::write(&input, "units metric\noffset 54\ntool 1 dia 6 length 50\nspindle cw rpm 2500\ndrill at x 10 y 20 depth 5 peck 0 feed 100\n").unwrap();
        fs::write(&output, "existing verified job").unwrap();
        let result = compile_with_post(
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            post::PostProcessorType::Mach3,
            None,
        );
        assert!(matches!(result, Err(Error::Post(_))));
        assert_eq!(
            fs::read_to_string(&output).unwrap(),
            "existing verified job"
        );
        fs::remove_file(&output).unwrap();
        let result = compile_with_post(
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            post::PostProcessorType::Mach4,
            None,
        );
        assert!(matches!(result, Err(Error::Post(_))));
        assert!(!output.exists());
        fs::write(&input, "units metric\noffset 54\ntool 1 dia 6 length 50\nspindle cw rpm 2500\ndrill at x 10 y 20 depth 5 feed 100 dwell .25\n").unwrap();
        for target in [
            post::PostProcessorType::Mach3,
            post::PostProcessorType::Mach4,
            post::PostProcessorType::Mach3Milliseconds,
        ] {
            compile_with_post(
                input.to_str().unwrap(),
                output.to_str().unwrap(),
                target,
                None,
            )
            .unwrap();
            let code = fs::read_to_string(&output).unwrap();
            assert!(code.contains("G01 Z-5.000000000 F100.000000000"));
            assert!(
                code.contains(if target == post::PostProcessorType::Mach3Milliseconds {
                    "G04 P250.000000000"
                } else {
                    "G04 P0.250000000"
                })
            );
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_drill_program() {
        let source = r#"
units metric
offset 54

tool 1 dia 6 length 50
spindle cw rpm 2500

drill at x 10 y 20 depth 5 peck 2 feed 100
"#;

        let tokens = lexer::lex(source);
        let mut parser = parser::Parser::new(tokens);
        let program = parser.parse().expect("parse failed");

        let validator = validator::Validator::new();
        validator
            .validate_program(&program)
            .expect("validation failed");

        let mut codegen = codegen::CodeGenerator::new();
        let gcode = codegen.generate(&program);

        assert!(gcode.contains("G83")); // Peck drill cycle
        assert!(gcode.contains("M30")); // Program end
    }

    #[test]
    fn test_imperial_units() {
        let source = r#"
units imperial
offset 54

tool 1 dia 0.125 length 1.0
spindle cw rpm 5000

drill at x 0.5 y 0.5 depth 0.25
"#;

        let tokens = lexer::lex(source);
        let mut parser = parser::Parser::new(tokens);
        let program = parser.parse().expect("parse failed");

        let validator = validator::Validator::new();
        validator
            .validate_program(&program)
            .expect("validation failed");

        let mut codegen = codegen::CodeGenerator::new();
        let gcode = codegen.generate(&program);

        println!("Generated G-code:\n{}", gcode);
        assert!(gcode.contains("G20")); // Imperial units
        assert!(gcode.contains("M30")); // Program end
    }
}
