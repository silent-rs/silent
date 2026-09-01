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

    #[test]
    fn hashes_and_verifies_password() {
        let password = "hello_password".to_string();
        let password_hash = make_password(password.clone()).unwrap();

        assert!(verify_password(password_hash, password).unwrap());
    }

    #[test]
    fn rejects_incorrect_password() {
        let password_hash = make_password("hello_password".to_string()).unwrap();

        assert!(!verify_password(password_hash, "incorrect_password".to_string()).unwrap());
    }

    #[test]
    fn generates_unique_hashes() {
        let password = "hello_password".to_string();

        assert_ne!(
            make_password(password.clone()).unwrap(),
            make_password(password).unwrap()
        );
    }

    #[test]
    fn verifies_existing_password_hash() {
        let password_hash = "$argon2id$v=19$m=19456,t=2,p=1$MDEyMzQ1Njc4OWFiY2RlZg$2PVUkrAGPo73NX+uUQvkZZi7VQPe4YUB3cxt/JBXmKc";

        assert!(verify_password(password_hash.to_string(), "hello_password".to_string()).unwrap());
    }
}
