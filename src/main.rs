mod ast;
mod lexer;
mod parser;
mod codegen;
mod validator;

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

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: gcode-dsl <input.dsl> [output.nc]");
        eprintln!("");
        eprintln!("Example:");
        eprintln!("  gcode-dsl program.dsl output.nc");
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = args.get(2).map(|s| s.as_str()).unwrap_or("output.nc");

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
    let gcode = codegen.generate(&program);
    
    // Write output
    fs::write(output_path, gcode)?;
    
    println!("Generated: {}", output_path);
    
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
