//! f64 math that is available without `std` (via `libm`).
#[cfg(feature = "std")]
mod imp {
    pub fn floor(x: f64) -> f64 { x.floor() }
    pub fn tan(x: f64) -> f64 { x.tan() }
    pub fn cos(x: f64) -> f64 { x.cos() }
    pub fn ln(x: f64) -> f64 { x.ln() }
    pub fn exp(x: f64) -> f64 { x.exp() }
    pub fn atan(x: f64) -> f64 { x.atan() }
    pub fn exp2(x: i32) -> f64 { 2f64.powi(x) }
}
#[cfg(not(feature = "std"))]
mod imp {
    pub fn floor(x: f64) -> f64 { libm::floor(x) }
    pub fn tan(x: f64) -> f64 { libm::tan(x) }
    pub fn cos(x: f64) -> f64 { libm::cos(x) }
    pub fn ln(x: f64) -> f64 { libm::log(x) }
    pub fn exp(x: f64) -> f64 { libm::exp(x) }
    pub fn atan(x: f64) -> f64 { libm::atan(x) }
    pub fn exp2(x: i32) -> f64 { libm::exp2(x as f64) }
}
pub use imp::*;
