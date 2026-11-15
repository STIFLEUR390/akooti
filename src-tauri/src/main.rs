#![forbid(unsafe_code)]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    akooti_lib::run()
}
