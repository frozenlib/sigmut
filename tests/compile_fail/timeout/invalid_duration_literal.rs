use sigmut::utils::timer::timeout;

#[timeout("10xs")]
async fn bad_timeout() -> Result<u32, sigmut::utils::timer::TimeoutError> {
    Ok(1)
}

fn main() {}
