#[macro_export]
macro_rules! bail_into {
    ($($arg:tt)+) => {
        return Err(anyhow::anyhow!($($arg)*).into());
    };
}
#[macro_export]
macro_rules! ensure_into {
    ($cond:expr, $($arg:tt)+) => {
        if !$cond {
            $crate::bail_into!($($arg)*)
        }
    };
}
