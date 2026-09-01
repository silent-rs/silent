use crate::{Result, SilentError, StatusCode};
use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};

pub fn make_password(password: String) -> Result<String> {
    Ok(Argon2::default()
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
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

#[cfg(test)]
mod test {
    use super::*;

    fn test_password() -> String {
        scru128::new_string()
    }

    #[test]
    fn hashes_and_verifies_password() {
        let password = test_password();
        let password_hash = make_password(password.clone()).unwrap();

        assert!(verify_password(password_hash, password).unwrap());
    }

    #[test]
    fn rejects_incorrect_password() {
        let password_hash = make_password(test_password()).unwrap();

        assert!(!verify_password(password_hash, test_password()).unwrap());
    }

    #[test]
    fn generates_unique_hashes() {
        let password = test_password();

        assert_ne!(
            make_password(password.clone()).unwrap(),
            make_password(password).unwrap()
        );
    }
}
