use crate::{Result, SilentError, StatusCode};
use pbkdf2::Pbkdf2;
use pbkdf2::password_hash::{PasswordHasher, PasswordVerifier};
use pbkdf2::phc::PasswordHash;

pub fn make_password(password: String) -> Result<String> {
    Ok(Pbkdf2::default()
        .hash_password(password.as_bytes())
        .map_err(|e| {
            SilentError::business_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("make password failed: {e}"),
            )
        })?
        .to_string())
}

pub fn verify_password(password_hash: String, password: String) -> Result<bool> {
    let parsed_hash = PasswordHash::new(&password_hash).map_err(|e| {
        SilentError::business_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("read password hash failed: {e}"),
        )
    })?;
    Ok(Pbkdf2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

#[cfg(test)]
mod test {
    use super::*;
    use tracing::info;

    #[test]
    fn hash_test() {
        let password = "hello_password".to_string();
        let password_hash = make_password(password.clone()).unwrap();
        info!("{}", password_hash);
        assert!(verify_password(password_hash, password).is_ok())
    }
}
