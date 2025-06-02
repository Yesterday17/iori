mod dash;
mod downloader;
mod hls;
mod source;

pub trait AssertWrapper {
    type Success;

    fn assert_success(self) -> Self::Success;
    fn assert_error(self);
}

impl<T, E> AssertWrapper for Result<T, E>
where
    E: std::fmt::Debug,
{
    type Success = T;

    fn assert_success(self) -> Self::Success {
        assert!(self.is_ok());

        self.unwrap()
    }

    fn assert_error(self) {
        assert!(self.is_err());
    }
}

impl<T> AssertWrapper for Option<T> {
    type Success = T;

    fn assert_success(self) -> Self::Success {
        assert!(self.is_some());
        self.unwrap()
    }

    fn assert_error(self) {
        assert!(self.is_none());
    }
}
