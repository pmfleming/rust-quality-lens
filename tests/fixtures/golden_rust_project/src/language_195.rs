#[repr(u128)]
pub enum WideTag {
    Zero = 0,
    High = u128::MAX,
}

fn initialized<const N: usize>() -> [u8; N] {
    [0; N]
}

pub fn inferred_const_argument() -> [u8; 4] {
    initialized::<_>()
}

pub fn guarded_value(input: Option<i32>, limit: Result<i32, ()>) -> i32 {
    match input {
        Some(value) if let Ok(max) = limit && value < max => value,
        _ => 0,
    }
}

unsafe extern "system" {
    pub fn platform_variadic(format: *const i8, ...) -> i32;
}
