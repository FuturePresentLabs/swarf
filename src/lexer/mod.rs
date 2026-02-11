use logos::Logos;

/// Tokens for the G-code DSL
/// Natural-ish language that machinists can read/write quickly

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\f]+")] // Skip whitespace
#[logos(error = LexerError)]
pub enum Token {
    // Literals
    #[regex(r"-?\d+\.?\d*", |lex| lex.slice().parse::<f64>().ok())]
    Number(Option<f64>),

    #[regex(r#""[^"]*""#, |lex| lex.slice()[1..lex.slice().len()-1].to_string())]
    String(String),

    // Keywords - Program structure
    #[token("program")]
    Program,

    #[token("units")]
    Units,

    #[token("metric")]
    Metric,

    #[token("imperial")]
    Imperial,

    #[token("offset")]
    Offset,

    // Keywords - Tools
    #[token("tool")]
    Tool,

    #[token("dia")]
    #[token("diameter")]
    Diameter,

    #[token("length")]
    Length,

    #[token("flutes")]
    Flutes,

    #[token("hss")]
    HSS,

    #[token("carbide")]
    Carbide,

    // Keywords - Spindle
    #[token("spindle")]
    Spindle,

    #[token("cw")]
    CW,

    #[token("ccw")]
    CCW,

    #[token("rpm")]
    RPM,

    // Keywords - Coolant
    #[token("coolant")]
    Coolant,

    #[token("flood")]
    Flood,

    #[token("mist")]
    Mist,

    #[token("off")]
    Off,

    // Keywords - Operations
    #[token("drill")]
    Drill,

    #[token("peck")]
    Peck,

    #[token("pocket")]
    Pocket,

    #[token("profile")]
    Profile,

    #[token("face")]
    Face,

    #[token("tap")]
    Tap,

    // Keywords - Geometry
    #[token("at")]
    At,

    #[token("rect")]
    Rect,

    #[token("rectangle")]
    Rectangle,

    #[token("circle")]
    Circle,

    #[token("radius")]
    Radius,

    #[token("width")]
    Width,

    #[token("height")]
    Height,

    #[token("depth")]
    Depth,

    #[token("corner")]
    Corner,

    #[token("corners")]
    Corners,

    #[token("center")]
    Center,

    #[token("grid")]
    Grid,

    #[token("pitch")]
    Pitch,

    // Keywords - Cut parameters
    #[token("inside")]
    Inside,

    #[token("outside")]
    Outside,

    #[token("on")]
    On,

    #[token("stepdown")]
    Stepdown,

    #[token("stepover")]
    Stepover,

    #[token("feed")]
    #[token("feedrate")]
    Feed,

    #[token("plunge")]
    Plunge,

    #[token("finish")]
    Finish,

    #[token("dwell")]
    Dwell,

    #[token("retract")]
    Retract,

    // Punctuation
    #[token("(")]
    LParen,

    #[token(")")]
    RParen,

    #[token("{")]
    LBrace,

    #[token("}")]
    RBrace,

    #[token("[")]
    LBracket,

    #[token("]")]
    RBracket,

    #[token(",")]
    Comma,

    #[token(":")]
    Colon,

    // Semicolon used for comments, not as a token
    // #[token(";")]
    // Semicolon,

    #[token("=")]
    Equals,

    #[token("x")]
    X,

    #[token("y")]
    Y,

    #[token("z")]
    Z,

    #[token("rotate")]
    Rotate,

    // Newlines for statement separation
    #[regex(r"\n\s*\n", logos::skip)] // Skip blank lines
    #[token("\n")]
    Newline,

    // Comments
    #[regex(r"//[^\n]*", logos::skip)]
    #[regex(r"/\*[^*]*\*+(?:[^/*][^*]*\*+)*/", logos::skip)]
    #[regex(r";[^\n]*", logos::skip)]
    Comment,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LexerError;

impl std::fmt::Display for LexerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lexer error")
    }
}

impl std::error::Error for LexerError {}

/// Lex the input string into tokens
pub fn lex(input: &str) -> Vec<(Token, logos::Span)> {
    Token::lexer(input)
        .spanned()
        .filter_map(|(result, span)| match result {
            Ok(token) => Some((token, span)),
            Err(_) => None, // Skip errors for now
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokens() {
        let input = "drill at x 10 y 20 depth 5";
        let tokens: Vec<_> = lex(input).into_iter().map(|(t, _)| t).collect();
        
        assert_eq!(tokens, vec![
            Token::Drill,
            Token::At,
            Token::X,
            Token::Number(Some(10.0)),
            Token::Y,
            Token::Number(Some(20.0)),
            Token::Depth,
            Token::Number(Some(5.0)),
        ]);
    }

    #[test]
    fn test_tool_definition() {
        let input = "tool 1 dia 6 length 50";
        let tokens: Vec<_> = lex(input).into_iter().map(|(t, _)| t).collect();
        
        assert_eq!(tokens, vec![
            Token::Tool,
            Token::Number(Some(1.0)),
            Token::Diameter,
            Token::Number(Some(6.0)),
            Token::Length,
            Token::Number(Some(50.0)),
        ]);
    }

    #[test]
    fn test_units() {
        let input = "units imperial";
        let tokens: Vec<_> = lex(input).into_iter().map(|(t, _)| t).collect();
        
        println!("Tokens: {:?}", tokens);
        assert_eq!(tokens, vec![
            Token::Units,
            Token::Imperial,
        ]);
    }
}
