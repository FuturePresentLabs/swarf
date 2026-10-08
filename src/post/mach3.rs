//! Separate Mach3 and Mach4 mill profiles sharing bounded cycle expansion.
use crate::codegen::GCodeOutput;
use crate::post::{mach_mill, Error, PostProcessor};

pub struct Mach3Post;
pub struct Mach3MillisecondsPost;
pub struct Mach4Post;

macro_rules! mill_post {
    ($post:ty, $name:literal, $dwell_scale:expr) => {
        impl PostProcessor for $post {
            fn process(&self, input: &GCodeOutput) -> Result<GCodeOutput, Error> {
                mach_mill::process(input, $name, $dwell_scale)
            }
            fn name(&self) -> &str {
                $name
            }
            fn supports_canned_cycles(&self) -> bool {
                false
            }
            fn supports_subroutines(&self) -> bool {
                false
            }
        }
    };
}
mill_post!(Mach3Post, "Mach3 Mill (dwell seconds)", 1.0);
mill_post!(
    Mach3MillisecondsPost,
    "Mach3 Mill (dwell milliseconds)",
    1000.0
);
mill_post!(Mach4Post, "Mach4 Mill", 1.0);
