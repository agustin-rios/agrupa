/// Agrega una línea a un `String` (escribir en memoria no falla).
macro_rules! outln {
    ($out:expr) => {
        $out.push('\n')
    };
    ($out:expr, $($arg:tt)*) => {{
        use std::fmt::Write as _;
        let _ = writeln!($out, $($arg)*);
    }};
}

pub mod assignment;
pub mod config;
pub mod data;
pub mod model;
pub mod report;
pub mod solver;
pub mod state;
