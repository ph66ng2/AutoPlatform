use crate::secret::Secret;

#[derive(Debug)]
pub struct WorkshopOAuth {
    access_token: Secret,
    refresh_token: Secret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopView {
    pub authorized: bool,
}

impl WorkshopOAuth {
    #[must_use]
    pub fn exchange(access: impl Into<String>, refresh: impl Into<String>) -> Self {
        Self {
            access_token: Secret::new(access.into()),
            refresh_token: Secret::new(refresh.into()),
        }
    }

    #[must_use]
    pub fn desktop_view(&self) -> DesktopView {
        DesktopView {
            authorized: self.access_token.is_present() && self.refresh_token.is_present(),
        }
    }
}
