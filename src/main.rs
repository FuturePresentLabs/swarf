#![allow(dead_code)]
#![allow(clippy::upper_case_acronyms)]

mod ast;
mod lexer;
mod parser;
mod codegen;
mod validator;
pub mod black_book;
pub mod post;

#[cfg(feature = "viz")]
mod viz;

use std::fs;

#[derive(Debug)]
enum Error {
    Io(std::io::Error),
    Parse(parser::ParseError),
    Validation(Vec<validator::ValidationError>),
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
                    eprintln!("Usage: swarf --viz [--2d] <gcode-file.nc>");
                    eprintln!("  --2d  Use 2D canvas view (default is 3D if available)");
                    std::process::exit(1);
                }
                
                // Check for --2d flag
                let use_2d = args.iter().any(|a| a == "--2d");
                let file_arg = if args[2] == "--2d" { args[3].clone() } else { args[2].clone() };
                
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(viz::runviz(file_arg, use_2d));
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
            println!("  mach3     - Mach3/Mach4 (expands canned cycles)");
            println!("  linuxcnc  - LinuxCNC");
            println!("  haas      - Haas");
        }
        _ => {
            // Parse options
            let mut post_type = post::PostProcessorType::Generic;
            let mut input_path = None;
            let mut output_path = "output.nc";

            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--post" | "-p" => {
                        if i + 1 < args.len() {
                            post_type = match args[i + 1].as_str() {
                                "mach3" => post::PostProcessorType::Mach3,
                                "linuxcnc" => post::PostProcessorType::LinuxCNC,
                                "haas" => post::PostProcessorType::Haas,
                                _ => post::PostProcessorType::Generic,
                            };
                            i += 2;
                        } else {
                            eprintln!("Error: --post requires an argument (mach3, linuxcnc, haas)");
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
                        if input_path.is_none() && !arg.starts_with('-') {
                            input_path = Some(arg);
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

            if let Err(e) = compile_with_post(input_path, output_path, post_type) {
                eprintln!("Error: {:?}", e);
                std::process::exit(1);
            }
        }
    }
}

fn print_usage() {
    println!("swarf - Natural language to G-code compiler");
    println!();
    println!("Usage:");
    println!("  swarf <input.dsl> [output.nc]          Compile DSL to G-code");
    println!("  swarf <input.dsl> --post <type>        Use post-processor");
    println!("  swarf --viz <file.nc>                  Start visualizer on http://localhost:3030");
    println!("  swarf --list-posts                     List available post-processors");
    println!("  swarf --help                           Show this help");
    println!();
    println!("Post-processors:");
    println!("  generic   - Fanuc-compatible (default)");
    println!("  mach3     - Mach3/Mach4 (expands canned cycles)");
    println!("  linuxcnc  - LinuxCNC");
    println!("  haas      - Haas");
    println!();
    println!("Examples:");
    println!("  swarf program.dsl output.nc");
    println!("  swarf program.dsl --post mach3 -o output.nc");
    println!("  swarf examples/bracket.dsl");
    println!("  swarf --viz output.nc");
}

fn compile(input_path: &str, output_path: &str) -> Result<(), Error> {
    compile_with_post(input_path, output_path, post::PostProcessorType::Generic)
}

fn compile_with_post(input_path: &str, output_path: &str, post_type: post::PostProcessorType) -> Result<(), Error> {
    // Read input
    let source = fs::read_to_string(input_path)?;

    // Lex
    let tokens = lexer::lex(&source);

    // Parse
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse()?;

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
    let mut codegen = codegen::CodeGenerator::new();
    let gcode_output = codegen.generate_output(&program);

    // Apply post-processor
    let processor = post_type.get_processor();
    let final_output = processor.process(&gcode_output);
    let gcode = final_output.to_string();

    // Write output
    fs::write(output_path, gcode)?;

    println!("Generated: {} (using {} post-processor)", output_path, processor.name());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        validator.validate_program(&program).expect("validation failed");
        
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
        validator.validate_program(&program).expect("validation failed");
        
        let mut codegen = codegen::CodeGenerator::new();
        let gcode = codegen.generate(&program);
        
        println!("Generated G-code:\n{}", gcode);
        assert!(gcode.contains("G20")); // Imperial units
        assert!(gcode.contains("M30")); // Program end
    }
}
