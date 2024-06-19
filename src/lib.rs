pub mod bar;
pub mod conf;
pub mod control;
pub mod tracing;

#[macro_export]
macro_rules! NAME {
    () => {
        env!("CARGO_CRATE_NAME")
    };
}
