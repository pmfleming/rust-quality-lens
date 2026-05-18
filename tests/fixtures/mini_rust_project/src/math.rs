pub struct WideType {
    pub first: i32,
    pub second: i32,
    pub third: i32,
    pub fourth: i32,
}

pub fn wide_branch(value: i32) -> i32 {
    if value % 5 == 0 {
        value / 5
    } else if value % 3 == 0 {
        value / 3
    } else if value % 2 == 0 {
        value / 2
    } else {
        value + 1
    }
}

#[allow(dead_code)]
pub unsafe fn raw_escape(ptr: *const i32) -> i32 {
    unsafe { *ptr }
}
