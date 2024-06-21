pub mod bar;
pub mod conf;
pub mod control;
pub mod fs;
pub mod tracing;
pub mod x11;

#[macro_export]
macro_rules! NAME {
    () => {
        env!("CARGO_CRATE_NAME")
    };
}
