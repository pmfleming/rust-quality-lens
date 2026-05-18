pub mod math;

pub fn public_entry(input: i32) -> i32 {
    if input > 10 {
        math::wide_branch(input)
    } else {
        math::wide_branch(input + 1)
    }
}
