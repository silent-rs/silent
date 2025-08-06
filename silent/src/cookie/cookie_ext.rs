use crate::{Request, Response};
use cookie::{Cookie, CookieJar};
use http_body::Body;

pub trait CookieExt {
    /// Get `CookieJar` reference.
    fn cookies(&self) -> CookieJar;
    /// Get `CookieJar` mutable reference.
    fn cookies_mut(&mut self) -> &mut CookieJar;
    /// Get `Cookie` from cookies.
    fn cookie<T: AsRef<str>>(&self, name: T) -> Option<&Cookie<'static>>;
}

impl CookieExt for Request {
    fn cookies(&self) -> CookieJar {
        self.extensions().get().cloned().unwrap_or_default()
    }

    fn cookies_mut(&mut self) -> &mut CookieJar {
        if self.extensions_mut().get::<CookieJar>().is_none() {
            self.extensions_mut().insert(CookieJar::new());
        }
        self.extensions_mut().get_mut().unwrap()
    }

    fn cookie<T: AsRef<str>>(&self, name: T) -> Option<&Cookie<'static>> {
        self.extensions().get::<CookieJar>()?.get(name.as_ref())
    }
}

impl<B: Body> CookieExt for Response<B> {
    fn cookies(&self) -> CookieJar {
        self.extensions().get().cloned().unwrap_or_default()
    }

    fn cookies_mut(&mut self) -> &mut CookieJar {
        if self.extensions_mut().get::<CookieJar>().is_none() {
            self.extensions_mut().insert(CookieJar::new());
        }
        self.extensions_mut().get_mut().unwrap()
    }

    fn cookie<T: AsRef<str>>(&self, name: T) -> Option<&Cookie<'static>> {
        self.extensions().get::<CookieJar>()?.get(name.as_ref())
    }
}
